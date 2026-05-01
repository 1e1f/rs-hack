//! @arch:layer(kg)
//! @arch:role(presentation)
//!
//! Per-ticket prelude assembler — `(ticket, kg, arch) -> Prelude`.
//!
//! The prelude is the cached prefix the agent runtime injects into the
//! system prompt every turn of a session. Same content across turns means
//! Anthropic's prompt cache hits at ~10% input pricing; that's what
//! `CacheControl::key` is for.
//!
//! This module is the single source of truth: the Claude Agent SDK pane
//! (R028-F3) consumes the rendered text directly, the CLAUDE.md /
//! AGENTS.md generators (R028-F6) consume the structured `sections`.
//!
//! Pure function — no I/O, no daemon access. Callers pre-fetch the KG
//! slice (via `arch.subgraph`) and pre-load the `@arch:see` doc bodies
//! before assembling. Keeps the assembler trivially testable.
//!
//! @yah:ticket(R037-F6, "@yah:agent(name) annotation: parser + prelude resolution (subclass + persona + pin + fallback) + override order; persona renders to Agent policy section, default_skills unions into skill resolver")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F14, "Authority-update propagation: enqueue ConfigSwitch on next turn for non-pinned sessions")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R037)

use crate::agent_policy::{AgentPolicyKind, AgentPolicyRule};
use crate::anno::{EngineRef, ThinkBudget, TicketStatus};
use crate::board::{Board, BoardItem};
use crate::edge::EdgeKind;
use crate::ids::NodeRef;
use crate::subgraph::Subgraph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Bounds for the assembler. `max_tokens` is a soft cap — sections drop
/// from the tail (KG slice, then arch docs) until the estimate fits.
/// Ticket and parent chain are never dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreludeOptions {
    /// Token budget for the rendered prelude. Estimated as
    /// `ceil(bytes / 4)`. Default 200K = 1 ring.
    pub max_tokens: u32,
    /// Hard cap on KG slice node count after it arrives (the daemon
    /// already bounds depth on the wire — this is a second guard).
    pub kg_node_limit: u32,
    /// `@arch:see` docs under this byte length inline as full text;
    /// longer docs render as a reference line only.
    pub arch_inline_max_bytes: usize,
}

impl Default for PreludeOptions {
    fn default() -> Self {
        Self {
            max_tokens: 200_000,
            kg_node_limit: 64,
            arch_inline_max_bytes: 16_384,
        }
    }
}

/// Inputs to [`assemble`]. All borrowed; the assembler builds an owned
/// `Prelude` without mutating its inputs.
pub struct PreludeInputs<'a> {
    pub ticket_id: &'a str,
    pub board: &'a Board,
    /// Optional KG slice rooted at the ticket's primary anchor file.
    /// Caller fetches via `arch.subgraph`; pass `None` to skip the KG
    /// section (CLI fallback, snapshot tests).
    pub kg_slice: Option<&'a Subgraph>,
    /// Map from `@arch:see` doc path -> file contents. Caller pre-loads
    /// what it has; missing entries render as a reference-only line.
    pub arch_docs: &'a BTreeMap<String, String>,
    /// Resolved skill names (R028-F5 will populate from column/tag rules).
    pub skills: &'a [String],
    /// Workspace-level agent-policy rules — those declared with
    /// `@yah:rule(agent-role|agent-do|agent-dont: ...)` outside any
    /// relay/ticket block. Always rendered, regardless of which ticket
    /// is being assembled. Caller (the daemon) collects them from the
    /// annotation index and passes them in. Empty for the unanchored /
    /// CLI-snapshot case.
    pub workspace_policy: &'a [AgentPolicyRule],
    pub options: PreludeOptions,
}

/// Structured prelude. `sections` is the canonical content; `render()`
/// concatenates them into the markdown the SDK actually sends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prelude {
    pub sections: Vec<PreludeSection>,
    pub cache: CacheControl,
    /// Engine selection from `@yah:engine(...)`, copied here so the SDK
    /// dispatch layer doesn't have to re-walk the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineRef>,
    /// Thinking budget from `@yah:think(...)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think: Option<ThinkBudget>,
    /// `ceil(rendered_bytes / 4)`.
    pub estimated_tokens: u32,
    /// `estimated_tokens / 200_000` — UI ring-depth affordance.
    pub ring_depth: f32,
    /// True when the assembler dropped sections to honour `max_tokens`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl Prelude {
    /// Concatenate sections with blank-line separators. This is the
    /// string the Claude SDK injects as the cached prefix.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&section.markdown);
            if !section.markdown.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreludeSection {
    pub kind: PreludeSectionKind,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "section", rename_all = "snake_case")]
