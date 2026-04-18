//! @arch:layer(arch)
//! @arch:role(ticket)
//! @hack:relay(R001, "hack-board: two-noun model")
//! @hack:assignee(agent:claude)
//! @hack:phase(P1)
//! @hack:handoff("Refactored to two-noun model: Ticket + Relay. Feature/Bug/Task collapsed into Ticket with kind tag. Story/Epic replaced by Relay parent chain.")
//! @hack:verify("cargo test -p rs-hack-arch")
//! @arch:see(architecture/hack-board.md)
//!
//! Ticket and Relay aggregation from `@hack:` annotations.
//!
//! Two nouns:
//! - **Ticket**: a unit of work (kind: feature/bug/task is just a tag)
//! - **Relay**: a thread of work / coordination point (owns tickets, single agent)
//!
//! Three tags:
//! - **kind**: feature, bug, task (on tickets)
//! - **phase**: P1, P2 (ordering within a relay)
//! - **parent**: relay-to-relay hierarchy (epic = relay with child relays)

use crate::annotation::{AnnotationTarget, ArchAnnotation, ArchKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Whether this is a Ticket or a Relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Ticket,
    Relay,
}

/// Status in the lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketStatus {
    Open,
    Claimed,
    InProgress,
    Handoff,
    Review,
    Done,
}

impl TicketStatus {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "open" => Self::Open,
            "claimed" => Self::Claimed,
            "in-progress" | "in_progress" | "inprogress" => Self::InProgress,
            "handoff" => Self::Handoff,
            "review" => Self::Review,
            "done" | "closed" | "complete" => Self::Done,
            _ => Self::Open,
        }
    }

    pub fn column(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Claimed => "Claimed",
            Self::InProgress => "In Progress",
            Self::Handoff => "Handoff",
            Self::Review => "Review",
            Self::Done => "Done",
        }
    }
}

/// A work item (Ticket or Relay) on the hack-board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    /// ID (e.g., "F01", "B03", "T12", "R001")
    pub id: String,

    /// Human-readable title
    pub title: String,

    /// Ticket or Relay
    pub item_type: ItemType,

    /// Kind tag: "feature", "bug", "task" (for tickets). Inferred from ID prefix if not explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Current status
    pub status: TicketStatus,

    /// Who is working on this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Phase tag (e.g., "P1")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// Parent relay ID (e.g., "R001") — for relay hierarchy (epic = relay with children)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Severity (for bugs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,

    /// Handoff message(s). Multiple `@hack:handoff(...)` annotations stack —
    /// each renders as its own bullet in the pickup prompt, making the
    /// completed-work section scannable. A single handoff renders as a
    /// paragraph for backward-compatible readability.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub handoff: Vec<String>,

    /// What the next agent should do
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub next_steps: Vec<String>,

    /// Deferred cleanup items
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cleanup: Vec<String>,

    /// Verification criteria (for relays)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub verify: Vec<String>,

    /// Pre-existing breakage / traps the next agent needs to know up front.
    /// From `@hack:gotcha(...)`. Rendered above the context block in
    /// the pickup prompt.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub gotchas: Vec<String>,

    /// Unverified assumptions that were baked into the handoff. From
    /// `@hack:assumes(...)`. Rendered as risks in the pickup prompt so
    /// the next agent knows to confirm or challenge them.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub assumes: Vec<String>,

    /// Depends on these IDs
    pub depends_on: Vec<String>,

    /// Links to architecture docs
    pub see_also: Vec<String>,

    /// Source file
    pub file: PathBuf,

    /// Line number
    pub line: usize,

    /// AST target
    pub target: AnnotationTarget,

    /// True when this relay acts as an epic — either explicitly declared with
    /// `@hack:kind(epic)` or inferred from having child relays via
    /// `@hack:parent(self.id)`. Set by [`TicketBoard::resolve_epics`].
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub is_epic: bool,

    /// Derived status for epics: `"active"` if any child relay is not in a
    /// terminal state (review/done), `"closed"` otherwise. For a freshly
    /// declared epic with no children, defaults to `"active"` (treat as
    /// planning). `None` for non-epics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_status: Option<String>,
}

impl Ticket {
    /// Badge letter for display (inferred from kind or ID prefix).
    pub fn badge(&self) -> &str {
        if self.item_type == ItemType::Relay {
            return "R";
        }
        match self.kind.as_deref() {
            Some("bug") => "B",
            Some("feature") => "F",
            Some("task") => "T",
            _ => {
                // Infer from ID prefix
                match self.id.chars().next() {
                    Some('B') | Some('b') => "B",
                    Some('F') | Some('f') => "F",
                    Some('T') | Some('t') => "T",
                    _ => "T", // default to task
                }
            }
        }
    }
}

/// The board — a collection of tickets and relays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketBoard {
    pub tickets: Vec<Ticket>,
}

impl TicketBoard {
    /// Build a board from annotations.
    ///
    /// Grouping is ticket-scoped, not target-scoped: a single `//!` doc-block
    /// can declare several stacked `@hack:ticket` / `@hack:relay` headers, and
    /// each one owns the non-defining annotations that follow it (until the
    /// next header). Before, all module-level annotations shared the same
    /// `AnnotationTarget::Module` id and collapsed into one ticket — stacked
    /// headers silently shadowed each other.
    pub fn from_annotations(annotations: &[ArchAnnotation]) -> Self {
        // Step 1: isolate "neighborhoods" — annotations that share a file and
        // syn target. Different targets in the same file (module block vs. a
        // struct) can't bleed into each other.
        let mut neighborhoods: HashMap<(PathBuf, String), Vec<&ArchAnnotation>> =
            HashMap::new();
        for ann in annotations {
            if is_hack_relevant(&ann.kind) {
                let key = (ann.file.clone(), ann.target.id());
                neighborhoods.entry(key).or_default().push(ann);
            }
        }

        // Step 2: within each neighborhood, walk in source order. Each
        // ticket/relay header opens a new logical bucket keyed by its ID;
        // subsequent non-defining annotations attach to the most recent
        // header. Annotations that appear before any header are dropped
        // (mirrors the old behavior, which required a defining annotation
        // in the bucket).
        let mut by_ticket: HashMap<String, Vec<&ArchAnnotation>> = HashMap::new();
        for (_, mut anns) in neighborhoods {
            anns.sort_by_key(|a| a.line);
            let mut current: Option<String> = None;
            for ann in anns {
                if let ArchKind::Ticket { id, .. } | ArchKind::Relay { id, .. } = &ann.kind {
                    current = Some(id.clone());
                }
                if let Some(ref cid) = current {
                    by_ticket.entry(cid.clone()).or_default().push(ann);
                }
            }
        }

        let mut tickets = Vec::new();
        for (_id, anns) in by_ticket {
            if let Some(ticket) = build_item(&anns) {
                tickets.push(ticket);
            }
        }

        tickets.sort_by(|a, b| a.id.cmp(&b.id));
        let mut board = TicketBoard { tickets };
        board.resolve_epics();
        board
    }

