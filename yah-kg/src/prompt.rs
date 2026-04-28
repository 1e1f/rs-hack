//! @arch:layer(kg)
//! @arch:role(presentation)
//!
//! Pickup / Review prompt rendering on top of [`Board`].
//!
//! Source-of-truth renderer used by both the CLI (`yah board show --prompt`,
//! via the legacy `Ticket::to_prompt_with_ctx` shim) and the daemon's
//! `arch.ticket_prompt` RPC. Operates on a [`BoardItem`] view so multi-anchor
//! conflicts, epic detection, and parent inheritance are already resolved
//! before rendering.
//!
//! The pickup-mode body is the canonical "what's the next agent supposed to
//! do?" prompt — read by `yah board show R012 --prompt` for years, and now
//! returned over the wire so the Tauri client's clipboard button doesn't
//! need to mirror it.

use crate::anno::TicketStatus;
use crate::board::{Board, BoardItem, ChildLiveCounts};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Which flavour of prompt to render. The `Pickup` shape briefs the next
/// agent on what to do; `Review` briefs a verifier on how to confirm or
/// send back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    #[default]
    Pickup,
    Review,
}

/// Render a prompt for `id` against `board`. Returns `None` when no item
/// in the board carries that id — callers (the daemon, the CLI shim)
/// surface that as a not-found result rather than throwing.
pub fn render(board: &Board, id: &str, mode: PromptMode) -> Option<String> {
    let ctx = build_context(board, id)?;
    Some(match mode {
        PromptMode::Pickup => render_pickup(&ctx),
        PromptMode::Review => render_review(&ctx),
    })
}

struct PromptCtx<'a> {
    item: &'a BoardItem,
    parent: Option<&'a BoardItem>,
    live_children: Vec<&'a BoardItem>,
    child_live_counts: HashMap<String, ChildLiveCounts>,
}

fn build_context<'a>(board: &'a Board, id: &'a str) -> Option<PromptCtx<'a>> {
    let item = board.get(id)?;

    let mut live_children: Vec<&BoardItem> = board
        .children_of(id)
        .filter(|c| status_is_live(c.item.anno.status))
        .collect();
    live_children.sort_by(|a, b| a.item.id.cmp(&b.item.id));

    let mut child_live_counts: HashMap<String, ChildLiveCounts> = HashMap::new();
    if item.is_epic {
        for child in &live_children {
            child_live_counts.insert(child.item.id.clone(), board.child_live_counts(&child.item.id));
        }
    }

    let parent = item
        .effective_parent
        .as_deref()
        .and_then(|pid| board.get(pid));

    Some(PromptCtx {
        item,
        parent,
        live_children,
        child_live_counts,
    })
}

fn status_is_live(s: Option<TicketStatus>) -> bool {
    matches!(
        s,
        Some(TicketStatus::Open)
            | Some(TicketStatus::Claimed)
            | Some(TicketStatus::InProgress)
            | Some(TicketStatus::Handoff)
            | None
    )
}

fn status_column(s: Option<TicketStatus>) -> &'static str {
    match s {
        Some(TicketStatus::Open) | None => "Open",
        Some(TicketStatus::Claimed) => "Claimed",
        Some(TicketStatus::InProgress) => "In Progress",
        Some(TicketStatus::Handoff) => "Handoff",
        Some(TicketStatus::Review) => "Review",
        Some(TicketStatus::Done) => "Done",
    }
}

