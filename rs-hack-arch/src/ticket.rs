//! @arch:layer(arch)
//! @arch:role(ticket)
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
//!

use crate::annotation::{AnnotationTarget, ArchAnnotation, ArchKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

    /// Annotation-form name (matches what `parse()` accepts and what
    /// `@hack:status(...)` writes in source).
    pub fn as_annotation(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::InProgress => "in-progress",
            Self::Handoff => "handoff",
            Self::Review => "review",
            Self::Done => "done",
        }
    }

    /// True when the status is "still in flight" — not yet in Review/Done.
    /// Matches the set of statuses the Rule08 sub-ticket cycle iterates over
    /// (Open → Claimed → InProgress → Handoff).
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            Self::Open | Self::Claimed | Self::InProgress | Self::Handoff
        )
    }
}

impl std::fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_annotation())
    }
}

/// One source location of a ticket's defining annotation. The first
/// entry of `Ticket::files` is the canonical (lex-first) location.
///
/// The field is named `path` (not `file`) to disambiguate from the
/// top-level `Ticket::file` alias — consumers reading nested JSON see
/// `files[i].path` and `files[i].line`, which reads cleanly as a
/// `path:line` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketLocation {
    pub path: PathBuf,
    pub line: usize,
}

/// One value of a scalar field, attributed to its source location.
/// Surfaced inside `Ticket::conflicts` when the same `@hack:` ID is
/// declared in multiple files with disagreeing scalar metadata
/// (e.g. one file says `status(open)`, another `status(review)`).
///
/// The Ticket's top-level scalar holds the lex-first value
/// deterministically; `conflicts` lists every observed value with its
/// `(path, line)` so the disagreement is loud rather than silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConflict {
    pub value: String,
    pub path: PathBuf,
    pub line: usize,
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

    /// Convenience alias for `files[0].path` — the lex-first source
    /// location. Kept for back-compat with consumers that read a single
    /// path; new code should iterate `files`.
    pub file: PathBuf,

    /// Convenience alias for `files[0].line`.
    pub line: usize,

    /// AST target
    pub target: AnnotationTarget,

    /// Every source occurrence of this ticket's `@hack:ticket` /
    /// `@hack:relay` header, sorted by `(file, line)`. Always populated
    /// with at least one entry. Length > 1 means the same ID is
    /// declared in multiple files — that's a smell to resolve (Rule11):
    /// either dedupe to one home or renumber one of the occurrences.
    pub files: Vec<TicketLocation>,

    /// Per-field disagreements when the same ID appears in multiple
    /// files. Keys are field names (`status`, `assignee`, `phase`,
    /// `title`, `kind`, `severity`, `parent`); values list every
    /// observed value with its source location. The Ticket's top-level
    /// scalar holds the lex-first value (deterministic winner);
    /// `conflicts` exposes the disagreement so the board / pickup
    /// prompt can flag it for resolution. Empty for the common case.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty", default)]
    pub conflicts: std::collections::BTreeMap<String, Vec<FieldConflict>>,

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

/// How many live sub-tickets hang off a child — counted by their status
/// bucket so the container prompt can render "2 open + 1 in-flight + 0
/// handoff" per child relay. Drives the epic two-level walk; for a plain
/// relay-with-subtickets the counts are unused.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildLiveCounts {
    pub open: usize,
    /// `Claimed` + `InProgress` — work someone is already on.
    pub in_flight: usize,
    pub handoff: usize,
}

impl ChildLiveCounts {
    pub fn total(&self) -> usize {
        self.open + self.in_flight + self.handoff
    }

    fn bump(&mut self, status: &TicketStatus) {
        match status {
            TicketStatus::Open => self.open += 1,
            TicketStatus::Claimed | TicketStatus::InProgress => self.in_flight += 1,
            TicketStatus::Handoff => self.handoff += 1,
            TicketStatus::Review | TicketStatus::Done => {}
        }
    }

    /// Render as a compact inline string, e.g. `2 open · 1 in-flight`.
    /// Empty buckets are dropped. Returns `""` when totally idle (no live
    /// sub-tickets) so the caller can elide the suffix.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.open > 0 {
            parts.push(format!("{} open", self.open));
        }
        if self.in_flight > 0 {
            parts.push(format!("{} in-flight", self.in_flight));
        }
        if self.handoff > 0 {
            parts.push(format!("{} handoff", self.handoff));
        }
        parts.join(" · ")
    }
}