pub enum PreludeSectionKind {
    Ticket,
    /// Resolved "Roles / Do / Don't" block from `@yah:rule(agent-...)`.
    /// Walks the parent chain plus workspace-level rules. Stable across
    /// turns and never trimmed — policy is the floor an agent runs on.
    AgentPolicy,
    ParentChain,
    KgSlice,
    ArchDoc { path: String, inline: bool },
    Skills,
    /// Top-level header for an unanchored or non-ticket session
    /// (rig-only chat, future arch-doc chat). Lets the budget trimmer
    /// distinguish "always keep" headers from droppable context.
    Chat,
    /// Stable yah:// link convention block — same content for every
    /// session, so chat- and ticket-mode share the body and the renderer
    /// can route the resulting markdown links into the Arch tab.
    OutputConventions,
}

/// Body of the yah:// output-conventions stanza. Used by both the
/// ticket-anchored prelude assembler (as its own section) and the
/// chat-mode prelude in `app/tauri/src/agent.rs::build_chat_prelude`,
/// so the agent sees one uniform convention regardless of mode.
pub const OUTPUT_CONVENTIONS_BODY: &str = "## Output conventions

When you reference a file, function, or symbol the user might want to jump to, prefer markdown links with the `yah://` scheme over bare paths:

- `[path/to/file.rs:42](yah://file/path/to/file.rs#L42)` — opens the file in the Architecture tab rooted at that line.
- `[Foo](yah://arch/symbol/Foo)` — re-roots the arch graph on the named symbol.

The renderer turns these into clickable affordances; bare backticked `path:line` chips also work but yah:// links are preferred for prose.

## Tool-call honesty

Do not invent, omit, or rewrite your own tool history when asked about it.

- If you retried a call (e.g. one tool failed and you fell back to another), say so plainly. Repeated calls are normal — pretending they didn't happen is not.
- If a call returned an error or `ok: false`, do not describe its result as a success. The user can see the failure on their side.
- If you don't have visibility into your earlier tool calls in the current context, say \"I don't have a reliable record of my prior tool calls in this turn\" rather than guessing.
- Each tool result begins with a one-line `_smell` summary (e.g. `read_file path · 4.6KB · ok`). When recounting what you did, you may quote that line — do not fabricate one.";