fn render_pickup(ctx: &PromptCtx) -> String {
    let item = ctx.item;
    let anno = &item.item.anno;
    let live_children = ctx.live_children.as_slice();
    let is_container = !live_children.is_empty();
    let is_epic_container = is_container && item.is_epic;

    let mut prompt = String::new();
    prompt.push_str(&format!("# Continue: {} — {}\n\n", item.item.id, anno.title));

    if !anno.gotchas.is_empty() {
        prompt.push_str("## ⚠ Gotchas (read first)\n\n");
        for g in &anno.gotchas {
            prompt.push_str(&format!("- {}\n", g));
        }
        prompt.push('\n');
    }

    if let Some(parent) = ctx.parent {
        if !parent.item.anno.gotchas.is_empty() {
            prompt.push_str(&format!(
                "## ⚠ Gotchas inherited from {} (read first)\n\n",
                parent.item.id
            ));
            for g in &parent.item.anno.gotchas {
                prompt.push_str(&format!("- {}\n", g));
            }
            prompt.push('\n');
        }
    }

    prompt.push_str("## Context\n\n");
    prompt.push_str(&format!("`{}` ", item.item.id));
    if let Some(ref assignee) = anno.assignee {
        prompt.push_str(&format!("(from {}) ", assignee));
    }
    prompt.push_str("is ready for continuation.\n\n");

    if let Some(ref phase) = anno.phase {
        prompt.push_str(&format!("**Phase**: {}\n\n", phase));
    }
    if let Some(ref parent) = item.effective_parent {
        prompt.push_str(&format!("**Parent relay**: {}\n\n", parent));
    }

    if !anno.handoff.is_empty() {
        prompt.push_str("## What was completed\n\n");
        if anno.handoff.len() == 1 {
            prompt.push_str(&anno.handoff[0]);
            prompt.push_str("\n\n");
        } else {
            for h in &anno.handoff {
                prompt.push_str(&format!("- {}\n", h));
            }
            prompt.push('\n');
        }
        let combined = anno.handoff.join("\n");
        let locs = extract_code_locations(&combined);
        if !locs.is_empty() {
            prompt.push_str("**Locations referenced above:**\n\n");
            for loc in &locs {
                prompt.push_str(&format!("- `{}`\n", loc));
            }
            prompt.push('\n');
        }
    }

    if is_container {
        if is_epic_container {
            prompt.push_str("## Child relays (watering hole — work one at a time)\n\n");
            prompt.push_str(
                "This is an epic — its baton is the chain of child relays below. \
                 You don't have to finish the whole epic in one session; pick the \
                 earliest live child, work its sub-tickets, then come back to this \
                 prompt to see what's next. Epic progress is measured by children \
                 reaching Review.\n\n",
            );
            for child in live_children {
                let status = status_column(child.item.anno.status).to_lowercase();
                let counts = ctx
                    .child_live_counts
                    .get(&child.item.id)
                    .copied()
                    .unwrap_or_default();
                let counts_suffix = {
                    let d = counts.describe();
                    if d.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", d)
                    }
                };
                let assignee = child
                    .item
                    .anno
                    .assignee
                    .as_deref()
                    .map(|a| format!(" · {}", a))
                    .unwrap_or_default();
                prompt.push_str(&format!(
                    "- **{}** [{}]{}{} · {}\n",
                    child.item.id, status, assignee, counts_suffix, child.item.anno.title
                ));
            }
            prompt.push('\n');
        } else {
            prompt.push_str("## Sub-tickets in flight\n\n");
            prompt.push_str(
                "This relay has live sub-tickets. Work them one at a time (Rule08): \
                 the earliest live one is the next action. Do it, archive it, then come \
                 back here for the next. Don't try to do the full chain in a single \
                 session.\n\n",
            );
            for child in live_children {
                let status = status_column(child.item.anno.status).to_lowercase();
                let phase = child
                    .item
                    .anno
                    .phase
                    .as_deref()
                    .map(|p| format!(" · {}", p))
                    .unwrap_or_default();
                let assignee = child
                    .item
                    .anno
                    .assignee
                    .as_deref()
                    .map(|a| format!(" · {}", a))
                    .unwrap_or_default();
                prompt.push_str(&format!(
                    "- **{}** [{}]{}{} · {}\n",
                    child.item.id, status, phase, assignee, child.item.anno.title
                ));
            }
        }

        if let Some(next) = live_children.first().copied() {
            let line = match next.item.anno.status {
                Some(TicketStatus::Open) | None => format!(
                    "\nStart with:\n\n```bash\nyah board claim {}\n```\n\n",
                    next.item.id
                ),
                Some(TicketStatus::Handoff) => format!(
                    "\nStart with:\n\n```bash\nyah board move {} active\n```\n\n",
                    next.item.id
                ),
                Some(TicketStatus::Claimed) | Some(TicketStatus::InProgress) => format!(
                    "\nContinue with **{}** — already in flight ({}). Pull its pickup \
                     prompt:\n\n```bash\nyah board tickets --prompt {}\n```\n\n",
                    next.item.id,
                    status_column(next.item.anno.status).to_lowercase(),
                    next.item.id
                ),
                _ => format!("\nStart with **{}**.\n\n", next.item.id),
            };
            prompt.push_str(&line);
        } else {
            prompt.push('\n');
        }
    }

    if !anno.next_steps.is_empty() {
        if is_container {
            prompt.push_str(
                "## Follow-on spawns (not the baton — see sub-tickets above)\n\n",
            );
        } else {
            prompt.push_str("## Next steps\n\n");
        }
        for step in &anno.next_steps {
            prompt.push_str(&format!("- {}\n", step));
        }
        prompt.push('\n');
    }

    if !anno.cleanup.is_empty() {
        prompt.push_str("## Cleanup backlog\n\n");
        for clean in &anno.cleanup {
            prompt.push_str(&format!("- {}\n", clean));
        }
        prompt.push('\n');
    }

    if !anno.verify.is_empty() {
        prompt.push_str("## Verification\n\n");
        let mut cmd_chain: Vec<String> = Vec::new();
        let mut last_was_prose = false;
        for v in &anno.verify {
            if looks_like_shell_command(v) {
                prompt.push_str("```bash\n");
                prompt.push_str(v);
                prompt.push_str("\n```\n\n");
                cmd_chain.push(strip_trailing_comment(v));
                last_was_prose = false;
            } else {
                prompt.push_str(&format!("- {}\n", v));
                last_was_prose = true;
            }
        }
        if last_was_prose {
            prompt.push('\n');
        }
        if cmd_chain.len() > 1 {
            prompt.push_str("Combined smoke test:\n\n```bash\n");
            prompt.push_str(&cmd_chain.join(" && "));
            prompt.push_str("\n```\n\n");
        }
    }

    if let Some(parent) = ctx.parent {
        let parent_cmds: Vec<String> = parent
            .item
            .anno
            .verify
            .iter()
            .filter(|v| looks_like_shell_command(v))
            .map(|v| strip_trailing_comment(v))
            .collect();
        if !parent_cmds.is_empty() {
            prompt.push_str(&format!(
                "## Verification inherited from {}\n\n",
                parent.item.id
            ));
            prompt.push_str(
                "Run the parent relay's smoke after your own checks — it catches \
                 regressions in adjacent sub-tickets that a narrow verify would \
                 miss.\n\n",
            );
            prompt.push_str("```bash\n");
            prompt.push_str(&parent_cmds.join(" && "));
            prompt.push_str("\n```\n\n");
        }
    }

    if !anno.assumes.is_empty() {
        prompt.push_str("## Assumptions (unverified — confirm or challenge)\n\n");
        for a in &anno.assumes {
            prompt.push_str(&format!("- {}\n", a));
        }
        prompt.push('\n');
    }

    if !anno.see_also.is_empty() {
        prompt.push_str("## Reference\n\n");
        for doc in &anno.see_also {
            prompt.push_str(&format!("- Read: {}\n", doc));
        }
        prompt.push('\n');
    }

    prompt.push_str("## Source\n\n");
    let canonical = &item.item.anchors[0];
    prompt.push_str(&format!("Defined at `{}:{}`\n\n", canonical.file, canonical.line));

    prompt.push_str("## First action\n\n");
    match anno.status {
        Some(TicketStatus::Open) | None => {
            prompt.push_str(&format!(
                "Claim this ticket — one atomic command flips status and assignee (Rule01):\n\n\
                 ```bash\n\
                 yah board claim {}\n\
                 ```\n\n\
                 The Prompt button's clipboard copy does **not** move the card for you. \
                 Run the claim before any other code edits.\n\n",
                item.item.id
            ));
        }
        Some(TicketStatus::Handoff) => {
            prompt.push_str(&format!(
                "Pick up the baton — one atomic command flips status and assignee (Rule01):\n\n\
                 ```bash\n\
                 yah board move {} active\n\
                 ```\n\n\
                 The Prompt button's clipboard copy does **not** move the card for you. \
                 Run the move before any other code edits.\n\n",
                item.item.id
            ));
        }
        Some(TicketStatus::Claimed) | Some(TicketStatus::InProgress) => {
            prompt.push_str(&format!(
                "This ticket is already `{}` — you're continuing an in-flight session, \
                 no claim needed. Begin with the next steps below.\n\n",
                status_column(anno.status).to_lowercase()
            ));
        }
        Some(TicketStatus::Review) | Some(TicketStatus::Done) => {
            prompt.push_str(&format!(
                "This ticket is already in `{}`. If it needs more work, send it back \
                 with `yah board move {} handoff --handoff \"what still needs doing\"`. \
                 Otherwise use the review-mode prompt from the card's Review button.\n\n",
                status_column(anno.status).to_lowercase(),
                item.item.id
            ));
        }
    }

    prompt.push_str("## Playbook\n\n");
    if !live_children.is_empty() {
        prompt.push_str(
            "Load-bearing rules for this pickup: **Rule01** (claim first — above), \
             **Rule08** (sub-ticket cycle — above), **Col01** (three end-states — below). \
             Full ruleset: `yah board rules --context pickup` (or `finishing` \
             when you wrap up).\n\n",
        );
    } else {
        prompt.push_str(
            "Load-bearing rules for this pickup: **Rule01** (claim first — above), \
             **Col01** (three end-states — below). Full ruleset: \
             `yah board rules --context pickup` (or `finishing` when you wrap up).\n\n",
        );
    }
    prompt.push_str(
        "Inspect any related ticket: `yah board show <ID>` \
         (compact view) or `yah board show <ID> --prompt` (full \
         pickup form, like this one).\n\n",
    );

    prompt.push_str("## Then\n\n");
    let mut step = 1usize;
    prompt.push_str(&format!(
        "{}. Read the reference docs and source context above.\n",
        step
    ));
    step += 1;
    prompt.push_str(&format!("{}. Complete the next steps listed.\n", step));
    step += 1;
    if !anno.cleanup.is_empty() {
        prompt.push_str(&format!(
            "{}. Address cleanup items if time permits.\n",
            step
        ));
        step += 1;
    }
    prompt.push_str(&format!("{}. Pick the right end-state (Col01):\n", step));
    prompt.push_str(&format!(
        "   - **More work remains (another phase, another agent):** \
            `yah board move {} handoff --handoff \"what you just finished\" --next \"first concrete next step\"` \
            — same R-number, baton moves forward in place (Rule03).\n",
        item.item.id
    ));
    prompt.push_str(&format!(
        "   - **This ticket's tasks are met, awaiting human sign-off:** \
            `yah board move {} review` and ping the user. Do **not** self-archive — \
            review is where a human exercises `@yah:verify(...)` and confirms.\n",
        item.item.id
    ));
    prompt.push_str(
        "   - **Already signed off in a previous pass:** archive via the card button \
            (strips `@yah:` lines from source, appends `archived` to `.yah/events.jsonl`).\n",
    );

    prompt
}