    /// Walk the board and mark every relay that acts as an epic, computing
    /// its derived status from its children's statuses.
    ///
    /// Two ways to qualify as an epic:
    /// 1. Explicit `@hack:kind(epic)` on the relay
    /// 2. At least one *bare-R-ID* relay declares `@hack:parent(self.id)`
    ///
    /// Sub-tickets with compound IDs (`R007-T1`, `R007-T2`) don't make
    /// their parent an epic — they make it an ordinary relay-with-subtickets.
    /// Epics coordinate *between* relays, not within one.
    ///
    /// Also: infer `parent` from a compound ID prefix if the ticket didn't
    /// declare `@hack:parent(...)` explicitly. `R007-T1` → parent `R007`.
    fn resolve_epics(&mut self) {
        use std::collections::HashMap;

        // Infer parent from compound ID when not explicit.
        for t in &mut self.tickets {
            if t.parent.is_none() {
                if let Some((p, _)) = t.id.split_once('-') {
                    t.parent = Some(p.to_string());
                }
            }
        }

        // Collect bare-R child statuses by parent-id. Sub-tickets (compound
        // IDs) are excluded — they don't promote their parent to epic.
        let mut epic_children: HashMap<String, Vec<TicketStatus>> = HashMap::new();
        for t in &self.tickets {
            if t.item_type != ItemType::Relay || t.id.contains('-') {
                continue;
            }
            if let Some(parent) = &t.parent {
                epic_children
                    .entry(parent.clone())
                    .or_default()
                    .push(t.status.clone());
            }
        }

        for t in &mut self.tickets {
            if t.item_type != ItemType::Relay {
                continue;
            }
            // A sub-ticket (e.g. R007-T1) is never itself an epic.
            if t.id.contains('-') {
                continue;
            }
            let explicit = t.kind.as_deref() == Some("epic");
            let children = epic_children.get(&t.id);
            let has_children = children.is_some();
            if !(explicit || has_children) {
                continue;
            }
            t.is_epic = true;
            t.epic_status = Some(compute_epic_status(children.map(|v| v.as_slice())));
        }
    }