/// Hierarchy context for building a pickup prompt. Lets the prompt see
/// its place in the tree: parent (for sub-ticket inheritance), live
/// children (for a relay-with-subtickets or an epic), and grandchild
/// counts (for epics, which need a two-level walk).
///
/// Empty `PromptContext::default()` reproduces the legacy flat-prompt
/// behavior: no parent, no children, no grandchildren.
#[derive(Default, Debug)]
pub struct PromptContext<'a> {
    /// Children still in flight — sub-tickets for a relay, child relays
    /// for an epic. Sorted by ID. Used to render the "work one at a
    /// time" section and pick the next-live starter.
    pub live_children: Vec<&'a Ticket>,

    /// Per-child live-subticket counts. Only populated when the ticket
    /// itself is an epic (its `live_children` are child relays, and
    /// each of those can own its own sub-tickets). Keyed by child ID.
    pub child_live_counts: HashMap<String, ChildLiveCounts>,

    /// Parent relay, if any. When present, the prompt inherits the
    /// parent's gotchas + verify smoke so a sub-ticket pickup doesn't
    /// relearn traps the relay already paid to discover.
    pub parent: Option<&'a Ticket>,
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
        for (_id, mut anns) in by_ticket {
            // Sort by (file, line) so build_item's "first wins" is
            // deterministic when the same ID appears in multiple files.
            // Without this, HashMap iteration order made `file` flip
            // between scans and the events log waffled.
            anns.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then_with(|| a.line.cmp(&b.line))
            });
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

    /// Build the hierarchy context used by `Ticket::to_prompt_with_ctx`.
    /// Walks the board once to gather: live children of `id`, grandchild
    /// counts (for epics — each child relay's own live sub-tickets), and
    /// the parent if `id` is itself a sub-ticket / child relay.
    ///
    /// Returns an empty `PromptContext` if the id isn't on the board —
    /// the caller is expected to have already resolved the ticket.
    pub fn build_prompt_context(&self, id: &str) -> PromptContext<'_> {
        let Some(ticket) = self.get(id) else {
            return PromptContext::default();
        };

        let mut live_children: Vec<&Ticket> = self
            .tickets
            .iter()
            .filter(|t| t.parent.as_deref() == Some(ticket.id.as_str()))
            .filter(|t| t.status.is_live())
            .collect();
        live_children.sort_by(|a, b| a.id.cmp(&b.id));

        // Epic grandchild counts: for each live child relay, count how many
        // of its own sub-tickets are still in flight. Only meaningful when
        // the current ticket is an epic — but cheap to compute either way,
        // so we always populate (the prompt builder ignores the map for
        // non-epic containers).
        let mut child_live_counts: HashMap<String, ChildLiveCounts> = HashMap::new();
        if ticket.is_epic {
            for child in &live_children {
                let mut counts = ChildLiveCounts::default();
                for grand in &self.tickets {
                    if grand.parent.as_deref() == Some(child.id.as_str()) {
                        counts.bump(&grand.status);
                    }
                }
                child_live_counts.insert(child.id.clone(), counts);
            }
        }

        let parent = ticket
            .parent
            .as_deref()
            .and_then(|pid| self.get(pid));

        PromptContext {
            live_children,
            child_live_counts,
            parent,
        }
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
    /// Equivalent to `to_prompt_with_ctx(&PromptContext::default())` —
    /// no hierarchy context. Callers that have a `TicketBoard` in hand
    /// should prefer `to_prompt_with_ctx` (fed by
    /// `TicketBoard::build_prompt_context`) so the prompt can surface
    /// live sub-tickets, parent gotchas, and epic grandchild counts.
    pub fn to_prompt(&self) -> String {
        self.to_prompt_with_ctx(&PromptContext::default())
    }

    /// Legacy shim: build a prompt from a flat children list (no parent,
    /// no grandchild counts). Retained so existing callers and tests
    /// that only pass children don't need to know about `PromptContext`.
    pub fn to_prompt_with_context(&self, live_children: &[&Ticket]) -> String {
        let ctx = PromptContext {
            live_children: live_children.to_vec(),
            child_live_counts: HashMap::new(),
            parent: None,
        };
        self.to_prompt_with_ctx(&ctx)
    }

    /// Build the continuation prompt with full hierarchy context.
    ///
    /// The shape of the prompt adapts to the ticket's place in the tree:
    /// - **Leaf sub-ticket** (`ctx.parent.is_some()`, no live_children):
    ///   inherits the parent relay's gotchas and combined verify smoke
    ///   so the pickup doesn't relearn traps the relay already paid for.
    /// - **Relay with sub-tickets** (live_children non-empty, not epic):
    ///   hoists the "Sub-tickets in flight" section above `next_steps`
    ///   and relabels `next_steps` as "Follow-on spawns" — the baton is
    ///   the sub-ticket cycle (Rule08), not the author's spawn list.
    /// - **Epic** (`self.is_epic` + live_children are child relays):
    ///   two-level walk — each child relay is rendered with its own
    ///   live-subticket counts, and the starter points at the earliest
    ///   live child (preferring ones that already have work in flight).
    /// - **Plain ticket** (no parent, no children): classic flat prompt.
    ///
    /// Picker fix: earliest-live by ID includes `Claimed`/`InProgress`,
    /// not just `Open`/`Handoff`. A relay with an already-in-progress
    /// sub-ticket points at that sub-ticket with a "continue it" verb
    /// rather than skipping to the next fresh one.
    pub fn to_prompt_with_ctx(&self, ctx: &PromptContext) -> String {
        let live_children = ctx.live_children.as_slice();
        let is_container = !live_children.is_empty();
        let is_epic_container = is_container && self.is_epic;

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

        // Inherited gotchas from parent relay. Sub-ticket pickups routinely
        // starve for parser traps / pre-existing breakage the relay already
        // discovered — surface them here, clearly marked as inherited so
        // the reader knows the source.
        if let Some(parent) = ctx.parent {
            if !parent.gotchas.is_empty() {
                prompt.push_str(&format!(
                    "## ⚠ Gotchas inherited from {} (read first)\n\n",
                    parent.id
                ));
                for g in &parent.gotchas {
                    prompt.push_str(&format!("- {}\n", g));
                }
                prompt.push('\n');
            }
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

        // Container section — hoisted above next_steps so the Rule08 cycle
        // (or epic child-relay walk) reads as THE next action. For containers
        // the relay/epic's own `next_steps` are usually forward-spawn noise
        // (follow-on tickets to file), not the baton.
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
                    let status = child.status.column().to_lowercase();
                    let counts = ctx
                        .child_live_counts
                        .get(&child.id)
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
                        .assignee
                        .as_deref()
                        .map(|a| format!(" · {}", a))
                        .unwrap_or_default();
                    prompt.push_str(&format!(
                        "- **{}** [{}]{}{} · {}\n",
                        child.id, status, assignee, counts_suffix, child.title
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
            }

            // Picker: earliest LIVE child by ID order. Includes InProgress /
            // Claimed — the R005 fix. Previously we only matched Open|Handoff,
            // which skipped an already-in-progress sub-ticket and pointed at
            // the next fresh one instead.
            if let Some(next) = live_children.first().copied() {
                let line = match next.status {
                    TicketStatus::Open => format!(
                        "\nStart with:\n\n```bash\nrs-hack board claim {}\n```\n\n",
                        next.id
                    ),
                    TicketStatus::Handoff => format!(
                        "\nStart with:\n\n```bash\nrs-hack board move {} active\n```\n\n",
                        next.id
                    ),
                    TicketStatus::Claimed | TicketStatus::InProgress => format!(
                        "\nContinue with **{}** — already in flight ({}). Pull its pickup \
                         prompt:\n\n```bash\nrs-hack board tickets --prompt {}\n```\n\n",
                        next.id,
                        next.status.column().to_lowercase(),
                        next.id
                    ),
                    // Review/Done shouldn't appear in live_children, but
                    // render something coherent if one slips through.
                    _ => format!("\nStart with **{}**.\n\n", next.id),
                };
                prompt.push_str(&line);
            } else {
                prompt.push('\n');
            }
        }

        if !self.next_steps.is_empty() {
            // When the relay has live children, its own `next_steps` is the
            // author's forward-spawn list (follow-on tickets to file) — not
            // the current baton. Relabel so a pickup agent doesn't confuse
            // them with the sub-ticket cycle, which is hoisted above.
            if is_container {
                prompt.push_str(
                    "## Follow-on spawns (not the baton — see sub-tickets above)\n\n",
                );
            } else {
                prompt.push_str("## Next steps\n\n");
            }
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

        // Parent-relay verify smoke: a sub-ticket pickup should be able to
        // run the relay's catch-all regression chain without opening the
        // parent card. Render only shell commands (prose criteria are too
        // context-heavy to inherit), as a combined smoke for copy-paste.
        if let Some(parent) = ctx.parent {
            let parent_cmds: Vec<String> = parent
                .verify
                .iter()
                .filter(|v| looks_like_shell_command(v))
                .map(|v| strip_trailing_comment(v))
                .collect();
            if !parent_cmds.is_empty() {
                prompt.push_str(&format!(
                    "## Verification inherited from {}\n\n",
                    parent.id
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

        prompt.push_str("## First action\n\n");
        match self.status {
            TicketStatus::Open => {
                prompt.push_str(&format!(
                    "Claim this ticket — one atomic command flips status and assignee (Rule01):\n\n\
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
                    "Pick up the baton — one atomic command flips status and assignee (Rule01):\n\n\
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
        // a generic list. Rule08 only matters when the relay has live children.
        prompt.push_str("## Playbook\n\n");
        if !live_children.is_empty() {
            prompt.push_str(
                "Load-bearing rules for this pickup: **Rule01** (claim first — above), \
                 **Rule08** (sub-ticket cycle — above), **Col01** (three end-states — below). \
                 Full ruleset: `rs-hack board rules --context pickup` (or `finishing` \
                 when you wrap up).\n\n",
            );
        } else {
            prompt.push_str(
                "Load-bearing rules for this pickup: **Rule01** (claim first — above), \
                 **Col01** (three end-states — below). Full ruleset: \
                 `rs-hack board rules --context pickup` (or `finishing` when you wrap up).\n\n",
            );
        }
        prompt.push_str(
            "Inspect any related ticket: `rs-hack board show <ID>` \
             (compact view) or `rs-hack board show <ID> --prompt` (full \
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
        if !self.cleanup.is_empty() {
            prompt.push_str(&format!(
                "{}. Address cleanup items if time permits.\n",
                step
            ));
            step += 1;
        }
        prompt.push_str(&format!("{}. Pick the right end-state (Col01):\n", step));
        prompt.push_str(&format!(
            "   - **More work remains (another phase, another agent):** \
                `rs-hack board move {} handoff --handoff \"what you just finished\" --next \"first concrete next step\"` \
                — same R-number, baton moves forward in place (Rule03).\n",
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

/// One file's contribution to a (potentially multi-file) ticket.
/// Scalars are last-write-wins within the file; vec fields accumulate.
/// Multiple `PartialTicket`s from different files are then CRDT-merged
/// in `build_item`: vecs union, scalars take the lex-first non-empty
/// value with disagreements surfaced via `Ticket::conflicts`.
#[derive(Default)]
struct PartialTicket {
    file: PathBuf,
    header_line: usize, // line of the first defining @hack:ticket/@hack:relay
    target: Option<AnnotationTarget>,
    id: Option<String>,
    title: Option<String>,
    item_type: Option<ItemType>,
    kind: Option<String>,
    status: Option<TicketStatus>,
    assignee: Option<String>,
    phase: Option<String>,
    parent: Option<String>,
    severity: Option<String>,
    handoff: Vec<String>,
    next_steps: Vec<String>,
    cleanup: Vec<String>,
    verify: Vec<String>,
    gotchas: Vec<String>,
    assumes: Vec<String>,
    see_also: Vec<String>,
    depends_on: Vec<String>,
}

/// Fold one file's worth of annotations (already sorted by line) into a
/// `PartialTicket`. Last-write-wins for scalars (matches the original
/// single-file behavior); vecs accumulate in source order.
fn fold_file(file: PathBuf, anns: &[&ArchAnnotation]) -> Option<PartialTicket> {
    let first = anns.first()?;
    let mut p = PartialTicket {
        file,
        header_line: first.line,
        target: Some(first.target.clone()),
        ..Default::default()
    };
    for ann in anns {
        match &ann.kind {
            ArchKind::Ticket { id, title } => {
                p.id = Some(id.clone());
                p.title = Some(title.clone());
                p.item_type = Some(ItemType::Ticket);
            }
            ArchKind::Relay { id, title } => {
                p.id = Some(id.clone());
                p.title = Some(title.clone());
                p.item_type = Some(ItemType::Relay);
                // Relays default to handoff unless an explicit status follows.
                if p.status.is_none() {
                    p.status = Some(TicketStatus::Handoff);
                }
            }
            ArchKind::Kind(k) => p.kind = Some(k.clone()),
            ArchKind::Status(s) => p.status = Some(TicketStatus::parse(s)),
            ArchKind::Assignee(a) => p.assignee = Some(a.clone()),
            ArchKind::Phase(ph) => p.phase = Some(ph.clone()),
            ArchKind::Parent(pa) => p.parent = Some(pa.clone()),
            ArchKind::HackSeverity(s) => p.severity = Some(s.clone()),
            ArchKind::Handoff(h) => p.handoff.push(h.clone()),
            ArchKind::Next(n) => p.next_steps.push(n.clone()),
            ArchKind::Cleanup(c) => p.cleanup.push(c.clone()),
            ArchKind::Verify(v) => p.verify.push(v.clone()),
            ArchKind::Gotcha(g) => p.gotchas.push(g.clone()),
            ArchKind::Assumes(a) => p.assumes.push(a.clone()),
            ArchKind::See(s) => p.see_also.push(s.clone()),
            ArchKind::DependsOn { target: dep, .. } => p.depends_on.push(dep.clone()),
            _ => {}
        }
    }
    Some(p)
}

/// Push items from `src` into `dst` skipping duplicates (set-union semantics).
/// Order: lex-first file's items first, then any new items from later files.
fn extend_dedup<T: Clone + PartialEq>(dst: &mut Vec<T>, src: &[T]) {
    for item in src {
        if !dst.contains(item) {
            dst.push(item.clone());
        }
    }
}

/// Record a scalar conflict: `value` from `loc` differs from a value
/// already chosen for `field`. Idempotent on (field, value, loc).
fn record_conflict(
    conflicts: &mut std::collections::BTreeMap<String, Vec<FieldConflict>>,
    field: &str,
    value: String,
    file: &Path,
    line: usize,
) {
    let entry = conflicts.entry(field.to_string()).or_default();
    let new = FieldConflict {
        value,
        path: file.to_path_buf(),
        line,
    };
    if !entry.contains(&new) {
        entry.push(new);
    }
}

/// Merge a scalar from `incoming` (from `loc`) into the winning `dst`
/// (already populated from the lex-first file when not None). On
/// disagreement, both values are recorded in `conflicts`.
fn merge_scalar<T: Clone + PartialEq + ToString>(
    dst: &mut Option<T>,
    incoming: &Option<T>,
    field: &str,
    file: &Path,
    line: usize,
    winner_loc: Option<(&Path, usize)>,
    conflicts: &mut std::collections::BTreeMap<String, Vec<FieldConflict>>,
) {
    let Some(inc) = incoming else { return };
    match dst {
        None => *dst = Some(inc.clone()),
        Some(existing) if existing == inc => {} // agreement, no-op
        Some(existing) => {
            // Record both the winner and this dissenting value, once each.
            if let Some((wf, wl)) = winner_loc {
                record_conflict(conflicts, field, existing.to_string(), wf, wl);
            }
            record_conflict(conflicts, field, inc.to_string(), file, line);
        }
    }
}

/// Build a Ticket or Relay from a group of annotations on the same ID.
/// `anns` may span multiple source files when the ID is declared in
/// more than one place; the per-file folds are CRDT-merged here.
fn build_item(anns: &[&ArchAnnotation]) -> Option<Ticket> {
    if anns.is_empty() {
        return None;
    }

    // Group by file (anns are pre-sorted by (file, line) so a simple
    // sequential scan keeps each file's annotations contiguous).
    let mut per_file: Vec<(PathBuf, Vec<&ArchAnnotation>)> = Vec::new();
    for ann in anns {
        match per_file.last_mut() {
            Some((f, group)) if f == &ann.file => group.push(ann),
            _ => per_file.push((ann.file.clone(), vec![ann])),
        }
    }
    let partials: Vec<PartialTicket> = per_file
        .into_iter()
        .filter_map(|(f, anns)| fold_file(f, &anns))
        .collect();
    if partials.is_empty() {
        return None;
    }

    // Lex-first partial wins for scalars and supplies the canonical
    // (file, line, target). Subsequent partials union vecs and surface
    // scalar disagreements via `conflicts`.
    let winner = &partials[0];
    let canonical_file = winner.file.clone();
    let canonical_line = winner.header_line;
    let target = winner
        .target
        .clone()
        .expect("fold_file always sets target when at least one ann present");

    let mut id = winner.id.clone();
    let mut title = winner.title.clone();
    let mut item_type = winner.item_type.clone();
    let mut kind = winner.kind.clone();
    let mut status = winner.status.clone();
    let mut assignee = winner.assignee.clone();
    let mut phase = winner.phase.clone();
    let mut parent = winner.parent.clone();
    let mut severity = winner.severity.clone();
    let mut handoff = winner.handoff.clone();
    let mut next_steps = winner.next_steps.clone();
    let mut cleanup = winner.cleanup.clone();
    let mut verify = winner.verify.clone();
    let mut gotchas = winner.gotchas.clone();
    let mut assumes = winner.assumes.clone();
    let mut see_also = winner.see_also.clone();
    let mut depends_on = winner.depends_on.clone();
    let mut conflicts: std::collections::BTreeMap<String, Vec<FieldConflict>> =
        std::collections::BTreeMap::new();

    let winner_loc: Option<(&Path, usize)> =
        Some((winner.file.as_path(), winner.header_line));

    for p in &partials[1..] {
        let f = p.file.as_path();
        let l = p.header_line;
        // Title and kind are scalars too — surface any disagreement.
        merge_scalar(&mut title, &p.title, "title", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut kind, &p.kind, "kind", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut status, &p.status, "status", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut assignee, &p.assignee, "assignee", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut phase, &p.phase, "phase", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut parent, &p.parent, "parent", f, l, winner_loc, &mut conflicts);
        merge_scalar(&mut severity, &p.severity, "severity", f, l, winner_loc, &mut conflicts);
        // id and item_type can't legitimately differ (same neighborhood
        // key, by construction) — fall back to the partial's value if
        // the winner had none, otherwise skip.
        if id.is_none() {
            id = p.id.clone();
        }
        if item_type.is_none() {
            item_type = p.item_type.clone();
        }
        // Vec fields union (set semantics).
        extend_dedup(&mut handoff, &p.handoff);
        extend_dedup(&mut next_steps, &p.next_steps);
        extend_dedup(&mut cleanup, &p.cleanup);
        extend_dedup(&mut verify, &p.verify);
        extend_dedup(&mut gotchas, &p.gotchas);
        extend_dedup(&mut assumes, &p.assumes);
        extend_dedup(&mut see_also, &p.see_also);
        extend_dedup(&mut depends_on, &p.depends_on);
    }

    // `files` is always populated, even with a single entry, so the
    // wire shape is uniform across single- and multi-file tickets.
    let files: Vec<TicketLocation> = partials
        .iter()
        .map(|p| TicketLocation {
            path: p.file.clone(),
            line: p.header_line,
        })
        .collect();

    let id = id?;
    let item_type = item_type?;
    let title = title.unwrap_or_default();
    let status = status.unwrap_or(TicketStatus::Open);
    let file = canonical_file;
    let line = canonical_line;

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
        files,
        conflicts,
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
    fn test_single_occurrence_files_field_has_one_entry() {
        // `files` is always-on: even a single-file ticket lists its
        // location, so consumers can iterate uniformly without an
        // empty-vs-singular branch. The conflicts map stays empty.
        let source = r#"//! @hack:ticket(F01, "single")
//! @hack:status(open)
"#;
        let anns = extract_from_source(source, Path::new("src/lib.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let t = board.get("F01").unwrap();
        assert_eq!(t.files.len(), 1);
        assert_eq!(t.files[0].path, Path::new("src/lib.rs"));
        assert_eq!(t.files[0].path, t.file);
        assert_eq!(t.files[0].line, t.line);
        assert!(t.conflicts.is_empty());
    }

    #[test]
    fn test_duplicate_id_surfaces_conflicts_and_unions_vec_fields() {
        // Two files declare R013-T2 with disagreeing scalars and
        // partly-overlapping vec fields. Expect:
        //  - lex-first file's scalar wins (`status: review`)
        //  - both values appear in `conflicts.status`
        //  - vec fields union (no duplicates), order = winner-first
        let src = r#"//! @hack:ticket(R013-T2, "P2: src view")
//! @hack:status(review)
//! @hack:assignee(agent:claude)
//! @hack:next("ship the signature change")
//! @hack:next("update callers")
"#;
        let test = r#"//! @hack:ticket(R013-T2, "P2: tests view")
//! @hack:status(open)
//! @hack:assignee(agent:claude)
//! @hack:next("update callers")
//! @hack:next("add fixture for chip_args")
"#;
        let mut anns = Vec::new();
        anns.extend(
            extract_from_source(src, Path::new("crates/banana/src/description.rs"))
                .unwrap(),
        );
        anns.extend(
            extract_from_source(test, Path::new("crates/banana/tests/chip_args.rs"))
                .unwrap(),
        );

        for _ in 0..8 {
            let board = TicketBoard::from_annotations(&anns);
            let t = board.get("R013-T2").expect("R013-T2 exists");
            // Lex-first wins for canonical singular fields.
            assert_eq!(t.file, Path::new("crates/banana/src/description.rs"));
            assert_eq!(t.status, TicketStatus::Review);
            assert_eq!(t.title, "P2: src view");

            // files lists both, sorted by (file, line).
            assert_eq!(t.files.len(), 2);
            assert_eq!(t.files[0].path, Path::new("crates/banana/src/description.rs"));
            assert_eq!(t.files[1].path, Path::new("crates/banana/tests/chip_args.rs"));

            // conflicts surfaces the disagreement on status + title.
            let status_conflicts = t.conflicts.get("status").expect("status conflict");
            assert_eq!(status_conflicts.len(), 2);
            assert!(status_conflicts.iter().any(|c| c.value == "review"));
            assert!(status_conflicts.iter().any(|c| c.value == "open"));
            let title_conflicts = t.conflicts.get("title").expect("title conflict");
            assert!(title_conflicts.iter().any(|c| c.value == "P2: src view"));
            assert!(title_conflicts.iter().any(|c| c.value == "P2: tests view"));
            // assignee agrees → no conflict entry.
            assert!(!t.conflicts.contains_key("assignee"));

            // Vec union: 3 distinct next_steps, winner-first ordering.
            assert_eq!(
                t.next_steps,
                vec![
                    "ship the signature change".to_string(),
                    "update callers".to_string(),
                    "add fixture for chip_args".to_string(),
                ]
            );
        }
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
    fn test_mixed_kind_compound_ids_all_infer_parent() {
        // R004: sub-tickets under one relay can use any of B/F/T as the
        // letter. Parent inference is letter-agnostic (split on '-'), so
        // R020-B1, R020-F2, R020-T3 should all resolve to parent R020 and
        // none should promote R020 to epic status.
        let source = r#"
//! @hack:relay(R020, "mixed-kind sub-tickets")
//! @hack:status(handoff)

pub mod m;

mod c1 {
    //! @hack:ticket(R020-B1, "bug")
    //! @hack:kind(bug)
    //! @hack:status(open)
}

mod c2 {
    //! @hack:ticket(R020-F2, "feature")
    //! @hack:kind(feature)
    //! @hack:status(open)
}

mod c3 {
    //! @hack:ticket(R020-T3, "task")
    //! @hack:kind(task)
    //! @hack:status(open)
}
"#;
        let annotations = extract_from_source(source, Path::new("mix.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        for id in ["R020-B1", "R020-F2", "R020-T3"] {
            let t = board.get(id).unwrap_or_else(|| panic!("{id} missing"));
            assert_eq!(t.parent.as_deref(), Some("R020"), "{id} parent");
            assert_eq!(t.item_type, ItemType::Ticket, "{id} type");
        }
        let r020 = board.get("R020").unwrap();
        assert!(!r020.is_epic, "mixed-kind sub-tickets don't promote to epic");
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

        // Trimmed playbook — no longer dumps Rule02/Rule03/Rule04/Col01 inline
        assert!(prompt.contains("**Rule01**"));
        assert!(!prompt.contains("**Rule03 —"));
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

    // ---------------------------------------------------------------
    // R005: container-aware pickup prompts. Each test here locks in
    // one specific piece of the hierarchy-context shape.
    // ---------------------------------------------------------------

    /// Picker fix: a relay with an InProgress sub-ticket points at that
    /// sub-ticket ("continue"), not the next fresh Open one. Real-world
    /// hit was R011 pointing at R011-T2 while R011-T1 was in-progress.
    #[test]
    fn test_picker_picks_in_progress_child_not_next_fresh() {
        let source = r#"
//! @hack:relay(R100, "container")
//! @hack:status(handoff)
//! @hack:ticket(R100-T1, "first — already in flight")
//! @hack:status(in-progress)
//! @hack:ticket(R100-T2, "second — open, ready")
//! @hack:status(open)

pub mod thing;
"#;
        let anns = extract_from_source(source, Path::new("c.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let ctx = board.build_prompt_context("R100");
        assert_eq!(
            ctx.live_children.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["R100-T1", "R100-T2"],
            "live_children sorted by ID include both — in-progress is not filtered out"
        );

        let prompt = board.get("R100").unwrap().to_prompt_with_ctx(&ctx);
        // Continue-verb for in-progress earliest child:
        assert!(
            prompt.contains("Continue with **R100-T1**"),
            "earliest live is in-progress → 'Continue with' verb, not 'claim':\n{}",
            prompt
        );
        assert!(
            prompt.contains("rs-hack board tickets --prompt R100-T1"),
            "should point at T1's pickup prompt for the actual work"
        );
        // And must NOT have silently skipped to T2:
        assert!(
            !prompt.contains("rs-hack board claim R100-T2"),
            "must not skip in-progress T1 and claim T2 instead:\n{}",
            prompt
        );
    }

    /// Sub-ticket pickup inherits the parent relay's gotchas and verify
    /// smoke. Real example: R011-T1 had no gotchas even though the Koda
    /// parser traps on the parent relay directly applied to it.
    #[test]
    fn test_subticket_inherits_parent_gotchas_and_verify() {
        let source = r#"
//! @hack:relay(R110, "parent with traps")
//! @hack:status(handoff)
//! @hack:gotcha("tuple-item lambda bodies need brackets; `&&` inside [ ] trips the parser")
//! @hack:verify("cargo check -p vivarium-banana-nodes --lib")
//! @hack:verify("cargo test -p banana --lib nodes::split::")
//! @hack:ticket(R110-T1, "child work")
//! @hack:status(handoff)
//! @hack:next("do the thing")

pub mod m;
"#;
        let anns = extract_from_source(source, Path::new("p.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);

        let ctx = board.build_prompt_context("R110-T1");
        assert_eq!(
            ctx.parent.map(|p| p.id.as_str()),
            Some("R110"),
            "parent resolved via compound-id inference"
        );
        let prompt = board.get("R110-T1").unwrap().to_prompt_with_ctx(&ctx);

        // Inherited gotchas appear with a clearly-marked header:
        assert!(
            prompt.contains("## ⚠ Gotchas inherited from R110"),
            "sub-ticket prompt surfaces parent gotchas:\n{}",
            prompt
        );
        assert!(prompt.contains("tuple-item lambda bodies need brackets"));

        // Inherited combined smoke at the tail of the verify section:
        assert!(
            prompt.contains("## Verification inherited from R110"),
            "sub-ticket prompt surfaces parent's verify smoke:\n{}",
            prompt
        );
        assert!(prompt.contains(
            "cargo check -p vivarium-banana-nodes --lib && cargo test -p banana --lib nodes::split::"
        ));
    }

    /// Relay-with-subtickets: the author's `next_steps` is forward-spawn
    /// noise, not the baton. Sub-tickets render above it, and the header
    /// relabels to "Follow-on spawns" so the pickup agent doesn't confuse
    /// them with the cycle.
    #[test]
    fn test_relay_with_children_hoists_subtickets_and_relabels_next() {
        let source = r#"
//! @hack:relay(R120, "multi-phase work")
//! @hack:status(handoff)
//! @hack:next("FOLLOW-ON: file new ticket for the framework rule")
//! @hack:next("FOLLOW-ON: retarget the design doc")
//! @hack:ticket(R120-T1, "first chunk")
//! @hack:status(open)

pub mod m;
"#;
        let anns = extract_from_source(source, Path::new("p.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let ctx = board.build_prompt_context("R120");
        let prompt = board.get("R120").unwrap().to_prompt_with_ctx(&ctx);

        let subticket_pos = prompt
            .find("## Sub-tickets in flight")
            .expect("sub-tickets section present");
        let followon_pos = prompt
            .find("## Follow-on spawns")
            .expect("next_steps relabeled to Follow-on spawns");
        assert!(
            subticket_pos < followon_pos,
            "sub-tickets must render above the spawn list — the cycle is the baton:\n{}",
            prompt
        );
        // Legacy label must NOT be used when we have children:
        assert!(
            !prompt.contains("## Next steps\n"),
            "with live children the next_steps section is relabeled"
        );
    }

    /// Epic prompts walk one level deeper: each child relay carries its
    /// own live-subticket count, and the header frames it as a "watering
    /// hole" agents can revisit.
    #[test]
    fn test_epic_prompt_shows_child_relays_with_grandchild_counts() {
        // R200 is an epic by virtue of child relays R201 / R202.
        // R201 has two open sub-tickets + one in-progress; R202 has none.
        let source = r#"
//! @hack:relay(R200, "big epic")
//! @hack:kind(epic)
//! @hack:status(handoff)

pub mod epic_root;

mod r201 {
    //! @hack:relay(R201, "first child relay")
    //! @hack:parent(R200)
    //! @hack:status(handoff)
    //! @hack:ticket(R201-T1, "sub a")
    //! @hack:status(in-progress)
    //! @hack:ticket(R201-T2, "sub b")
    //! @hack:status(open)
    //! @hack:ticket(R201-T3, "sub c")
    //! @hack:status(open)
}

mod r202 {
    //! @hack:relay(R202, "second child relay")
    //! @hack:parent(R200)
    //! @hack:status(open)
}
"#;
        let anns = extract_from_source(source, Path::new("e.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let r200 = board.get("R200").unwrap();
        assert!(r200.is_epic, "R200 qualifies as epic (has child relays)");

        let ctx = board.build_prompt_context("R200");
        assert_eq!(
            ctx.live_children.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["R201", "R202"],
            "epic's live_children are its live child relays"
        );
        let r201_counts = ctx.child_live_counts.get("R201").copied().unwrap_or_default();
        assert_eq!(r201_counts.open, 2);
        assert_eq!(r201_counts.in_flight, 1);
        let r202_counts = ctx.child_live_counts.get("R202").copied().unwrap_or_default();
        assert_eq!(r202_counts.total(), 0, "R202 has no sub-tickets of its own");

        let prompt = r200.to_prompt_with_ctx(&ctx);
        assert!(
            prompt.contains("## Child relays"),
            "epic uses 'Child relays' header, not 'Sub-tickets in flight':\n{}",
            prompt
        );
        assert!(
            prompt.contains("watering hole"),
            "epic framing explicitly names the re-entry pattern"
        );
        // Per-child count suffix rendered:
        assert!(
            prompt.contains("R201")
                && prompt.contains("2 open")
                && prompt.contains("1 in-flight"),
            "R201 line shows live-subticket counts:\n{}",
            prompt
        );
        // Epic starts with the earliest live child (R201, which is in
        // Handoff — picker verb = `move ... active`).
        assert!(
            prompt.contains("rs-hack board move R201 active"),
            "epic picker points at R201 (earliest live, handoff):\n{}",
            prompt
        );
    }

    /// Regression: a leaf ticket with no parent/children gets the classic
    /// flat prompt — no inherited sections, no container section.
    #[test]
    fn test_flat_ticket_unchanged() {
        let source = r#"
//! @hack:ticket(T999, "lonely")
//! @hack:status(open)

pub fn t() {}
"#;
        let anns = extract_from_source(source, Path::new("l.rs")).unwrap();
        let board = TicketBoard::from_annotations(&anns);
        let prompt = board.get("T999").unwrap().to_prompt();
        assert!(!prompt.contains("Gotchas inherited"));
        assert!(!prompt.contains("Verification inherited"));
        assert!(!prompt.contains("Sub-tickets in flight"));
        assert!(!prompt.contains("Child relays"));
        assert!(!prompt.contains("Follow-on spawns"));
    }
}