fn render_review(ctx: &PromptCtx) -> String {
    let item = ctx.item;
    let anno = &item.item.anno;

    let mut prompt = String::new();
    prompt.push_str(&format!("# Review: {} — {}\n\n", item.item.id, anno.title));

    prompt.push_str("## Context\n\n");
    prompt.push_str(&format!("`{}` ", item.item.id));
    if let Some(ref assignee) = anno.assignee {
        prompt.push_str(&format!("(from {}) ", assignee));
    }
    prompt.push_str("is awaiting review.\n\n");
    if let Some(ref parent) = item.effective_parent {
        prompt.push_str(&format!("**Parent relay**: {}\n\n", parent));
    }

    if !anno.handoff.is_empty() {
        prompt.push_str("## What was claimed\n\n");
        if anno.handoff.len() == 1 {
            prompt.push_str(&anno.handoff[0]);
            prompt.push_str("\n\n");
        } else {
            for h in &anno.handoff {
                prompt.push_str(&format!("- {}\n", h));
            }
            prompt.push('\n');
        }
    }

    prompt.push_str("## Verify\n\n");
    if anno.verify.is_empty() {
        prompt.push_str(
            "No `@yah:verify(...)` commands declared. Read the diff for this ticket \
             and exercise the change manually before deciding.\n\n",
        );
    } else {
        prompt.push_str("Run each command. If any fail, the ticket is not ready.\n\n");
        let mut cmd_chain: Vec<String> = Vec::new();
        let mut last_was_prose = false;
        for v in &anno.verify {
            if looks_like_shell_command(v) {
                prompt.push_str("```bash\n");
                prompt.push_str(v);
                prompt.push_str("\n```\n\n");
                cmd_chain.push(strip_trailing_comment(v));
                last_was_prose = false;
            } else {
                prompt.push_str(&format!("- {}\n", v));
                last_was_prose = true;
            }
        }
        if last_was_prose {
            prompt.push('\n');
        }
        if cmd_chain.len() > 1 {
            prompt.push_str("Combined smoke test:\n\n```bash\n");
            prompt.push_str(&cmd_chain.join(" && "));
            prompt.push_str("\n```\n\n");
        }
    }

    prompt.push_str("## Source\n\n");
    let canonical = &item.item.anchors[0];
    prompt.push_str(&format!("Defined at `{}:{}`\n\n", canonical.file, canonical.line));

    prompt.push_str("## Decide\n\n");
    prompt.push_str(
        "- **Approve:** click the card's archive button (two-stage: arms first, \
         then commits — strips `@yah:` lines from source, appends `archived` to \
         `.yah/events.jsonl`).\n",
    );
    prompt.push_str(&format!(
        "- **Send back:** `yah board move {} handoff --handoff \"what still \
         needs doing\"` — the relay returns to the next pickup with your notes \
         attached.\n",
        item.item.id
    ));

    prompt
}