    pub fn epics(&self) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.is_epic).collect()
    }

    pub fn by_status(&self, status: &TicketStatus) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| &t.status == status).collect()
    }

    pub fn get(&self, id: &str) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.id == id)
    }

    pub fn assigned_to(&self, assignee: &str) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.assignee.as_deref() == Some(assignee)).collect()
    }

    pub fn handoffs(&self) -> Vec<&Ticket> {
        self.by_status(&TicketStatus::Handoff)
    }

    pub fn relays(&self) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.item_type == ItemType::Relay).collect()
    }

    /// Get tickets that belong to a relay (via parent tag).
    pub fn children_of(&self, relay_id: &str) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.parent.as_deref() == Some(relay_id)).collect()
    }

    pub fn to_markdown(&self) -> String {
        let columns = [
            TicketStatus::Open,
            TicketStatus::Claimed,
            TicketStatus::InProgress,
            TicketStatus::Handoff,
            TicketStatus::Review,
            TicketStatus::Done,
        ];

        let mut output = String::from("# Hack Board\n\n");

        for status in &columns {
            let tickets = self.by_status(status);
            if tickets.is_empty() {
                continue;
            }

            output.push_str(&format!("## {}\n\n", status.column()));
            for ticket in tickets {
                output.push_str(&format!(
                    "- **[{}] {}**: {}\n",
                    ticket.badge(), ticket.id, ticket.title
                ));
                if let Some(ref assignee) = ticket.assignee {
                    output.push_str(&format!("  assignee: {}\n", assignee));
                }
                if let Some(ref parent) = ticket.parent {
                    output.push_str(&format!("  parent: {}\n", parent));
                }
                if let Some(ref phase) = ticket.phase {
                    output.push_str(&format!("  phase: {}\n", phase));
                }
                if let Some(ref severity) = ticket.severity {
                    output.push_str(&format!("  severity: {}\n", severity));
                }
                if !ticket.handoff.is_empty() {
                    output.push_str("  handoff:\n");
                    for h in &ticket.handoff {
                        output.push_str(&format!("    - {}\n", h));
                    }
                }
                if !ticket.next_steps.is_empty() {
                    output.push_str("  next:\n");
                    for step in &ticket.next_steps {
                        output.push_str(&format!("    - {}\n", step));
                    }
                }
                if !ticket.cleanup.is_empty() {
                    output.push_str("  cleanup:\n");
                    for item in &ticket.cleanup {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
                if !ticket.verify.is_empty() {
                    output.push_str("  verify:\n");
                    for v in &ticket.verify {
                        output.push_str(&format!("    - {}\n", v));
                    }
                }
                if !ticket.see_also.is_empty() {
                    output.push_str(&format!("  see: {}\n", ticket.see_also.join(", ")));
                }
                output.push_str(&format!("  source: {}:{}\n", ticket.file.display(), ticket.line));
            }
            output.push('\n');
        }

        let total = self.tickets.len();
        let done = self.by_status(&TicketStatus::Done).len();
        output.push_str(&format!("---\n{}/{} complete\n", done, total));
        output
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

impl Ticket {
    /// Generate a continuation prompt for the next agent.
    ///
    /// Equivalent to `to_prompt_with_context(&[])` — no sibling context.
    /// Callers that have a `TicketBoard` in hand should prefer
    /// `to_prompt_with_context` so the prompt can surface live sub-tickets
    /// under the relay (R8 cycle).
    pub fn to_prompt(&self) -> String {
        self.to_prompt_with_context(&[])
    }

    /// Generate a continuation prompt, optionally including live sub-tickets
    /// so the next agent sees the R8 cycle they're stepping into.
    /// `live_children` should be the sub-tickets/child relays still in flight
    /// (Open / Active / Handoff), sorted by ID. Pass `&[]` if none or unknown.
    pub fn to_prompt_with_context(&self, live_children: &[&Ticket]) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!("# Continue: {} — {}\n\n", self.id, self.title));

        // Gotchas: render at the top so the next agent doesn't stub their toe.
        if !self.gotchas.is_empty() {
            prompt.push_str("## ⚠ Gotchas (read first)\n\n");
            for g in &self.gotchas {
                prompt.push_str(&format!("- {}\n", g));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Context\n\n");
        prompt.push_str(&format!("`{}` ", self.id));
        if let Some(ref assignee) = self.assignee {
            prompt.push_str(&format!("(from {}) ", assignee));
        }
        prompt.push_str("is ready for continuation.\n\n");

        if let Some(ref phase) = self.phase {
            prompt.push_str(&format!("**Phase**: {}\n\n", phase));
        }
        if let Some(ref parent) = self.parent {
            prompt.push_str(&format!("**Parent relay**: {}\n\n", parent));
        }

        if !self.handoff.is_empty() {
            prompt.push_str("## What was completed\n\n");
            if self.handoff.len() == 1 {
                // Single handoff renders as a paragraph (legacy, preserves
                // prose that was one long summary).
                prompt.push_str(&self.handoff[0]);
                prompt.push_str("\n\n");
            } else {
                // Multiple handoff lines render as bullets — each one a
                // discrete chunk of work, file-grouped or bullet-per-change
                // at the author's discretion.
                for h in &self.handoff {
                    prompt.push_str(&format!("- {}\n", h));
                }
                prompt.push('\n');
            }
            // Surface any file:line references buried in the prose. Scan the
            // combined body so locations are extracted whether the handoff
            // is a single paragraph or a list of bullets.
            let combined = self.handoff.join("\n");
            let locs = extract_code_locations(&combined);
            if !locs.is_empty() {
                prompt.push_str("**Locations referenced above:**\n\n");
                for loc in &locs {
                    prompt.push_str(&format!("- `{}`\n", loc));
                }
                prompt.push('\n');
            }
        }

        if !self.next_steps.is_empty() {
            prompt.push_str("## Next steps\n\n");
            for step in &self.next_steps {
                prompt.push_str(&format!("- {}\n", step));
            }
            prompt.push('\n');
        }

        if !self.cleanup.is_empty() {
            prompt.push_str("## Cleanup backlog\n\n");
            for item in &self.cleanup {
                prompt.push_str(&format!("- {}\n", item));
            }
            prompt.push('\n');
        }

        if !self.verify.is_empty() {
            prompt.push_str("## Verification\n\n");
            // Split into runnable commands vs prose criteria. Only commands get
            // fenced (copy-pasteable); prose stays as plain bullets so the next
            // agent doesn't try to execute a sentence.
            let mut cmd_chain: Vec<String> = Vec::new();
            let mut last_was_prose = false;
            for v in &self.verify {
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
                // Close the bullet run with a blank line before whatever
                // follows (smoke test or next section).
                prompt.push('\n');
            }
            if cmd_chain.len() > 1 {
                prompt.push_str("Combined smoke test:\n\n```bash\n");
                prompt.push_str(&cmd_chain.join(" && "));
                prompt.push_str("\n```\n\n");
            }
        }

        // Assumptions baked into the handoff that the next agent should validate.
        if !self.assumes.is_empty() {
            prompt.push_str("## Assumptions (unverified — confirm or challenge)\n\n");
            for a in &self.assumes {
                prompt.push_str(&format!("- {}\n", a));
            }
            prompt.push('\n');
        }

        if !self.see_also.is_empty() {
            prompt.push_str("## Reference\n\n");
            for doc in &self.see_also {
                prompt.push_str(&format!("- Read: {}\n", doc));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Source\n\n");
        prompt.push_str(&format!("Defined at `{}:{}`\n\n", self.file.display(), self.line));

        // Sub-tickets in flight — if the relay has live children, teach the R8
        // cycle explicitly. Agents shepherding a relay need this more than they
        // need another restatement of R1.
        if !live_children.is_empty() {
            prompt.push_str("## Sub-tickets in flight\n\n");
            prompt.push_str(
                "This relay has live sub-tickets. Work them one at a time (R8): \
                 claim the earliest open one, do it, archive it, then claim the next. \
                 Don't try to do the full chain in a single session.\n\n",
            );
            for child in live_children {
                let status = child.status.column().to_lowercase();
                let phase = child
                    .phase
                    .as_deref()
                    .map(|p| format!(" · {}", p))
                    .unwrap_or_default();
                let assignee = child
                    .assignee
                    .as_deref()
                    .map(|a| format!(" · {}", a))
                    .unwrap_or_default();
                prompt.push_str(&format!(
                    "- **{}** [{}]{}{} · {}\n",
                    child.id, status, phase, assignee, child.title
                ));
            }
            // Suggest the earliest claimable child by ID order.
            if let Some(first_claimable) = live_children
                .iter()
                .find(|c| matches!(c.status, TicketStatus::Open | TicketStatus::Handoff))
            {
                let verb = match first_claimable.status {
                    TicketStatus::Handoff => format!("move {} active", first_claimable.id),
                    _ => format!("claim {}", first_claimable.id),
                };
                prompt.push_str(&format!(
                    "\nStart with:\n\n```bash\nrs-hack board {}\n```\n\n",
                    verb
                ));
            } else {
                prompt.push('\n');
            }
        }

        prompt.push_str("## First action\n\n");
        match self.status {
            TicketStatus::Open => {
                prompt.push_str(&format!(
                    "Claim this ticket — one atomic command flips status and assignee (R1):\n\n\
                     ```bash\n\
                     rs-hack board claim {}\n\
                     ```\n\n\
                     The Prompt button's clipboard copy does **not** move the card for you. \
                     Run the claim before any other code edits.\n\n",
                    self.id
                ));
            }
            TicketStatus::Handoff => {
                prompt.push_str(&format!(
                    "Pick up the baton — one atomic command flips status and assignee (R1):\n\n\
                     ```bash\n\
                     rs-hack board move {} active\n\
                     ```\n\n\
                     The Prompt button's clipboard copy does **not** move the card for you. \
                     Run the move before any other code edits.\n\n",
                    self.id
                ));
            }
            TicketStatus::Claimed | TicketStatus::InProgress => {
                prompt.push_str(&format!(
                    "This ticket is already `{}` — you're continuing an in-flight session, \
                     no claim needed. Begin with the next steps below.\n\n",
                    self.status.column().to_lowercase()
                ));
            }
            TicketStatus::Review | TicketStatus::Done => {
                prompt.push_str(&format!(
                    "This ticket is already in `{}`. If it needs more work, send it back \
                     with `rs-hack board move {} handoff --handoff \"what still needs doing\"`. \
                     Otherwise use the review-mode prompt from the card's Review button.\n\n",
                    self.status.column().to_lowercase(),
                    self.id
                ));
            }
        }

        // Playbook — name the rules that are load-bearing for THIS pickup, not
        // a generic list. R8 only matters when the relay has live children.
        prompt.push_str("## Playbook\n\n");
        if !live_children.is_empty() {
            prompt.push_str(
                "Load-bearing rules for this pickup: **R1** (claim first — above), \
                 **R8** (sub-ticket cycle — above), **C1** (three end-states — below). \
                 Full ruleset: `rs-hack board rules --context pickup` (or `finishing` \
                 when you wrap up).\n\n",
            );
        } else {
            prompt.push_str(
                "Load-bearing rules for this pickup: **R1** (claim first — above), \
                 **C1** (three end-states — below). Full ruleset: \
                 `rs-hack board rules --context pickup` (or `finishing` when you wrap up).\n\n",
            );
        }

        prompt.push_str("## Then\n\n");
        let mut step = 1usize;
        prompt.push_str(&format!(
            "{}. Read the reference docs and source context above.\n",
            step
        ));
        step += 1;
        prompt.push_str(&format!("{}. Complete the next steps listed.\n", step));
        step += 1;
        if !self.cleanup.is_empty() {
            prompt.push_str(&format!(
                "{}. Address cleanup items if time permits.\n",
                step
            ));
            step += 1;
        }
        prompt.push_str(&format!("{}. Pick the right end-state (C1):\n", step));
        prompt.push_str(&format!(
            "   - **More work remains (another phase, another agent):** \
                `rs-hack board move {} handoff --handoff \"what you just finished\" --next \"first concrete next step\"` \
                — same R-number, baton moves forward in place (R3).\n",
            self.id
        ));
        prompt.push_str(&format!(
            "   - **This ticket's tasks are met, awaiting human sign-off:** \
                `rs-hack board move {} review` and ping the user. Do **not** self-archive — \
                review is where a human exercises `@hack:verify(...)` and confirms.\n",
            self.id
        ));
        prompt.push_str(
            "   - **Already signed off in a previous pass:** archive via the card button \
                (strips `@hack:` lines from source, appends `archived` to `.hack/events.jsonl`).\n",
        );

        prompt
    }

    /// Generate a relay markdown document.
    pub fn to_relay_doc(&self) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("# {}: {}\n\n", self.id, self.title));
        doc.push_str(&format!("**Status**: {}\n", self.status.column()));
        if let Some(ref assignee) = self.assignee {
            doc.push_str(&format!("**Agent**: {}\n", assignee));
        }
        if let Some(ref phase) = self.phase {
            doc.push_str(&format!("**Phase**: {}\n", phase));
        }
        if let Some(ref parent) = self.parent {
            doc.push_str(&format!("**Parent**: {}\n", parent));
        }
        doc.push_str(&format!("**Source**: `{}:{}`\n", self.file.display(), self.line));
        doc.push('\n');

        if !self.handoff.is_empty() {
            doc.push_str("## Completed\n\n");
            if self.handoff.len() == 1 {
                doc.push_str(&self.handoff[0]);
                doc.push_str("\n\n");
            } else {
                for h in &self.handoff {
                    doc.push_str(&format!("- {}\n", h));
                }
                doc.push('\n');
            }
        }

        if !self.next_steps.is_empty() {
            doc.push_str("## Next Steps\n\n");
            for step in &self.next_steps {
                doc.push_str(&format!("- {}\n", step));
            }
            doc.push('\n');
        }

        if !self.cleanup.is_empty() {
            doc.push_str("## Cleanup Backlog\n\n");
            for item in &self.cleanup {
                doc.push_str(&format!("- {}\n", item));
            }
            doc.push('\n');
        }

        if !self.verify.is_empty() {
            doc.push_str("## Verification\n\n");
            for v in &self.verify {
                doc.push_str(&format!("- {}\n", v));
            }
            doc.push('\n');
        }

        if !self.see_also.is_empty() {
            doc.push_str("## References\n\n");
            for s in &self.see_also {
                doc.push_str(&format!("- {}\n", s));
            }
            doc.push('\n');
        }

        doc
    }
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Return true if `s` reads like a runnable shell command (rather than a prose
/// verification criterion like `"cargo test ... is clean (no new errors)"`).
///
/// Heuristic: strip any leading `$ ` / `> ` prompt, then:
/// 1. The first whitespace-delimited token must be a known CLI prefix OR a
///    relative/absolute path (`./...`, `/usr/...`).
/// 2. Reject if the line contains prose-like phrases that signal "this is a
///    criterion, not a command" — " is clean", " should pass", " expected:",
///    " is currently", etc. Authors often embed a command in a sentence; the
///    sentence is not runnable even though it starts with `cargo`.
///
/// Conservative on purpose: false negatives (missing a command) just become
/// plain bullets, which is safe. False positives (fencing prose) produce
/// broken copy-paste, which is what the feedback called out.
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
        "cargo", "rs-hack", "rshack", "bun", "npm", "pnpm", "yarn", "deno",
        "npx", "make", "cmake", "ninja", "bash", "sh", "zsh", "python",
        "python3", "pip", "uv", "pytest", "node", "rustup", "rustc", "cc",
        "clang", "gcc", "curl", "wget", "git", "gh", "docker", "podman",
        "kubectl", "sudo", "env", "just", "task",
    ];
    let first_is_cmd = COMMANDS.contains(&first)
        || first.starts_with("./")
        || first.starts_with("../")
        || first.starts_with("/");
    if !first_is_cmd {
        return false;
    }
    // Strip any trailing ` # ...` comment before the prose check — comment
    // bodies (`# expected: 42/42`) are fine in a runnable command and should
    // not trip the prose detector.
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

/// Strip a trailing shell comment (`  # ...`) off a command so the combined
/// smoke-test chain doesn't carry annotations that would comment out the
/// following `&&`. Example: `cargo test # expected: 79/90` → `cargo test`.
fn strip_trailing_comment(s: &str) -> String {
    // Find the first `  #` (two-space-hash) or ` #` at the end. Only strip if
    // the `#` isn't inside a quoted string — conservative: look for the
    // rightmost ` # ` / `  #` that comes after the last quote close.
    let last_quote = s.rfind(['"', '\'']);
    // Search for `  #` (style-guide comment) first, then ` # `.
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

/// Scan prose (typically a `@hack:handoff(...)` body) for `path:line` code
/// locations so the pickup prompt can surface them as a list. Matches segments
/// like `src/foo.rs:42`, `crates/bar/src/baz.rs:12833`, `banana_graph_builder.rs:11256`.
///
/// Conservative: only emits unique entries, in source-order. The regex is
/// inlined rather than pulling in a regex crate dep — fast enough for one
/// handoff body per prompt.
fn extract_code_locations(prose: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bytes = prose.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next `:NNN` sequence.
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
        // Walk backwards from `colon` to the start of the path token. A path
        // token is a run of [A-Za-z0-9_./-] with at least one `/` or `.rs`/
        // `.md`/`.toml`/`.ts` extension before the colon.
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
        // Require an extension or at least one `/` to avoid matching e.g. "R008:".
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

/// Derive an epic's status from the statuses of its child relays.
///
/// - `None` / empty slice → `"active"` (treat as in-planning / freshly declared)
/// - any child not in `Review` or `Done` → `"active"`
/// - all children in `Review`/`Done` → `"closed"`
///
/// Archived children don't appear in source at all, so they're simply absent
/// from the slice — that's handled the same as "terminal" by the any-active
/// check below.
fn compute_epic_status(children: Option<&[TicketStatus]>) -> String {
    let Some(children) = children else {
        return "active".to_string();
    };
    if children.is_empty() {
        return "active".to_string();
    }
    let any_active = children.iter().any(|s| {
        !matches!(s, TicketStatus::Review | TicketStatus::Done)
    });
    if any_active {
        "active".to_string()
    } else {
        "closed".to_string()
    }
}

fn is_hack_relevant(kind: &ArchKind) -> bool {
    matches!(
        kind,
        ArchKind::Ticket { .. }
            | ArchKind::Relay { .. }
            | ArchKind::Kind(_)
            | ArchKind::Status(_)
            | ArchKind::Assignee(_)
            | ArchKind::Phase(_)
            | ArchKind::Parent(_)
            | ArchKind::HackSeverity(_)
            | ArchKind::Handoff(_)
            | ArchKind::Next(_)
            | ArchKind::Cleanup(_)
            | ArchKind::Verify(_)
            | ArchKind::Gotcha(_)
            | ArchKind::Assumes(_)
            | ArchKind::See(_)
            | ArchKind::DependsOn { .. }
    )
}

/// Build a Ticket or Relay from a group of annotations on the same target.
fn build_item(anns: &[&ArchAnnotation]) -> Option<Ticket> {
    let mut id = None;
    let mut title = String::new();
    let mut item_type = None;
    let mut kind = None;
    let mut status = TicketStatus::Open;
    let mut assignee = None;
    let mut phase = None;
    let mut parent = None;
    let mut severity = None;
    let mut handoff = Vec::new();
    let mut next_steps = Vec::new();
    let mut cleanup = Vec::new();
    let mut verify = Vec::new();
    let mut gotchas = Vec::new();
    let mut assumes = Vec::new();
    let mut see_also = Vec::new();
    let mut depends_on = Vec::new();

    let first = anns.first()?;
    let file = first.file.clone();
    let line = first.line;
    let target = first.target.clone();

    for ann in anns {
        match &ann.kind {
            ArchKind::Ticket { id: tid, title: ttitle } => {
                id = Some(tid.clone());
                title = ttitle.clone();
                item_type = Some(ItemType::Ticket);
            }
            ArchKind::Relay { id: rid, title: rtitle } => {
                id = Some(rid.clone());
                title = rtitle.clone();
                item_type = Some(ItemType::Relay);
                if status == TicketStatus::Open {
                    status = TicketStatus::Handoff;
                }
            }
            ArchKind::Kind(k) => kind = Some(k.clone()),
            ArchKind::Status(s) => status = TicketStatus::parse(s),
            ArchKind::Assignee(a) => assignee = Some(a.clone()),
            ArchKind::Phase(p) => phase = Some(p.clone()),
            ArchKind::Parent(p) => parent = Some(p.clone()),
            ArchKind::HackSeverity(s) => severity = Some(s.clone()),
            ArchKind::Handoff(h) => handoff.push(h.clone()),
            ArchKind::Next(n) => next_steps.push(n.clone()),
            ArchKind::Cleanup(c) => cleanup.push(c.clone()),
            ArchKind::Verify(v) => verify.push(v.clone()),
            ArchKind::Gotcha(g) => gotchas.push(g.clone()),
            ArchKind::Assumes(a) => assumes.push(a.clone()),
            ArchKind::See(s) => see_also.push(s.clone()),
            ArchKind::DependsOn { target: dep, .. } => depends_on.push(dep.clone()),
            _ => {}
        }
    }

    let id = id?;
    let item_type = item_type?;

    // Infer kind from legacy aliases (bug/feature/task parsed as Ticket)
    // or from ID prefix if no explicit @hack:kind
    if kind.is_none() && item_type == ItemType::Ticket {
        kind = match id.chars().next() {
            Some('B') | Some('b') => Some("bug".to_string()),
            Some('F') | Some('f') => Some("feature".to_string()),
            _ => None, // T or unknown — badge() handles the default
        };
    }

    Some(Ticket {
        id,
        title,
        item_type,
        kind,
        status,
        assignee,
        phase,
        parent,
        severity,
        handoff,
        next_steps,
        cleanup,
        verify,
        gotchas,
        assumes,
        depends_on,
        see_also,
        file,
        line,
        target,
        is_epic: false,       // resolved later by TicketBoard::resolve_epics
        epic_status: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_from_source;
    use std::path::Path;

    #[test]
    fn test_ticket_extraction() {
        let source = r#"
//! @hack:ticket(F01, "Implement voice allocation")
//! @hack:status(in-progress)
//! @hack:assignee(agent:claude)
//! @hack:phase(P2)
//! @arch:see(architecture/vivarium/voice_allocation.md)

pub mod voice_alloc;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let ticket = &board.tickets[0];
        assert_eq!(ticket.id, "F01");
        assert_eq!(ticket.item_type, ItemType::Ticket);
        assert_eq!(ticket.kind.as_deref(), Some("feature")); // inferred from F prefix
        assert_eq!(ticket.badge(), "F");
        assert_eq!(ticket.status, TicketStatus::InProgress);
        assert_eq!(ticket.assignee.as_deref(), Some("agent:claude"));
        assert_eq!(ticket.phase.as_deref(), Some("P2"));
    }

    #[test]
    fn test_stacked_tickets_in_one_module_block() {
        // Regression: several @hack:ticket / @hack:relay headers in the same
        // //! block must produce distinct tickets — each one owns the
        // annotations that follow it, bounded by the next header.
        let source = r#"
//! @hack:relay(R010, "Parent relay")
//! @hack:status(handoff)
//! @hack:handoff("R010 handoff text")
//! @hack:ticket(R010-T1, "First sub-ticket")
//! @hack:status(in-progress)
//! @hack:next("T1 next step")
//! @hack:ticket(R010-T2, "Second sub-ticket")
//! @hack:status(open)
//! @hack:next("T2 next step")

pub mod thing;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 3, "expected 3 distinct items");

        let r010 = board.get("R010").expect("R010 must exist");
        assert_eq!(r010.item_type, ItemType::Relay);
        assert_eq!(r010.status, TicketStatus::Handoff);
        assert_eq!(r010.handoff, vec!["R010 handoff text".to_string()]);
        assert!(r010.next_steps.is_empty(), "relay must not inherit sub-ticket next() lines");

        let t1 = board.get("R010-T1").expect("R010-T1 must exist");
        assert_eq!(t1.status, TicketStatus::InProgress);
        assert_eq!(t1.next_steps, vec!["T1 next step".to_string()]);

        let t2 = board.get("R010-T2").expect("R010-T2 must exist");
        assert_eq!(t2.status, TicketStatus::Open);
        assert_eq!(t2.next_steps, vec!["T2 next step".to_string()]);
    }

    #[test]
    fn test_bug_extraction() {
        let source = r#"
//! @hack:ticket(B01, "Panic on zero-length buffer")
//! @hack:status(open)
//! @hack:severity(high)

pub fn process_buffer() {}
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let ticket = &board.tickets[0];
        assert_eq!(ticket.id, "B01");
        assert_eq!(ticket.badge(), "B"); // inferred from B prefix
        assert_eq!(ticket.severity.as_deref(), Some("high"));
    }

    #[test]
    fn test_legacy_bug_alias() {
        // @hack:bug(...) still works as an alias for @hack:ticket(...)
        let source = r#"
//! @hack:bug(B02, "Old-style bug annotation")
//! @hack:status(open)

pub fn buggy() {}
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        assert_eq!(board.tickets[0].badge(), "B");
    }

    #[test]
    fn test_relay_extraction_and_prompt() {
        let source = r#"
//! @hack:relay(R001, "ProcessBlock Unification Phase 4")
//! @hack:assignee(agent:claude)
//! @hack:phase(P4)
//! @hack:handoff("AudioProcessor -> ProcessBlock across ~80 files.")
//! @hack:next("Simplify add_control_node_unified -> add_node_named")
//! @hack:next("Inline/remove intermediary functions")
//! @hack:cleanup("CvRbjFilter is dead code")
//! @hack:verify("cargo test -p vivarium")
//! @arch:see(architecture/vivarium/PROCESS_BLOCK_UNIFICATION.md)

pub mod process_block;
"#;
        let annotations = extract_from_source(source, Path::new("engine.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let relay = &board.tickets[0];
        assert_eq!(relay.id, "R001");
        assert_eq!(relay.item_type, ItemType::Relay);
        assert_eq!(relay.badge(), "R");
        assert_eq!(relay.status, TicketStatus::Handoff);
        assert_eq!(relay.next_steps.len(), 2);
        assert_eq!(relay.cleanup.len(), 1);
        assert_eq!(relay.verify.len(), 1);

        let prompt = relay.to_prompt();
        assert!(prompt.contains("# Continue: R001"));
        assert!(prompt.contains("ProcessBlock Unification"));
        assert!(prompt.contains("## Verification"));
        assert!(prompt.contains("cargo test"));
        // First-action block: relay is in Handoff, so the pickup verb is
        // `board move <ID> active`, not a hand-edit of the status line.
        assert!(
            prompt.contains("rs-hack board move R001 active"),
            "expected pickup verb in prompt:\n{}",
            prompt
        );

        let md = board.to_markdown();
        assert!(md.contains("[R] R001"));
    }

    #[test]
    fn test_epic_inferred_from_children() {
        // R001 has no explicit kind=epic, but R002 declares @hack:parent(R001).
        // That parent-pointer alone should mark R001 as an epic.
        let source = r#"
//! @hack:relay(R001, "Big epic")
//! @hack:status(handoff)

pub mod epic_root;

mod child {
    //! @hack:relay(R002, "child thread")
    //! @hack:parent(R001)
    //! @hack:status(in-progress)
}
"#;
        let annotations = extract_from_source(source, Path::new("epic.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let r001 = board.get("R001").unwrap();
        assert!(r001.is_epic, "R001 should be epic (has child R002)");
        assert_eq!(r001.epic_status.as_deref(), Some("active"));
        let r002 = board.get("R002").unwrap();
        assert!(!r002.is_epic, "R002 has no children and no explicit kind");
    }

    #[test]
    fn test_epic_explicit_kind_no_children_is_active() {
        let source = r#"
//! @hack:relay(R010, "Planning epic")
//! @hack:kind(epic)
//! @hack:status(open)

pub mod plan;
"#;
        let annotations = extract_from_source(source, Path::new("e.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let r010 = board.get("R010").unwrap();
        assert!(r010.is_epic);
        // No children yet — default "active" (planning)
        assert_eq!(r010.epic_status.as_deref(), Some("active"));
    }

    #[test]
    fn test_epic_closed_when_all_children_terminal() {
        let source = r#"
//! @hack:relay(R020, "Parent")
//! @hack:kind(epic)

pub mod parent;

mod child_a {
    //! @hack:relay(R021, "a")
    //! @hack:parent(R020)
    //! @hack:status(review)
}

mod child_b {
    //! @hack:relay(R022, "b")
    //! @hack:parent(R020)
    //! @hack:status(done)
}
"#;
        let annotations = extract_from_source(source, Path::new("e.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let r020 = board.get("R020").unwrap();
        assert!(r020.is_epic);
        assert_eq!(r020.epic_status.as_deref(), Some("closed"));
    }

    #[test]
    fn test_epic_active_when_any_child_live() {
        let source = r#"
//! @hack:relay(R030, "Parent")

pub mod parent;

mod a {
    //! @hack:relay(R031, "a")
    //! @hack:parent(R030)
    //! @hack:status(review)
}

mod b {
    //! @hack:relay(R032, "b")
    //! @hack:parent(R030)
    //! @hack:status(in-progress)
}
"#;
        let annotations = extract_from_source(source, Path::new("e.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let r030 = board.get("R030").unwrap();
        assert!(r030.is_epic);
        assert_eq!(r030.epic_status.as_deref(), Some("active"));
    }

    #[test]
    fn test_compound_id_infers_parent_and_is_not_epic_child() {
        // R007 is a regular relay. R007-T1 is a sub-ticket inside that
        // relay. The sub-ticket should:
        //   - infer parent = "R007" from its ID even without @hack:parent
        //   - NOT promote R007 to epic status (only bare-R children do that)
        let source = r#"
//! @hack:relay(R007, "replay coverage")
//! @hack:status(handoff)

pub mod relay_root;

mod child {
    //! @hack:ticket(R007-T1, "cluster A")
    //! @hack:status(open)
}
"#;
        let annotations = extract_from_source(source, Path::new("e.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let t1 = board.get("R007-T1").unwrap();
        assert_eq!(t1.parent.as_deref(), Some("R007"));
        assert_eq!(t1.item_type, ItemType::Ticket);
        let r007 = board.get("R007").unwrap();
        assert!(
            !r007.is_epic,
            "R007 has only a compound sub-ticket — not an epic"
        );
    }

    #[test]
    fn test_extract_code_locations_from_prose() {
        let prose = "Fixed classify_binding() at banana_graph_builder.rs:11256+. \
                     Also see src/foo.rs:42 and crates/vivarium/lib.rs:1. \
                     Ignore R008: and plain colons. Tests at tests/fixtures.rs:9.";
        let locs = extract_code_locations(prose);
        assert!(locs.contains(&"banana_graph_builder.rs:11256".to_string()));
        assert!(locs.contains(&"src/foo.rs:42".to_string()));
        assert!(locs.contains(&"crates/vivarium/lib.rs:1".to_string()));
        assert!(locs.contains(&"tests/fixtures.rs:9".to_string()));
        // No false positive on `R008:`
        assert!(!locs.iter().any(|l| l.starts_with("R008")));
    }

    #[test]
    fn test_verify_heuristic_separates_commands_from_prose() {
        let source = r#"
//! @hack:ticket(T01, "t")
//! @hack:status(handoff)
//! @hack:verify("cargo check -p foo")
//! @hack:verify("cargo test --lib  # expected: 42/42")
//! @hack:verify("cargo test -p vivarium-banana-nodes --lib is clean (no new errors)")
//! @hack:verify("Manual: launch example and listen for modulation")

pub fn thing() {}
"#;
        let anns = extract_from_source(source, Path::new("f.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let prompt = board.get("T01").unwrap().to_prompt();

        // Runnable commands get fenced.
        assert!(prompt.contains("```bash\ncargo check -p foo\n```"));
        assert!(prompt.contains("```bash\ncargo test --lib  # expected: 42/42\n```"));

        // Prose-laced items stay plain bullets — they must NOT be inside a
        // ```bash fence (because they're criteria, not runnable commands).
        assert!(prompt.contains("- cargo test -p vivarium-banana-nodes --lib is clean (no new errors)"));
        assert!(prompt.contains("- Manual: launch example and listen for modulation"));

        // Combined smoke test: only actual commands, comments stripped off.
        let smoke_start = prompt.find("Combined smoke test:").expect("smoke test present");
        let smoke_block = &prompt[smoke_start..];
        assert!(smoke_block.contains("cargo check -p foo && cargo test --lib"));
        assert!(
            !smoke_block.contains("is clean"),
            "smoke test must not include prose"
        );
        assert!(
            !smoke_block.contains("# expected"),
            "smoke test must strip trailing # comments"
        );
    }

    #[test]
    fn test_handoff_single_vs_multiple_rendering() {
        // Single @hack:handoff(...) → paragraph.
        let single = r#"
//! @hack:ticket(T02, "single")
//! @hack:status(handoff)
//! @hack:handoff("One long paragraph covering everything that was done in this session.")

pub fn s() {}
"#;
        let anns = extract_from_source(single, Path::new("s.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let t = board.get("T02").unwrap();
        assert_eq!(t.handoff.len(), 1);
        let p = t.to_prompt();
        // Paragraph form — no bullet prefix on the handoff body.
        assert!(p.contains("\n\nOne long paragraph"));
        assert!(!p.contains("- One long paragraph"));

        // Multiple @hack:handoff(...) → bullets.
        let multi = r#"
//! @hack:ticket(T03, "multi")
//! @hack:status(handoff)
//! @hack:handoff("Fixed resolver at ctx.rs:2263.")
//! @hack:handoff("Added symbol table for note_v2 in koda_relay_node.rs:583.")
//! @hack:handoff("Updated MIX_UNIFICATION.md §Phase 9b.")

pub fn m() {}
"#;
        let anns = extract_from_source(multi, Path::new("m.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let t = board.get("T03").unwrap();
        assert_eq!(t.handoff.len(), 3);
        let p = t.to_prompt();
        assert!(p.contains("- Fixed resolver at ctx.rs:2263."));
        assert!(p.contains("- Added symbol table for note_v2 in koda_relay_node.rs:583."));
        assert!(p.contains("- Updated MIX_UNIFICATION.md §Phase 9b."));
        // Locations extracted from across all handoff lines:
        assert!(p.contains("- `ctx.rs:2263`"));
        assert!(p.contains("- `koda_relay_node.rs:583`"));
    }

    #[test]
    fn test_gotcha_and_assumes_extracted_and_rendered() {
        let source = r#"
//! @hack:ticket(T42, "handle the thing")
//! @hack:status(handoff)
//! @hack:handoff("Scaffolded the thing in src/thing.rs:10. Left unsafe blocks TBD.")
//! @hack:gotcha("banana_nodes --lib has pre-existing compile errors unrelated to this ticket")
//! @hack:gotcha("cargo test -p vivarium-banana-nodes --lib fails for unrelated IRFunction churn — don't fix")
//! @hack:assumes("Koda IR-lowering flattens @children arm tuples into IRValue::List")
//! @hack:verify("cargo check -p vivarium-banana-nodes --lib")
//! @hack:verify("cargo test -p banana --lib nodes::split::")

pub fn thing() {}
"#;
        let annotations = extract_from_source(source, Path::new("f.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let t = board.get("T42").unwrap();
        assert_eq!(t.gotchas.len(), 2);
        assert_eq!(t.assumes.len(), 1);

        let prompt = t.to_prompt();
        // Gotchas render above Context
        let gotcha_pos = prompt.find("## ⚠ Gotchas").expect("gotcha section present");
        let context_pos = prompt.find("## Context").unwrap();
        assert!(gotcha_pos < context_pos, "gotchas must precede context");

        // Assumptions block present
        assert!(prompt.contains("## Assumptions"));
        assert!(prompt.contains("IRValue::List"));

        // Verify rendered as fenced bash + combined smoke test
        assert!(prompt.contains("```bash\ncargo check -p vivarium-banana-nodes --lib\n```"));
        assert!(prompt.contains("Combined smoke test"));
        assert!(prompt.contains(
            "cargo check -p vivarium-banana-nodes --lib && cargo test -p banana --lib nodes::split::"
        ));

        // Trimmed playbook — no longer dumps R2/R3/R4/C1 inline
        assert!(prompt.contains("**R1**"));
        assert!(!prompt.contains("**R3 —"));
        assert!(prompt.contains("rs-hack board rules"));

        // File:line extraction from handoff prose
        assert!(prompt.contains("Locations referenced above"));
        assert!(prompt.contains("src/thing.rs:10"));
    }

    #[test]
    fn test_parent_relay() {
        let source = r#"
//! @hack:relay(R005, "CV Port Bridge")
//! @hack:parent(R001)
//! @hack:assignee(agent:claude)

pub mod cv_bridge;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets[0].parent.as_deref(), Some("R001"));

        let prompt = board.tickets[0].to_prompt();
        assert!(prompt.contains("**Parent relay**: R001"));
    }

    #[test]
    fn test_ticket_with_parent_and_phase() {
        let source = r#"
//! @hack:ticket(T01, "Add cv_to_hz to RbjBiquadNode")
//! @hack:parent(R005)
//! @hack:phase(P1)
//! @hack:status(open)

pub struct RbjBiquadNode;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        let t = &board.tickets[0];
        assert_eq!(t.id, "T01");
        assert_eq!(t.badge(), "T");
        assert_eq!(t.parent.as_deref(), Some("R005"));
        assert_eq!(t.phase.as_deref(), Some("P1"));
    }

    #[test]
    fn test_board_markdown() {
        let source = r#"
//! @hack:ticket(T01, "Do the thing")
//! @hack:status(done)

pub mod a;
"#;
        let annotations = extract_from_source(source, Path::new("a.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let md = board.to_markdown();
        assert!(md.contains("1/1 complete"));
    }

    #[test]
    fn test_mixed_arch_and_hack() {
        let source = r#"
//! @arch:layer(vivarium)
//! @arch:role(synthesis)
//! @hack:ticket(T03, "Add polyphony support")
//! @hack:status(claimed)
//! @hack:assignee(agent:claude)

pub struct Synth;
"#;
        let annotations = extract_from_source(source, Path::new("synth.rs")).unwrap();

        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        assert_eq!(board.tickets[0].id, "T03");

        let graph = crate::graph::ArchGraph::from_annotations(annotations);
        let ctx = crate::query::get_file_context(&graph, "synth");
        assert_eq!(ctx.layer.as_deref(), Some("vivarium"));
    }
}