/// Build the shared yah:// output-conventions section. Always-on,
/// stable bytes → cache-friendly. Sits at the tail of the assembled
/// prelude (after Skills) so per-ticket sections own the high-churn
/// portion of the cache key.
pub fn output_conventions_section() -> PreludeSection {
    PreludeSection {
        kind: PreludeSectionKind::OutputConventions,
        markdown: OUTPUT_CONVENTIONS_BODY.to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    /// Stable 32-char hex hash over the rendered prelude. Same content
    /// = same key = Anthropic prompt cache hit.
    pub key: String,
    pub ttl: CacheTtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheTtl {
    /// Anthropic default: 5-minute TTL.
    Ephemeral,
    /// Anthropic extended: 1-hour TTL.
    Extended,
}

/// Build a prelude for `inputs.ticket_id`. Returns `None` when no item
/// in the board carries that id — same null-when-missing convention as
/// `arch.get_ticket` and `arch.ticket_prompt`.
pub fn assemble(inputs: &PreludeInputs<'_>) -> Option<Prelude> {
    let item = inputs.board.get(inputs.ticket_id)?;

    let mut sections: Vec<PreludeSection> = Vec::new();
    sections.push(render_ticket_section(item));

    let chain = build_parent_chain(inputs.board, item);

    let resolved_policy = resolve_agent_policy(item, &chain, inputs.workspace_policy);
    if !resolved_policy.is_empty() {
        sections.push(render_agent_policy_section(&resolved_policy));
    }

    if !chain.is_empty() {
        sections.push(render_parent_chain_section(&chain));
    }

    if let Some(kg) = inputs.kg_slice {
        if !kg.nodes.is_empty() {
            sections.push(render_kg_slice_section(kg, inputs.options.kg_node_limit));
        }
    }

    for path in &item.item.anno.see_also {
        let body = inputs.arch_docs.get(path);
        let inline =
            body.map(|b| b.len() <= inputs.options.arch_inline_max_bytes).unwrap_or(false);
        sections.push(render_arch_doc_section(path, body, inline));
    }

    if !inputs.skills.is_empty() {
        sections.push(render_skills_section(inputs.skills));
    }

    sections.push(output_conventions_section());

    let truncated = trim_to_budget(&mut sections, inputs.options.max_tokens);

    let combined = render_combined(&sections);
    let estimated_tokens = ((combined.len() as f32) / 4.0).ceil() as u32;
    let ring_depth = (estimated_tokens as f32) / 200_000.0;
    let cache_key = compute_cache_key(&combined);

    Some(Prelude {
        sections,
        cache: CacheControl { key: cache_key, ttl: CacheTtl::Ephemeral },
        engine: item.item.anno.engine.clone(),
        think: item.item.anno.think,
        estimated_tokens,
        ring_depth,
        truncated,
    })
}

fn render_ticket_section(item: &BoardItem) -> PreludeSection {
    let anno = &item.item.anno;
    let mut s = String::new();
    let _ = writeln!(s, "# Ticket: {} — {}", item.item.id, anno.title);
    s.push('\n');
    let _ = writeln!(s, "- **id**: `{}`", item.item.id);
    if let Some(ref kind) = anno.kind {
        let _ = writeln!(s, "- **kind**: {}", kind);
    }
    let _ = writeln!(s, "- **status**: {}", status_str(anno.status));
    if let Some(ref phase) = anno.phase {
        let _ = writeln!(s, "- **phase**: {}", phase);
    }
    if let Some(ref parent) = item.effective_parent {
        let _ = writeln!(s, "- **parent**: {}", parent);
    }
    if let Some(ref assignee) = anno.assignee {
        let _ = writeln!(s, "- **assignee**: {}", assignee);
    }
    if let Some(anchor) = item.item.anchors.first() {
        let _ = writeln!(s, "- **source**: `{}:{}`", anchor.file, anchor.line);
    }
    if let Some(ref engine) = anno.engine {
        let _ = writeln!(s, "- **engine**: {}", engine.as_payload());
    }
    if let Some(think) = anno.think {
        let _ = writeln!(s, "- **think**: {}", think.as_payload());
    }
    s.push('\n');

    if !anno.gotchas.is_empty() {
        s.push_str("## Gotchas (read first)\n\n");
        for g in &anno.gotchas {
            let _ = writeln!(s, "- {}", g);
        }
        s.push('\n');
    }

    if !anno.handoff.is_empty() {
        s.push_str("## Handoff\n\n");
        if anno.handoff.len() == 1 {
            let _ = writeln!(s, "{}", anno.handoff[0]);
        } else {
            for h in &anno.handoff {
                let _ = writeln!(s, "- {}", h);
            }
        }
        s.push('\n');
    }

    if !anno.next_steps.is_empty() {
        s.push_str("## Next steps\n\n");
        for n in &anno.next_steps {
            let _ = writeln!(s, "- {}", n);
        }
        s.push('\n');
    }

    if !anno.verify.is_empty() {
        s.push_str("## Verify\n\n");
        for v in &anno.verify {
            let _ = writeln!(s, "- {}", v);
        }
        s.push('\n');
    }

    if !anno.assumes.is_empty() {
        s.push_str("## Assumptions (challenge if wrong)\n\n");
        for a in &anno.assumes {
            let _ = writeln!(s, "- {}", a);
        }
        s.push('\n');
    }

    if !anno.cleanup.is_empty() {
        s.push_str("## Cleanup backlog\n\n");
        for c in &anno.cleanup {
            let _ = writeln!(s, "- {}", c);
        }
        s.push('\n');
    }

    PreludeSection {
        kind: PreludeSectionKind::Ticket,
        markdown: s.trim_end().to_string(),
    }
}

/// Walk `effective_parent` upwards from `item`. Stops on missing parent
/// or cycle. Returns nearest-first.
fn build_parent_chain<'a>(board: &'a Board, item: &'a BoardItem) -> Vec<&'a BoardItem> {
    let mut chain: Vec<&BoardItem> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    seen.insert(item.item.id.as_str());
    let mut cursor = item.effective_parent.as_deref();
    while let Some(pid) = cursor {
        if !seen.insert(pid) {
            break;
        }
        let Some(parent) = board.get(pid) else {
            break;
        };
        chain.push(parent);
        cursor = parent.effective_parent.as_deref();
    }
    chain
}

fn render_parent_chain_section(chain: &[&BoardItem]) -> PreludeSection {
    let mut s = String::from("## Parent chain\n\n");
    for parent in chain {
        let anno = &parent.item.anno;
        let _ = writeln!(
            s,
            "- **{}** [{}] — {}",
            parent.item.id,
            status_str(anno.status),
            anno.title
        );
        for g in &anno.gotchas {
            let _ = writeln!(s, "  - gotcha: {}", g);
        }
    }
    PreludeSection {
        kind: PreludeSectionKind::ParentChain,
        markdown: s.trim_end().to_string(),
    }
}

/// Effective agent-policy for a ticket. Closer-scope wins over wider —
/// the ticket's own rules come first, then each ancestor in the parent
/// chain (nearest-first), then the workspace-level defaults. Duplicates
/// (same kind + role-name + body) are dropped, so a ticket can override
/// an ancestor's role by repeating it locally with the same name.
fn resolve_agent_policy<'a>(
    item: &'a BoardItem,
    chain: &[&'a BoardItem],
    workspace: &'a [AgentPolicyRule],
) -> Vec<AgentPolicyRule> {
    let mut out: Vec<AgentPolicyRule> = Vec::new();
    let mut seen: BTreeSet<(String, Option<String>, String)> = BTreeSet::new();
    let push = |rule: &AgentPolicyRule,
                out: &mut Vec<AgentPolicyRule>,
                seen: &mut BTreeSet<(String, Option<String>, String)>| {
        let key = (
            policy_key(&rule.kind),
            rule.role_name.clone(),
            rule.body.clone(),
        );
        if seen.insert(key) {
            out.push(rule.clone());
        }
    };
    for rule in &item.item.anno.agent_policy {
        push(rule, &mut out, &mut seen);
    }
    for ancestor in chain {
        for rule in &ancestor.item.anno.agent_policy {
            push(rule, &mut out, &mut seen);
        }
    }
    for rule in workspace {
        push(rule, &mut out, &mut seen);
    }
    out
}

fn policy_key(kind: &AgentPolicyKind) -> String {
    match kind {
        AgentPolicyKind::Role => "role".into(),
        AgentPolicyKind::Do => "do".into(),
        AgentPolicyKind::Dont => "dont".into(),
        AgentPolicyKind::Unknown { raw } => format!("unknown:{}", raw),
    }
}

fn render_agent_policy_section(rules: &[AgentPolicyRule]) -> PreludeSection {
    let mut s = String::from("## Agent policy\n\n");

    let roles: Vec<&AgentPolicyRule> = rules
        .iter()
        .filter(|r| matches!(r.kind, AgentPolicyKind::Role))
        .collect();
    if !roles.is_empty() {
        s.push_str("### Roles\n\n");
        for rule in &roles {
            let name = rule.role_name.as_deref().unwrap_or("(unnamed)");
            let _ = writeln!(s, "- **{}** — {}", name, rule.body);
        }
        s.push('\n');
    }

    let dos: Vec<&AgentPolicyRule> = rules
        .iter()
        .filter(|r| matches!(r.kind, AgentPolicyKind::Do))
        .collect();
    if !dos.is_empty() {
        s.push_str("### Do\n\n");
        for rule in &dos {
            let _ = writeln!(s, "- {}", rule.body);
        }
        s.push('\n');
    }

    let donts: Vec<&AgentPolicyRule> = rules
        .iter()
        .filter(|r| matches!(r.kind, AgentPolicyKind::Dont))
        .collect();
    if !donts.is_empty() {
        s.push_str("### Don't\n\n");
        for rule in &donts {
            let _ = writeln!(s, "- {}", rule.body);
        }
        s.push('\n');
    }

    let unknowns: Vec<&AgentPolicyRule> = rules
        .iter()
        .filter(|r| matches!(r.kind, AgentPolicyKind::Unknown { .. }))
        .collect();
    if !unknowns.is_empty() {
        s.push_str("### Unrecognized policy rules\n\n");
        for rule in &unknowns {
            let raw = match &rule.kind {
                AgentPolicyKind::Unknown { raw } => raw.as_str(),
                _ => "?",
            };
            let _ = writeln!(s, "- `{}`: {}", raw, rule.body);
        }
        s.push('\n');
    }

    PreludeSection {
        kind: PreludeSectionKind::AgentPolicy,
        markdown: s.trim_end().to_string(),
    }
}

fn render_kg_slice_section(kg: &Subgraph, node_limit: u32) -> PreludeSection {
    let mut s = String::from("## KG slice\n\n");

    let nodes_taken = (kg.nodes.len() as u32).min(node_limit);
    let mut by_file: BTreeMap<&str, Vec<&NodeRef>> = BTreeMap::new();
    for n in kg.nodes.iter().take(nodes_taken as usize) {
        by_file.entry(n.file.as_str()).or_default().push(n);
    }

    for (file, nodes) in &by_file {
        let _ = writeln!(s, "### `{}`", file);
        for n in nodes {
            let _ = writeln!(
                s,
                "- {} `{}` — line {}",
                kind_label(&n.kind),
                n.qualified,
                n.span.start_line
            );
        }
        s.push('\n');
    }

    if !kg.edges.is_empty() {
        s.push_str("### Cross-references\n\n");
        let mut by_kind: BTreeMap<&'static str, u32> = BTreeMap::new();
        for e in &kg.edges {
            *by_kind.entry(edge_kind_label(&e.kind)).or_default() += 1;
        }
        for (k, n) in &by_kind {
            let _ = writeln!(s, "- {}: {}", k, n);
        }
        s.push('\n');
    }

    if kg.truncated || (kg.nodes.len() as u32) > node_limit {
        s.push_str("_KG slice truncated by depth or node budget._\n");
    }

    PreludeSection {
        kind: PreludeSectionKind::KgSlice,
        markdown: s.trim_end().to_string(),
    }
}

fn kind_label(kind: &crate::kind::NodeKind) -> &'static str {
    use crate::kind::{CommonKind, NodeKind};
    match kind {
        NodeKind::Common(CommonKind::Function) => "fn",
        NodeKind::Common(CommonKind::Method) => "method",
        NodeKind::Common(CommonKind::Type) => "type",
        NodeKind::Common(CommonKind::Module) => "mod",
        NodeKind::Common(CommonKind::File) => "file",
        NodeKind::Common(CommonKind::Field) => "field",
        NodeKind::Common(CommonKind::Variant) => "variant",
        NodeKind::Common(CommonKind::Constant) => "const",
        NodeKind::Common(CommonKind::Document) => "doc",
        NodeKind::Common(CommonKind::Tag) => "tag",
        NodeKind::Common(CommonKind::Relay) => "relay",
        NodeKind::Common(CommonKind::Ticket) => "ticket",
        NodeKind::Common(CommonKind::Directory) => "dir",
        NodeKind::Rust(_) => "rust",
        NodeKind::Ts(_) => "ts",
        NodeKind::Doc(_) => "doc",
        NodeKind::Koda(_) => "koda",
    }
}

fn edge_kind_label(kind: &EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Contains => "contains",
        EdgeKind::Defines => "defines",
        EdgeKind::Imports => "imports",
        EdgeKind::ReExports => "re_exports",
        EdgeKind::Calls => "calls",
        EdgeKind::References => "references",
        EdgeKind::Implements => "implements",
        EdgeKind::ImplFor => "impl_for",
        EdgeKind::ImplOfTrait => "impl_of_trait",
        EdgeKind::MacroInvokes => "macro_invokes",
        EdgeKind::DerivedBy => "derived_by",
        EdgeKind::AttributedBy => "attributed_by",
        EdgeKind::Bounds => "bounds",
        EdgeKind::GeneratedBy => "generated_by",
        EdgeKind::Extends => "extends",
        EdgeKind::DecoratedBy => "decorated_by",
        EdgeKind::RefersTo => "refers_to",
        EdgeKind::ConformsTo => "conforms_to",
        EdgeKind::Tag => "tag",
        EdgeKind::Flow => "flow",
        EdgeKind::Anchors => "anchors",
        EdgeKind::ParentItem => "parent_item",
        EdgeKind::Koda(_) => "koda",
    }
}