// ── Helpers (ported from yah/src/arch/ticket.rs so the daemon can render
// without depending on the `yah` crate) ────────────────────────────────────

/// Return true if `s` reads like a runnable shell command (rather than a prose
/// verification criterion like `"cargo test ... is clean (no new errors)"`).
fn looks_like_shell_command(s: &str) -> bool {
    let trimmed = s
        .trim_start()
        .trim_start_matches("$ ")
        .trim_start_matches("> ")
        .trim_start();
    let Some(first) = trimmed.split_whitespace().next() else {
        return false;
    };
    const COMMANDS: &[&str] = &[
        "cargo", "yah", "yahh", "yahb", "yaha", "rs-hack", "rshack", "bun", "npm", "pnpm", "yarn",
        "deno", "npx", "make", "cmake", "ninja", "bash", "sh", "zsh", "python", "python3", "pip",
        "uv", "pytest", "node", "rustup", "rustc", "cc", "clang", "gcc", "curl", "wget", "git",
        "gh", "docker", "podman", "kubectl", "sudo", "env", "just", "task",
    ];
    let first_is_cmd = COMMANDS.contains(&first)
        || first.starts_with("./")
        || first.starts_with("../")
        || first.starts_with('/');
    if !first_is_cmd {
        return false;
    }
    let body = strip_trailing_comment(s);
    let lower = body.to_lowercase();
    const PROSE_MARKERS: &[&str] = &[
        " is clean",
        " should pass",
        " should be",
        " should now",
        " expected:",
        " is currently",
        " currently fails",
        " is green",
        " must pass",
        " must be",
        " remain green",
        "don't fix",
        " unrelated to",
    ];
    if PROSE_MARKERS.iter().any(|m| lower.contains(m)) {
        return false;
    }
    true
}