fn render_arch_doc_section(
    path: &str,
    body: Option<&String>,
    inline: bool,
) -> PreludeSection {
    let mut s = String::new();
    let _ = writeln!(s, "## Arch doc: {}", path);
    s.push('\n');
    match (body, inline) {
        (Some(text), true) => {
            s.push_str(text.trim_end());
            s.push('\n');
        }
        (Some(_), false) => {
            let _ = writeln!(
                s,
                "_Reference only — read `{}` for full content (over inline budget)._",
                path
            );
        }
        (None, _) => {
            let _ = writeln!(s, "_Reference: `{}` (not pre-loaded)._", path);
        }
    }
    PreludeSection {
        kind: PreludeSectionKind::ArchDoc {
            path: path.to_string(),
            inline: inline && body.is_some(),
        },
        markdown: s.trim_end().to_string(),
    }
}

fn render_skills_section(skills: &[String]) -> PreludeSection {
    let mut s = String::from("## Skills available\n\n");
    for skill in skills {
        let _ = writeln!(s, "- {}", skill);
    }
    PreludeSection {
        kind: PreludeSectionKind::Skills,
        markdown: s.trim_end().to_string(),
    }
}

fn status_str(s: Option<TicketStatus>) -> &'static str {
    match s {
        Some(s) => s.as_str(),
        None => "open",
    }
}

fn trim_to_budget(sections: &mut Vec<PreludeSection>, max_tokens: u32) -> bool {
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    let mut truncated = false;
    loop {
        let total: usize = sections.iter().map(|s| s.markdown.len() + 1).sum();
        if total <= max_bytes {
            return truncated;
        }
        let drop_idx = sections.iter().rposition(|s| {
            matches!(
                s.kind,
                PreludeSectionKind::KgSlice | PreludeSectionKind::ArchDoc { .. }
            )
        });
        match drop_idx {
            Some(i) => {
                sections.remove(i);
                truncated = true;
            }
            None => return truncated,
        }
    }
}

fn render_combined(sections: &[PreludeSection]) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&section.markdown);
        if !section.markdown.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn compute_cache_key(combined: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(combined.as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    let mut s = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(s, "{:02x}", byte);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anno::{WorkItemAnno, WorkItemType};
    use crate::ids::NodeId;
    use crate::kind::Lang;
    use crate::board::{WorkItem, WorkItemAnchor};

    fn synth_id(qualified: &str) -> NodeId {
        NodeId::compute(Lang::Rust, qualified, "<synthetic>")
    }

    fn anchor(file: &str, line: u32, anno: WorkItemAnno) -> WorkItemAnchor {
        WorkItemAnchor {
            node: synth_id(&format!("anchor:{file}:{line}")),
            file: file.to_string(),
            line,
            anno,
        }
    }

    fn work_item(id: &str, item_type: WorkItemType, anchors: Vec<WorkItemAnchor>) -> WorkItem {
        let canonical = anchors[0].anno.clone();
        WorkItem {
            id: id.to_string(),
            node: synth_id(&format!("ticket:{id}")),
            item_type,
            anno: canonical,
            anchors,
            last_modified_ts: 0,
        }
    }

    fn known_ticket() -> WorkItemAnno {
        let mut a = WorkItemAnno {
            id: "R028-F2".into(),
            title: "Per-ticket prelude assembler: (ticket, kg, arch) -> Prelude".into(),
            ..Default::default()
        };
        a.kind = Some("feature".into());
        a.status = Some(TicketStatus::InProgress);
        a.phase = Some("P1".into());
        a.parent = Some("R028".into());
        a.assignee = Some("agent:claude".into());
        a.handoff
            .push("Pure fn assembling: ticket block + parent chain walk".into());
        a.next_steps.push("Output structured Prelude type".into());
        a.next_steps.push("Bound KG slice by depth + token budget".into());
        a.verify.push(
            "Snapshot test: assembling Prelude for a known ticket produces stable bytes".into(),
        );
        a.see_also
            .push(".yah/arch/authored/yah-agent-runtime.md".into());
        a
    }

    fn known_board() -> Board {
        let mut parent_anno = WorkItemAnno {
            id: "R028".into(),
            title: "Yah agent runtime".into(),
            ..Default::default()
        };
        parent_anno.status = Some(TicketStatus::InProgress);
        let r028 = work_item(
            "R028",
            WorkItemType::Relay,
            vec![anchor("app/tauri/src/lib.rs", 1, parent_anno)],
        );
        let r028_f2 = work_item(
            "R028-F2",
            WorkItemType::Ticket,
            vec![anchor("yah-kg/src/lib.rs", 30, known_ticket())],
        );
        Board::from_work_items(vec![r028], vec![r028_f2])
    }

    #[test]
    fn unknown_id_returns_none() {
        let board = Board::default();
        let arch_docs = BTreeMap::new();
        let inputs = PreludeInputs {
            ticket_id: "missing",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        assert!(assemble(&inputs).is_none());
    }

    #[test]
    fn rendered_prelude_is_stable_bytes_for_known_ticket() {
        let board = known_board();
        let arch_docs = BTreeMap::new();
        let inputs = PreludeInputs {
            ticket_id: "R028-F2",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).expect("known ticket");
        let rendered = prelude.render();
        // Pin both the bytes and the cache key so any drift surfaces on
        // diff. If you change the renderer, update both — the assertion
        // is intentionally chatty.
        let expected = "\
# Ticket: R028-F2 — Per-ticket prelude assembler: (ticket, kg, arch) -> Prelude

- **id**: `R028-F2`
- **kind**: feature
- **status**: in-progress
- **phase**: P1
- **parent**: R028
- **assignee**: agent:claude
- **source**: `yah-kg/src/lib.rs:30`

## Handoff

Pure fn assembling: ticket block + parent chain walk

## Next steps

- Output structured Prelude type
- Bound KG slice by depth + token budget

## Verify

- Snapshot test: assembling Prelude for a known ticket produces stable bytes

## Parent chain

- **R028** [in-progress] — Yah agent runtime

## Arch doc: .yah/arch/authored/yah-agent-runtime.md

_Reference: `.yah/arch/authored/yah-agent-runtime.md` (not pre-loaded)._

## Output conventions

When you reference a file, function, or symbol the user might want to jump to, prefer markdown links with the `yah://` scheme over bare paths:

- `[path/to/file.rs:42](yah://file/path/to/file.rs#L42)` — opens the file in the Architecture tab rooted at that line.
- `[Foo](yah://arch/symbol/Foo)` — re-roots the arch graph on the named symbol.

The renderer turns these into clickable affordances; bare backticked `path:line` chips also work but yah:// links are preferred for prose.

## Tool-call honesty

Do not invent, omit, or rewrite your own tool history when asked about it.

- If you retried a call (e.g. one tool failed and you fell back to another), say so plainly. Repeated calls are normal — pretending they didn't happen is not.
- If a call returned an error or `ok: false`, do not describe its result as a success. The user can see the failure on their side.
- If you don't have visibility into your earlier tool calls in the current context, say \"I don't have a reliable record of my prior tool calls in this turn\" rather than guessing.
- Each tool result begins with a one-line `_smell` summary (e.g. `read_file path · 4.6KB · ok`). When recounting what you did, you may quote that line — do not fabricate one.
";
        assert_eq!(rendered, expected, "prelude render drift");
        assert_eq!(prelude.cache.key.len(), 32);
        assert_eq!(
            prelude.cache.key,
            compute_cache_key(expected),
            "cache key tracks rendered bytes"
        );
        assert!(!prelude.truncated);
    }

    #[test]
    fn assemble_includes_engine_and_think_when_present() {
        let mut a = known_ticket();
        a.engine = Some(EngineRef {
            provider: "claude".into(),
            model: Some("opus-4-7".into()),
        });
        a.think = Some(ThinkBudget::Deep);
        let r028_f2 =
            work_item("R028-F2", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![r028_f2]);
        let arch_docs = BTreeMap::new();
        let inputs = PreludeInputs {
            ticket_id: "R028-F2",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        assert_eq!(
            prelude.engine.as_ref().map(|e| e.as_payload()),
            Some("claude:opus-4-7".to_string())
        );
        assert_eq!(prelude.think, Some(ThinkBudget::Deep));
        assert!(prelude.render().contains("**engine**: claude:opus-4-7"));
        assert!(prelude.render().contains("**think**: deep"));
    }

    #[test]
    fn arch_doc_inlines_when_under_budget() {
        let r = work_item(
            "T01",
            WorkItemType::Ticket,
            vec![anchor("a.rs", 1, {
                let mut a = WorkItemAnno {
                    id: "T01".into(),
                    title: "x".into(),
                    ..Default::default()
                };
                a.see_also.push("docs/short.md".into());
                a
            })],
        );
        let board = Board::from_work_items(vec![], vec![r]);
        let mut arch_docs = BTreeMap::new();
        arch_docs.insert("docs/short.md".to_string(), "# Short\n\nbody.".to_string());
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        let rendered = prelude.render();
        assert!(rendered.contains("## Arch doc: docs/short.md"));
        assert!(rendered.contains("# Short"));
        assert!(rendered.contains("body."));
    }

    #[test]
    fn arch_doc_falls_back_to_reference_over_budget() {
        let r = work_item(
            "T01",
            WorkItemType::Ticket,
            vec![anchor("a.rs", 1, {
                let mut a = WorkItemAnno {
                    id: "T01".into(),
                    title: "x".into(),
                    ..Default::default()
                };
                a.see_also.push("docs/long.md".into());
                a
            })],
        );
        let board = Board::from_work_items(vec![], vec![r]);
        let mut arch_docs = BTreeMap::new();
        arch_docs.insert("docs/long.md".to_string(), "x".repeat(2_000));
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions {
                arch_inline_max_bytes: 1_024,
                ..PreludeOptions::default()
            },
        };
        let prelude = assemble(&inputs).unwrap();
        let rendered = prelude.render();
        assert!(rendered.contains("Reference only"));
        assert!(!rendered.contains(&"x".repeat(2_000)));
    }

    #[test]
    fn skills_section_renders_when_non_empty() {
        let r = work_item(
            "T01",
            WorkItemType::Ticket,
            vec![anchor(
                "a.rs",
                1,
                WorkItemAnno {
                    id: "T01".into(),
                    title: "x".into(),
                    ..Default::default()
                },
            )],
        );
        let board = Board::from_work_items(vec![], vec![r]);
        let arch_docs = BTreeMap::new();
        let skills = vec!["/review".to_string(), "/security-review".to_string()];
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &skills,
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        let rendered = prelude.render();
        assert!(rendered.contains("## Skills available"));
        assert!(rendered.contains("- /review"));
        assert!(rendered.contains("- /security-review"));
    }

    #[test]
    fn token_budget_drops_kg_and_arch_first() {
        let mut a = WorkItemAnno {
            id: "T01".into(),
            title: "x".into(),
            ..Default::default()
        };
        a.see_also.push("docs/long.md".into());
        let r = work_item("T01", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![r]);
        let mut arch_docs = BTreeMap::new();
        arch_docs.insert("docs/long.md".to_string(), "z".repeat(4_000));
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            // Tiny budget — only the ticket block survives.
            options: PreludeOptions {
                max_tokens: 100,
                arch_inline_max_bytes: 32_768,
                ..PreludeOptions::default()
            },
        };
        let prelude = assemble(&inputs).unwrap();
        assert!(prelude.truncated);
        assert!(prelude
            .sections
            .iter()
            .all(|s| !matches!(s.kind, PreludeSectionKind::ArchDoc { .. })));
    }

    #[test]
    fn agent_policy_section_renders_roles_do_dont_in_order() {
        // Ticket has its own agent-policy rules; parent contributes one
        // role; workspace contributes one Do. Ordering inside the section
        // is Roles → Do → Don't, with closer-scope-first within each
        // category. The section itself sits between Ticket and ParentChain.
        let mut parent_anno = WorkItemAnno {
            id: "R042".into(),
            title: "Reviewer relay".into(),
            ..Default::default()
        };
        parent_anno.agent_policy.push(AgentPolicyRule {
            kind: AgentPolicyKind::Role,
            body: "You verify, you don't write.".into(),
            role_name: Some("Reviewer".into()),
            schema_version: 1,
        });
        let parent =
            work_item("R042", WorkItemType::Relay, vec![anchor("p.rs", 1, parent_anno)]);

        let mut child_anno = WorkItemAnno {
            id: "R042-T1".into(),
            title: "Audit migration safety".into(),
            ..Default::default()
        };
        child_anno.parent = Some("R042".into());
        child_anno.status = Some(TicketStatus::InProgress);
        child_anno.agent_policy.push(AgentPolicyRule {
            kind: AgentPolicyKind::Do,
            body: "Run cargo test before approval.".into(),
            role_name: None,
            schema_version: 1,
        });
        child_anno.agent_policy.push(AgentPolicyRule {
            kind: AgentPolicyKind::Dont,
            body: "Land code without sign-off.".into(),
            role_name: None,
            schema_version: 1,
        });
        let child = work_item(
            "R042-T1",
            WorkItemType::Ticket,
            vec![anchor("c.rs", 1, child_anno)],
        );

        let board = Board::from_work_items(vec![parent], vec![child]);
        let arch_docs = BTreeMap::new();
        let workspace_policy = vec![AgentPolicyRule {
            kind: AgentPolicyKind::Do,
            body: "Use yah's MCP tools where applicable.".into(),
            role_name: None,
            schema_version: 1,
        }];
        let inputs = PreludeInputs {
            ticket_id: "R042-T1",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &workspace_policy,
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).expect("known ticket");
        let rendered = prelude.render();

        // Section position: between Ticket (header "# Ticket: ...") and
        // Parent chain ("## Parent chain").
        let policy_idx = rendered
            .find("## Agent policy")
            .expect("agent policy section");
        let parent_idx = rendered
            .find("## Parent chain")
            .expect("parent chain section");
        let ticket_idx = rendered.find("# Ticket: R042-T1").unwrap();
        assert!(ticket_idx < policy_idx, "ticket precedes policy");
        assert!(policy_idx < parent_idx, "policy precedes parent chain");

        // Roles → Do → Don't order inside the section.
        let roles_idx = rendered.find("### Roles").expect("Roles header");
        let do_idx = rendered.find("### Do\n").expect("Do header");
        let dont_idx = rendered.find("### Don't").expect("Don't header");
        assert!(roles_idx < do_idx);
        assert!(do_idx < dont_idx);

        // Content from each scope is present.
        assert!(rendered.contains("**Reviewer** — You verify, you don't write."));
        assert!(rendered.contains("- Run cargo test before approval."));
        assert!(rendered.contains("- Use yah's MCP tools where applicable."));
        assert!(rendered.contains("- Land code without sign-off."));

        // The structured section kind is present too — useful for
        // downstream consumers (CLAUDE.md vs. AGENTS.md generators).
        assert!(prelude
            .sections
            .iter()
            .any(|s| matches!(s.kind, PreludeSectionKind::AgentPolicy)));
    }

    #[test]
    fn agent_policy_dedupes_across_scopes() {
        // Same Do rule on ticket and workspace — dedupes by (kind, body).
        let mut anno = WorkItemAnno {
            id: "T01".into(),
            title: "x".into(),
            ..Default::default()
        };
        anno.agent_policy.push(AgentPolicyRule {
            kind: AgentPolicyKind::Do,
            body: "Run tests.".into(),
            role_name: None,
            schema_version: 1,
        });
        let r = work_item("T01", WorkItemType::Ticket, vec![anchor("a.rs", 1, anno)]);
        let board = Board::from_work_items(vec![], vec![r]);
        let arch_docs = BTreeMap::new();
        let workspace_policy = vec![AgentPolicyRule {
            kind: AgentPolicyKind::Do,
            body: "Run tests.".into(),
            role_name: None,
            schema_version: 1,
        }];
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &workspace_policy,
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        let rendered = prelude.render();
        // The body appears once, not twice.
        assert_eq!(rendered.matches("- Run tests.").count(), 1);
    }

    #[test]
    fn agent_policy_section_omitted_when_no_rules() {
        let r = work_item(
            "T01",
            WorkItemType::Ticket,
            vec![anchor(
                "a.rs",
                1,
                WorkItemAnno {
                    id: "T01".into(),
                    title: "x".into(),
                    ..Default::default()
                },
            )],
        );
        let board = Board::from_work_items(vec![], vec![r]);
        let arch_docs = BTreeMap::new();
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        assert!(prelude
            .sections
            .iter()
            .all(|s| !matches!(s.kind, PreludeSectionKind::AgentPolicy)));
        assert!(!prelude.render().contains("Agent policy"));
    }

    #[test]
    fn ring_depth_tracks_estimated_tokens() {
        let r = work_item(
            "T01",
            WorkItemType::Ticket,
            vec![anchor(
                "a.rs",
                1,
                WorkItemAnno {
                    id: "T01".into(),
                    title: "tiny".into(),
                    ..Default::default()
                },
            )],
        );
        let board = Board::from_work_items(vec![], vec![r]);
        let arch_docs = BTreeMap::new();
        let inputs = PreludeInputs {
            ticket_id: "T01",
            board: &board,
            kg_slice: None,
            arch_docs: &arch_docs,
            skills: &[],
            workspace_policy: &[],
            options: PreludeOptions::default(),
        };
        let prelude = assemble(&inputs).unwrap();
        assert!(prelude.estimated_tokens > 0);
        assert!(prelude.ring_depth < 0.01, "tiny ticket << 1 ring");
    }
}