fn strip_trailing_comment(s: &str) -> String {
    let last_quote = s.rfind(['"', '\'']);
    let candidates = ["  #", " # ", "\t#"];
    let mut cut: Option<usize> = None;
    for pat in &candidates {
        if let Some(pos) = s.find(pat) {
            if last_quote.map_or(true, |lq| pos > lq) {
                cut = Some(cut.map_or(pos, |c| c.min(pos)));
            }
        }
    }
    match cut {
        Some(pos) => s[..pos].trim_end().to_string(),
        None => s.trim_end().to_string(),
    }
}

fn extract_code_locations(prose: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bytes = prose.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let Some(colon) = (i..bytes.len()).find(|&j| bytes[j] == b':') else { break };
        let start_digit = colon + 1;
        let mut end = start_digit;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == start_digit {
            i = colon + 1;
            continue;
        }
        let mut path_start = colon;
        while path_start > 0 {
            let b = bytes[path_start - 1];
            let ok = b.is_ascii_alphanumeric()
                || b == b'_'
                || b == b'.'
                || b == b'/'
                || b == b'-';
            if !ok {
                break;
            }
            path_start -= 1;
        }
        let path_tok = &prose[path_start..colon];
        let looks_like_path = path_tok.contains('/')
            || path_tok.ends_with(".rs")
            || path_tok.ends_with(".md")
            || path_tok.ends_with(".toml")
            || path_tok.ends_with(".ts")
            || path_tok.ends_with(".tsx")
            || path_tok.ends_with(".js")
            || path_tok.ends_with(".json");
        if !path_tok.is_empty() && looks_like_path {
            let loc = format!("{}:{}", path_tok, &prose[start_digit..end]);
            if seen.insert(loc.clone()) {
                out.push(loc);
            }
        }
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anno::{TicketStatus, WorkItemAnno, WorkItemType};
    use crate::ids::NodeId;
    use crate::kind::Lang;
    use crate::rpc::{WorkItem, WorkItemAnchor};

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

    fn item(id: &str, item_type: WorkItemType, anchors: Vec<WorkItemAnchor>) -> WorkItem {
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

    #[test]
    fn unknown_id_returns_none() {
        let board = Board::default();
        assert!(render(&board, "missing", PromptMode::Pickup).is_none());
    }

    #[test]
    fn pickup_includes_source_anchor() {
        let mut a = WorkItemAnno {
            id: "T01".into(),
            title: "Do the thing".into(),
            status: Some(TicketStatus::Open),
            ..Default::default()
        };
        a.next_steps.push("first step".into());
        let t01 = item("T01", WorkItemType::Ticket, vec![anchor("src/foo.rs", 42, a)]);
        let board = Board::from_work_items(vec![], vec![t01]);
        let p = render(&board, "T01", PromptMode::Pickup).unwrap();
        assert!(p.contains("# Continue: T01 — Do the thing"));
        assert!(p.contains("Defined at `src/foo.rs:42`"));
        assert!(p.contains("yah board claim T01"));
    }

    #[test]
    fn pickup_renders_sub_tickets_and_rule08() {
        let r017 = {
            let a = WorkItemAnno {
                id: "R017".into(),
                title: "Some relay".into(),
                ..Default::default()
            };
            item("R017", WorkItemType::Relay, vec![anchor("src/lib.rs", 1, a)])
        };
        let r017_t1 = {
            let mut a = WorkItemAnno {
                id: "R017-T1".into(),
                title: "child".into(),
                status: Some(TicketStatus::Handoff),
                ..Default::default()
            };
            a.parent = Some("R017".into());
            item("R017-T1", WorkItemType::Ticket, vec![anchor("src/lib.rs", 5, a)])
        };
        let board = Board::from_work_items(vec![r017], vec![r017_t1]);
        let p = render(&board, "R017", PromptMode::Pickup).unwrap();
        assert!(p.contains("## Sub-tickets in flight"));
        assert!(p.contains("R017-T1"));
        assert!(p.contains("Rule08"));
        assert!(p.contains("yah board move R017-T1 active"));
    }

    #[test]
    fn pickup_omits_rule08_when_no_children() {
        let mut a = WorkItemAnno {
            id: "T01".into(),
            title: "Solo".into(),
            status: Some(TicketStatus::Open),
            ..Default::default()
        };
        a.next_steps.push("ship it".into());
        let t01 = item("T01", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![t01]);
        let p = render(&board, "T01", PromptMode::Pickup).unwrap();
        assert!(!p.contains("Rule08"));
        assert!(!p.contains("Sub-tickets in flight"));
    }

    #[test]
    fn pickup_renders_reference_section_from_see_also() {
        let mut a = WorkItemAnno {
            id: "T01".into(),
            title: "Solo".into(),
            status: Some(TicketStatus::Open),
            ..Default::default()
        };
        a.see_also
            .push("architecture/yah-roadmap-2026Q2.md".into());
        let t01 = item("T01", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![t01]);
        let p = render(&board, "T01", PromptMode::Pickup).unwrap();
        assert!(p.contains("## Reference"));
        assert!(p.contains("- Read: architecture/yah-roadmap-2026Q2.md"));
    }

    #[test]
    fn review_mode_renders_decide_section() {
        let a = WorkItemAnno {
            id: "T01".into(),
            title: "Done thing".into(),
            status: Some(TicketStatus::Review),
            ..Default::default()
        };
        let t01 = item("T01", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![t01]);
        let p = render(&board, "T01", PromptMode::Review).unwrap();
        assert!(p.contains("# Review: T01 — Done thing"));
        assert!(p.contains("## Decide"));
        assert!(p.contains("yah board move T01 handoff"));
    }
}
