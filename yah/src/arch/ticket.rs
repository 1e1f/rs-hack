//! @arch:layer(arch)
//! @arch:role(ticket)
//! @arch:see(architecture/hack-board.md)
//!
//! Ticket and Relay aggregation from `@yah:` annotations.
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

use crate::arch::annotation::{AnnotationTarget, ArchAnnotation, ArchKind};
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

    /// Annotation-form name (matches what `parse()` accepts and what
    /// `@yah:status(...)` writes in source).
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
/// Surfaced inside `Ticket::conflicts` when the same `@yah:` ID is
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

    /// Handoff message(s). Multiple `@yah:handoff(...)` annotations stack —
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
    /// From `@yah:gotcha(...)`. Rendered above the context block in
    /// the pickup prompt.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub gotchas: Vec<String>,

    /// Unverified assumptions that were baked into the handoff. From
    /// `@yah:assumes(...)`. Rendered as risks in the pickup prompt so
    /// the next agent knows to confirm or challenge them.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub assumes: Vec<String>,

    /// Depends on these IDs
    pub depends_on: Vec<String>,

    /// Links to architecture docs
    pub see_also: Vec<String>,

    /// `@yah:think(deep | standard | fast | budget=N)` — per-ticket
    /// thinking budget for the agent runtime (R028). Translated into the
    /// Claude SDK's `thinking` config when the prelude is assembled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<yah_kg::anno::ThinkBudget>,

    /// `@yah:engine(provider:model)` — per-ticket model selection (R028).
    /// Drives runner dispatch and surfaces in the agent pane's engine
    /// chip. `None` defers to the workspace default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<yah_kg::anno::EngineRef>,

    /// Convenience alias for `files[0].path` — the lex-first source
    /// location. Kept for back-compat with consumers that read a single
    /// path; new code should iterate `files`.
    pub file: PathBuf,

    /// Convenience alias for `files[0].line`.
    pub line: usize,

    /// AST target
    pub target: AnnotationTarget,

    /// Every source occurrence of this ticket's `@yah:ticket` /
    /// `@yah:relay` header, sorted by `(file, line)`. Always populated
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
    /// `@yah:kind(epic)` or inferred from having child relays via
    /// `@yah:parent(self.id)`. Computed by [`yah_kg::board::Board`] and
    /// projected onto the legacy `Ticket` shape in `from_annotations`.
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TicketBoard {
    pub tickets: Vec<Ticket>,
    /// Underlying KG board view backing the cross-anchor recompute and the
    /// shared renderer in [`yah_kg::prompt`]. Populated by
    /// [`TicketBoard::from_annotations`]; consumers use
    /// [`TicketBoard::to_prompt`] (preferred) or read it directly.
    ///
    /// `#[serde(default)]` so older snapshot fixtures without this field
    /// still deserialize — the prompt rendering then falls back to the
    /// per-ticket shim, which is best-effort for epic grandchild counts.
    #[serde(default)]
    pub kg_board: yah_kg::board::Board,
}

impl TicketBoard {
    /// Build a board from annotations.
    ///
    /// The CLI is the sole legacy consumer that still extracts tickets from
    /// `&[ArchAnnotation]`; the daemon ships them directly as
    /// `Vec<WorkItem>` over RPC. Both paths now converge on
    /// [`yah_kg::board::Board`] for the cross-anchor recompute (epic
    /// inference, scalar conflict surfacing). This method:
    ///
    /// 1. Buckets per `(file, syn-target)` "neighborhood", then per
    ///    `@yah:ticket`/`@yah:relay` header within it. Stacked headers in
    ///    one `//!` block produce distinct buckets — they used to collapse
    ///    when keyed solely by `AnnotationTarget`.
    /// 2. Folds each `(id, file)` bucket into a `PartialTicket` (per-file
    ///    parse, last-write-wins for scalars).
    /// 3. Translates each partial into a `WorkItemAnchor` with its parsed
    ///    `WorkItemAnno`, groups anchors per id, hands the result to
    ///    `Board::from_work_items` for the cross-anchor recompute.
    /// 4. Maps each `BoardItem` back to a legacy `Ticket`, pulling
    ///    `is_epic`/`epic_status`/`conflicts` straight from the board and
    ///    reading file/line/target from `anchors[0]` for back-compat. A
    ///    sidecar carries fields outside `WorkItemAnno`'s shape
    ///    (`depends_on`, `see_also`, `target`) and the cross-anchor vec
    ///    union (`handoff`/`next_steps`/…) the daemon does not perform on
    ///    the wire DTO.
    pub fn from_annotations(annotations: &[ArchAnnotation]) -> Self {
        use yah_kg::anno::WorkItemType as KgItemType;
        use yah_kg::board::Board;
        use yah_kg::ids::NodeId;
        use yah_kg::kind::Lang;
        use yah_kg::rpc::WorkItem;

        // Step 1: isolate "neighborhoods" — annotations that share a file
        // and syn target. Different targets in the same file (module block
        // vs. a struct) can't bleed into each other.
        let mut neighborhoods: HashMap<(PathBuf, String), Vec<&ArchAnnotation>> =
            HashMap::new();
        for ann in annotations {
            if is_hack_relevant(&ann.kind) {
                let key = (ann.file.clone(), ann.target.id());
                neighborhoods.entry(key).or_default().push(ann);
            }
        }

        // Step 2: within each neighborhood, walk in source order. Each
        // `@yah:ticket`/`@yah:relay` header opens a logical bucket keyed by
        // (id, file). Annotations that appear before any header are dropped
        // (matches legacy behavior — a ticket needs a defining header in
        // the bucket).
        let mut by_id_file: HashMap<(String, PathBuf), Vec<&ArchAnnotation>> =
            HashMap::new();
        for (_, mut anns) in neighborhoods {
            anns.sort_by_key(|a| a.line);
            let mut current: Option<String> = None;
            for ann in anns {
                if let ArchKind::Ticket { id, .. } | ArchKind::Relay { id, .. } = &ann.kind {
                    current = Some(id.clone());
                }
                if let Some(ref cid) = current {
                    by_id_file
                        .entry((cid.clone(), ann.file.clone()))
                        .or_default()
                        .push(ann);
                }
            }
        }

        // Step 3: per (id, file), fold the chunk into a `PartialTicket`.
        let mut partials_by_id: HashMap<String, Vec<PartialTicket>> = HashMap::new();
        for ((id, file), anns) in by_id_file {
            if let Some(p) = fold_file(file, &anns) {
                partials_by_id.entry(id).or_default().push(p);
            }
        }

        // Step 4: per id, sort partials by (file, header_line) — the
        // lex-first is the canonical winner. Translate partials into
        // anchors and bundle into `WorkItem`s for the Board recompute.
        let mut relays: Vec<WorkItem> = Vec::new();
        let mut tickets_wi: Vec<WorkItem> = Vec::new();
        let mut sidecar: HashMap<String, Sidecar> = HashMap::new();

        for (id, mut partials) in partials_by_id {
            partials.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then_with(|| a.header_line.cmp(&b.header_line))
            });

            let item_type = match partials.iter().find_map(|p| p.item_type.clone()) {
                Some(t) => t,
                None => continue,
            };

            let anchors: Vec<yah_kg::rpc::WorkItemAnchor> = partials
                .iter()
                .map(|p| partial_to_anchor(&id, p))
                .collect();

            let kg_item_type = match item_type {
                ItemType::Relay => KgItemType::Relay,
                ItemType::Ticket => KgItemType::Ticket,
            };

            // Board uses `WorkItem::anno` verbatim as the canonical
            // (lex-first) view; per-anchor payloads on `anchors[i].anno`
            // drive the cross-anchor conflict sweep.
            let canonical_anno = anchors[0].anno.clone();
            let work_item = WorkItem {
                id: id.clone(),
                node: NodeId::compute(Lang::Rust, &format!("ticket:{id}"), "<cli-adapter>"),
                item_type: kg_item_type,
                anno: canonical_anno,
                anchors,
                last_modified_ts: 0,
            };

            sidecar.insert(id.clone(), build_sidecar(&partials));

            match work_item.item_type {
                KgItemType::Relay => relays.push(work_item),
                KgItemType::Ticket => tickets_wi.push(work_item),
            }
        }

        // Step 5: hand off to the shared recompute layer.
        let board = Board::from_work_items(relays, tickets_wi);

        // Step 6: project `BoardItem`s back into legacy `Ticket`s. The
        // wire-DTO doesn't carry `depends_on`/`target`, and the daemon
        // doesn't union vec fields across anchors — both come from the
        // sidecar. (`see_also` now flows through `WorkItemAnno` so the
        // daemon's Reference section matches the CLI's.)
        let tickets: Vec<Ticket> = board
            .items
            .iter()
            .cloned()
            .map(|bi| board_item_to_ticket(bi, &sidecar))
            .collect();

        TicketBoard {
            tickets,
            kg_board: board,
        }
    }

    /// Render the pickup or review prompt for `id` against this board.
    /// Delegates to [`yah_kg::prompt::render`] — the same renderer the
    /// daemon's `arch.ticket_prompt` RPC calls — so the CLI and the Tauri
    /// client cannot drift on prompt shape.
    ///
    /// Returns `None` when the id isn't on the board; callers (CLI, tests)
    /// surface the miss explicitly instead of printing an empty prompt.
    pub fn to_prompt(
        &self,
        id: &str,
        mode: yah_kg::prompt::PromptMode,
    ) -> Option<String> {
        yah_kg::prompt::render(&self.kg_board, id, mode)
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

    /// CLI shim: delegates to [`yah_kg::prompt::render`] (the source-of-truth
    /// renderer the daemon's `arch.ticket_prompt` RPC also uses) by translating
    /// `self + ctx` into a one-off [`yah_kg::board::Board`]. Both paths produce
    /// byte-identical output for the same input — the daemon RPC and `yah board
    /// show --prompt` cannot drift.
    ///
    /// The contract of [`Ticket::to_prompt_with_ctx`] is unchanged. See
    /// [`yah_kg::prompt`] for the rendering logic.
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
        let board = build_one_off_kg_board(self, ctx);
        yah_kg::prompt::render(&board, &self.id, yah_kg::prompt::PromptMode::Pickup)
            .unwrap_or_default()
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
        "cargo", "yah", "yahh", "yahb", "yaha", "rs-hack", "rshack",
        "bun", "npm", "pnpm", "yarn", "deno",
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

/// Scan prose (typically a `@yah:handoff(...)` body) for `path:line` code
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
            | ArchKind::Think(_)
            | ArchKind::Engine(_)
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
    header_line: usize, // line of the first defining @yah:ticket/@yah:relay
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
    think: Option<yah_kg::anno::ThinkBudget>,
    engine: Option<yah_kg::anno::EngineRef>,
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
            ArchKind::Think(t) => p.think = Some(*t),
            ArchKind::Engine(e) => p.engine = Some(e.clone()),
            _ => {}
        }
    }
    Some(p)
}

/// Out-of-band per-id state the `WorkItemAnno` wire DTO doesn't carry.
/// Built once per id from its `PartialTicket`s before handing them to the
/// Board recompute, then read back when projecting `BoardItem`s into the
/// legacy `Ticket` shape.
///
/// Two flavors of fields here:
///
/// - **Out of WorkItemAnno's schema** — `target` (legacy
///   `AnnotationTarget`), `depends_on`, `see_also`. These don't exist on
///   the wire DTO at all, so the daemon can't ship them; the CLI keeps
///   them on the side.
/// - **Cross-anchor vec unions** — `handoff` / `next_steps` / `cleanup` /
///   `verify` / `gotchas` / `assumes`. The Board uses `WorkItem::anno`
///   verbatim (i.e. anchors[0]'s payload) for vec fields, but the legacy
///   CLI behavior unions them across files (winner-first dedup) so a
///   ticket declared twice doesn't lose `@yah:next(...)` lines from the
///   second declaration. The sidecar holds the unioned view.
struct Sidecar {
    target: AnnotationTarget,
    depends_on: Vec<String>,
    see_also: Vec<String>,
    handoff: Vec<String>,
    next_steps: Vec<String>,
    cleanup: Vec<String>,
    verify: Vec<String>,
    gotchas: Vec<String>,
    assumes: Vec<String>,
}

fn build_sidecar(partials: &[PartialTicket]) -> Sidecar {
    let target = partials[0]
        .target
        .clone()
        .expect("fold_file always sets target when at least one ann present");
    let mut sc = Sidecar {
        target,
        depends_on: Vec::new(),
        see_also: Vec::new(),
        handoff: Vec::new(),
        next_steps: Vec::new(),
        cleanup: Vec::new(),
        verify: Vec::new(),
        gotchas: Vec::new(),
        assumes: Vec::new(),
    };
    for p in partials {
        push_dedup(&mut sc.depends_on, &p.depends_on);
        push_dedup(&mut sc.see_also, &p.see_also);
        push_dedup(&mut sc.handoff, &p.handoff);
        push_dedup(&mut sc.next_steps, &p.next_steps);
        push_dedup(&mut sc.cleanup, &p.cleanup);
        push_dedup(&mut sc.verify, &p.verify);
        push_dedup(&mut sc.gotchas, &p.gotchas);
        push_dedup(&mut sc.assumes, &p.assumes);
    }
    sc
}

/// Set-union extend: append items from `src` to `dst` in source order,
/// skipping any already present.
fn push_dedup<T: Clone + PartialEq>(dst: &mut Vec<T>, src: &[T]) {
    for item in src {
        if !dst.contains(item) {
            dst.push(item.clone());
        }
    }
}

fn partial_to_anchor(id: &str, p: &PartialTicket) -> yah_kg::rpc::WorkItemAnchor {
    let anno = yah_kg::anno::WorkItemAnno {
        id: id.to_string(),
        title: p.title.clone().unwrap_or_default(),
        kind: p.kind.clone(),
        status: p.status.as_ref().map(to_kg_status),
        at: None,
        assignee: p.assignee.clone(),
        parent: p.parent.clone(),
        phase: p.phase.clone(),
        severity: p.severity.clone(),
        handoff: p.handoff.clone(),
        next_steps: p.next_steps.clone(),
        gotchas: p.gotchas.clone(),
        assumes: p.assumes.clone(),
        verify: p.verify.clone(),
        cleanup: p.cleanup.clone(),
        see_also: p.see_also.clone(),
        think: p.think,
        engine: p.engine.clone(),
        agent_policy: Vec::new(),
    };
    let file_str = p.file.to_string_lossy().into_owned();
    yah_kg::rpc::WorkItemAnchor {
        node: yah_kg::ids::NodeId::compute(
            yah_kg::kind::Lang::Rust,
            &format!("anchor:{id}:{file_str}:{}", p.header_line),
            "<cli-adapter>",
        ),
        file: file_str,
        line: p.header_line as u32,
        anno,
    }
}

fn to_kg_status(s: &TicketStatus) -> yah_kg::anno::TicketStatus {
    use yah_kg::anno::TicketStatus as Kg;
    match s {
        TicketStatus::Open => Kg::Open,
        TicketStatus::Claimed => Kg::Claimed,
        TicketStatus::InProgress => Kg::InProgress,
        TicketStatus::Handoff => Kg::Handoff,
        TicketStatus::Review => Kg::Review,
        TicketStatus::Done => Kg::Done,
    }
}

/// Translate a single legacy `Ticket` into the wire-shape `WorkItem` the
/// renderer in [`yah_kg::prompt`] consumes. Used by the
/// [`Ticket::to_prompt_with_ctx`] shim to feed its renderer a
/// `Board::from_work_items`-shaped view.
fn ticket_to_work_item(t: &Ticket) -> yah_kg::rpc::WorkItem {
    use yah_kg::anno::WorkItemType;
    use yah_kg::ids::NodeId;
    use yah_kg::kind::Lang;

    let anno = yah_kg::anno::WorkItemAnno {
        id: t.id.clone(),
        title: t.title.clone(),
        kind: t.kind.clone(),
        status: Some(to_kg_status(&t.status)),
        at: None,
        assignee: t.assignee.clone(),
        parent: t.parent.clone(),
        phase: t.phase.clone(),
        severity: t.severity.clone(),
        handoff: t.handoff.clone(),
        next_steps: t.next_steps.clone(),
        gotchas: t.gotchas.clone(),
        assumes: t.assumes.clone(),
        verify: t.verify.clone(),
        cleanup: t.cleanup.clone(),
        see_also: t.see_also.clone(),
        think: t.think,
        engine: t.engine.clone(),
        agent_policy: Vec::new(),
    };

    // Lex-first anchor first so `anchors[0]` matches `Ticket::file/line`.
    let mut anchors: Vec<yah_kg::rpc::WorkItemAnchor> = t
        .files
        .iter()
        .map(|loc| {
            let file_str = loc.path.to_string_lossy().into_owned();
            yah_kg::rpc::WorkItemAnchor {
                node: NodeId::compute(
                    Lang::Rust,
                    &format!("anchor:{}:{}:{}", t.id, file_str, loc.line),
                    "<prompt-shim>",
                ),
                file: file_str,
                line: loc.line as u32,
                anno: anno.clone(),
            }
        })
        .collect();
    if anchors.is_empty() {
        let file_str = t.file.to_string_lossy().into_owned();
        anchors.push(yah_kg::rpc::WorkItemAnchor {
            node: NodeId::compute(
                Lang::Rust,
                &format!("anchor:{}:{}:{}", t.id, file_str, t.line),
                "<prompt-shim>",
            ),
            file: file_str,
            line: t.line as u32,
            anno: anno.clone(),
        });
    }

    let item_type = match t.item_type {
        ItemType::Relay => WorkItemType::Relay,
        ItemType::Ticket => WorkItemType::Ticket,
    };

    yah_kg::rpc::WorkItem {
        id: t.id.clone(),
        node: NodeId::compute(Lang::Rust, &format!("ticket:{}", t.id), "<prompt-shim>"),
        item_type,
        anno,
        anchors,
        last_modified_ts: 0,
    }
}

/// Build a one-off [`yah_kg::board::Board`] from `t` and its
/// `PromptContext` neighbours so [`yah_kg::prompt::render`] can render
/// without needing a full workspace scan.
fn build_one_off_kg_board(t: &Ticket, ctx: &PromptContext) -> yah_kg::board::Board {
    use yah_kg::anno::WorkItemType;

    let mut relays: Vec<yah_kg::rpc::WorkItem> = Vec::new();
    let mut tickets: Vec<yah_kg::rpc::WorkItem> = Vec::new();
    let mut bucket = |ticket: &Ticket| {
        let wi = ticket_to_work_item(ticket);
        match wi.item_type {
            WorkItemType::Relay => relays.push(wi),
            WorkItemType::Ticket => tickets.push(wi),
        }
    };

    bucket(t);
    if let Some(parent) = ctx.parent {
        if parent.id != t.id {
            bucket(parent);
        }
    }
    for child in &ctx.live_children {
        if child.id != t.id {
            bucket(child);
        }
    }

    yah_kg::board::Board::from_work_items(relays, tickets)
}

fn from_kg_status(s: yah_kg::anno::TicketStatus) -> TicketStatus {
    use yah_kg::anno::TicketStatus as Kg;
    match s {
        Kg::Open => TicketStatus::Open,
        Kg::Claimed => TicketStatus::Claimed,
        Kg::InProgress => TicketStatus::InProgress,
        Kg::Handoff => TicketStatus::Handoff,
        Kg::Review => TicketStatus::Review,
        Kg::Done => TicketStatus::Done,
    }
}

fn board_item_to_ticket(
    bi: yah_kg::board::BoardItem,
    sidecar: &HashMap<String, Sidecar>,
) -> Ticket {
    use yah_kg::anno::WorkItemType as KgItemType;

    let id = bi.item.id.clone();
    let sc = sidecar
        .get(&id)
        .expect("sidecar populated for every id that produced a WorkItem");
    let anchors = &bi.item.anchors;
    let canonical = &anchors[0];

    let item_type = match bi.item.item_type {
        KgItemType::Relay => ItemType::Relay,
        KgItemType::Ticket => ItemType::Ticket,
    };

    let status = bi
        .item
        .anno
        .status
        .map(from_kg_status)
        .unwrap_or(TicketStatus::Open);

    // Infer kind from ID prefix when no explicit @yah:kind on tickets.
    // Relays carry "relay"-shaped semantics natively and don't get a
    // kind unless the author wrote one.
    let mut kind = bi.item.anno.kind.clone();
    if kind.is_none() && item_type == ItemType::Ticket {
        kind = match id.chars().next() {
            Some('B') | Some('b') => Some("bug".to_string()),
            Some('F') | Some('f') => Some("feature".to_string()),
            _ => None,
        };
    }

    let files: Vec<TicketLocation> = anchors
        .iter()
        .map(|a| TicketLocation {
            path: PathBuf::from(&a.file),
            line: a.line as usize,
        })
        .collect();

    let conflicts: std::collections::BTreeMap<String, Vec<FieldConflict>> = bi
        .conflicts
        .iter()
        .map(|(field, vals)| {
            (
                field.clone(),
                vals.iter()
                    .map(|fc| FieldConflict {
                        value: fc.value.clone(),
                        path: PathBuf::from(&fc.file),
                        line: fc.line as usize,
                    })
                    .collect(),
            )
        })
        .collect();

    Ticket {
        id,
        title: bi.item.anno.title.clone(),
        item_type,
        kind,
        status,
        assignee: bi.item.anno.assignee.clone(),
        phase: bi.item.anno.phase.clone(),
        parent: bi.effective_parent.clone(),
        severity: bi.item.anno.severity.clone(),
        handoff: sc.handoff.clone(),
        next_steps: sc.next_steps.clone(),
        cleanup: sc.cleanup.clone(),
        verify: sc.verify.clone(),
        gotchas: sc.gotchas.clone(),
        assumes: sc.assumes.clone(),
        depends_on: sc.depends_on.clone(),
        see_also: sc.see_also.clone(),
        think: bi.item.anno.think,
        engine: bi.item.anno.engine.clone(),
        file: PathBuf::from(&canonical.file),
        line: canonical.line as usize,
        target: sc.target.clone(),
        files,
        conflicts,
        is_epic: bi.is_epic,
        epic_status: bi.epic_status.map(|es| es.as_str().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::extract::extract_from_source;
    use std::path::Path;

    #[test]
    fn test_ticket_extraction() {
        let source = r#"
//! @yah:ticket(F01, "Implement voice allocation")
//! @yah:status(in-progress)
//! @yah:assignee(agent:claude)
//! @yah:phase(P2)
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
    fn test_think_and_engine_annotations_round_trip() {
        let source = r#"
//! @yah:ticket(R028-T9, "Agent runtime smoke")
//! @yah:status(open)
//! @yah:think(deep)
//! @yah:engine(claude:opus-4-7)

pub mod runtime;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        let ticket = &board.tickets[0];
        assert_eq!(ticket.id, "R028-T9");
        assert_eq!(ticket.think, Some(yah_kg::anno::ThinkBudget::Deep));
        let engine = ticket.engine.as_ref().expect("engine present");
        assert_eq!(engine.provider, "claude");
        assert_eq!(engine.model.as_deref(), Some("opus-4-7"));
        // JSON surface — verify scan reports these via `board tickets --format json`.
        let json = serde_json::to_value(ticket).unwrap();
        assert_eq!(json["think"]["mode"], "deep");
        assert_eq!(json["engine"]["provider"], "claude");
        assert_eq!(json["engine"]["model"], "opus-4-7");
    }

    #[test]
    fn test_think_budget_with_explicit_token_count() {
        let source = r#"
//! @yah:ticket(R028-T9, "Long-context tuning")
//! @yah:think(budget=8192)

pub mod tuning;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let ticket = &board.tickets[0];
        assert_eq!(
            ticket.think,
            Some(yah_kg::anno::ThinkBudget::Budget { tokens: 8192 })
        );
    }

    #[test]
    fn test_stacked_tickets_in_one_module_block() {
        // Regression: several @yah:ticket / @yah:relay headers in the same
        // //! block must produce distinct tickets — each one owns the
        // annotations that follow it, bounded by the next header.
        let source = r#"
//! @yah:relay(R010, "Parent relay")
//! @yah:status(handoff)
//! @yah:handoff("R010 handoff text")
//! @yah:ticket(R010-T1, "First sub-ticket")
//! @yah:status(in-progress)
//! @yah:next("T1 next step")
//! @yah:ticket(R010-T2, "Second sub-ticket")
//! @yah:status(open)
//! @yah:next("T2 next step")

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
        let source = r#"//! @yah:ticket(F01, "single")
//! @yah:status(open)
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
        let src = r#"//! @yah:ticket(R013-T2, "P2: src view")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:next("ship the signature change")
//! @yah:next("update callers")
"#;
        let test = r#"//! @yah:ticket(R013-T2, "P2: tests view")
//! @yah:status(open)
//! @yah:assignee(agent:claude)
//! @yah:next("update callers")
//! @yah:next("add fixture for chip_args")
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
//! @yah:ticket(B01, "Panic on zero-length buffer")
//! @yah:status(open)
//! @yah:severity(high)

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
        // @yah:bug(...) still works as an alias for @yah:ticket(...)
        let source = r#"
//! @yah:bug(B02, "Old-style bug annotation")
//! @yah:status(open)

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
//! @yah:relay(R001, "ProcessBlock Unification Phase 4")
//! @yah:assignee(agent:claude)
//! @yah:phase(P4)
//! @yah:handoff("AudioProcessor -> ProcessBlock across ~80 files.")
//! @yah:next("Simplify add_control_node_unified -> add_node_named")
//! @yah:next("Inline/remove intermediary functions")
//! @yah:cleanup("CvRbjFilter is dead code")
//! @yah:verify("cargo test -p vivarium")
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
            prompt.contains("yah board move R001 active"),
            "expected pickup verb in prompt:\n{}",
            prompt
        );

        let md = board.to_markdown();
        assert!(md.contains("[R] R001"));
    }

    #[test]
    fn test_epic_inferred_from_children() {
        // R001 has no explicit kind=epic, but R002 declares @yah:parent(R001).
        // That parent-pointer alone should mark R001 as an epic.
        let source = r#"
//! @yah:relay(R001, "Big epic")
//! @yah:status(handoff)

pub mod epic_root;

mod child {
    //! @yah:relay(R002, "child thread")
    //! @yah:parent(R001)
    //! @yah:status(in-progress)
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
//! @yah:relay(R010, "Planning epic")
//! @yah:kind(epic)
//! @yah:status(open)

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
//! @yah:relay(R020, "Parent")
//! @yah:kind(epic)

pub mod parent;

mod child_a {
    //! @yah:relay(R021, "a")
    //! @yah:parent(R020)
    //! @yah:status(review)
}

mod child_b {
    //! @yah:relay(R022, "b")
    //! @yah:parent(R020)
    //! @yah:status(done)
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
//! @yah:relay(R030, "Parent")

pub mod parent;

mod a {
    //! @yah:relay(R031, "a")
    //! @yah:parent(R030)
    //! @yah:status(review)
}

mod b {
    //! @yah:relay(R032, "b")
    //! @yah:parent(R030)
    //! @yah:status(in-progress)
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
        //   - infer parent = "R007" from its ID even without @yah:parent
        //   - NOT promote R007 to epic status (only bare-R children do that)
        let source = r#"
//! @yah:relay(R007, "replay coverage")
//! @yah:status(handoff)

pub mod relay_root;

mod child {
    //! @yah:ticket(R007-T1, "cluster A")
    //! @yah:status(open)
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
//! @yah:relay(R020, "mixed-kind sub-tickets")
//! @yah:status(handoff)

pub mod m;

mod c1 {
    //! @yah:ticket(R020-B1, "bug")
    //! @yah:kind(bug)
    //! @yah:status(open)
}

mod c2 {
    //! @yah:ticket(R020-F2, "feature")
    //! @yah:kind(feature)
    //! @yah:status(open)
}

mod c3 {
    //! @yah:ticket(R020-T3, "task")
    //! @yah:kind(task)
    //! @yah:status(open)
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
//! @yah:ticket(T01, "t")
//! @yah:status(handoff)
//! @yah:verify("cargo check -p foo")
//! @yah:verify("cargo test --lib  # expected: 42/42")
//! @yah:verify("cargo test -p vivarium-banana-nodes --lib is clean (no new errors)")
//! @yah:verify("Manual: launch example and listen for modulation")

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
        // Single @yah:handoff(...) → paragraph.
        let single = r#"
//! @yah:ticket(T02, "single")
//! @yah:status(handoff)
//! @yah:handoff("One long paragraph covering everything that was done in this session.")

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

        // Multiple @yah:handoff(...) → bullets.
        let multi = r#"
//! @yah:ticket(T03, "multi")
//! @yah:status(handoff)
//! @yah:handoff("Fixed resolver at ctx.rs:2263.")
//! @yah:handoff("Added symbol table for note_v2 in koda_relay_node.rs:583.")
//! @yah:handoff("Updated MIX_UNIFICATION.md §Phase 9b.")

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
//! @yah:ticket(T42, "handle the thing")
//! @yah:status(handoff)
//! @yah:handoff("Scaffolded the thing in src/thing.rs:10. Left unsafe blocks TBD.")
//! @yah:gotcha("banana_nodes --lib has pre-existing compile errors unrelated to this ticket")
//! @yah:gotcha("cargo test -p vivarium-banana-nodes --lib fails for unrelated IRFunction churn — don't fix")
//! @yah:assumes("Koda IR-lowering flattens @children arm tuples into IRValue::List")
//! @yah:verify("cargo check -p vivarium-banana-nodes --lib")
//! @yah:verify("cargo test -p banana --lib nodes::split::")

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
        assert!(prompt.contains("yah board rules"));

        // File:line extraction from handoff prose
        assert!(prompt.contains("Locations referenced above"));
        assert!(prompt.contains("src/thing.rs:10"));
    }

    #[test]
    fn test_parent_relay() {
        let source = r#"
//! @yah:relay(R005, "CV Port Bridge")
//! @yah:parent(R001)
//! @yah:assignee(agent:claude)

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
//! @yah:ticket(T01, "Add cv_to_hz to RbjBiquadNode")
//! @yah:parent(R005)
//! @yah:phase(P1)
//! @yah:status(open)

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
//! @yah:ticket(T01, "Do the thing")
//! @yah:status(done)

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
//! @yah:ticket(T03, "Add polyphony support")
//! @yah:status(claimed)
//! @yah:assignee(agent:claude)

pub struct Synth;
"#;
        let annotations = extract_from_source(source, Path::new("synth.rs")).unwrap();

        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        assert_eq!(board.tickets[0].id, "T03");

        let graph = crate::arch::graph::ArchGraph::from_annotations(annotations);
        let ctx = crate::arch::query::get_file_context(&graph, "synth");
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
//! @yah:relay(R100, "container")
//! @yah:status(handoff)
//! @yah:ticket(R100-T1, "first — already in flight")
//! @yah:status(in-progress)
//! @yah:ticket(R100-T2, "second — open, ready")
//! @yah:status(open)

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
            prompt.contains("yah board tickets --prompt R100-T1"),
            "should point at T1's pickup prompt for the actual work"
        );
        // And must NOT have silently skipped to T2:
        assert!(
            !prompt.contains("yah board claim R100-T2"),
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
//! @yah:relay(R110, "parent with traps")
//! @yah:status(handoff)
//! @yah:gotcha("tuple-item lambda bodies need brackets; `&&` inside [ ] trips the parser")
//! @yah:verify("cargo check -p vivarium-banana-nodes --lib")
//! @yah:verify("cargo test -p banana --lib nodes::split::")
//! @yah:ticket(R110-T1, "child work")
//! @yah:status(handoff)
//! @yah:next("do the thing")

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
//! @yah:relay(R120, "multi-phase work")
//! @yah:status(handoff)
//! @yah:next("FOLLOW-ON: file new ticket for the framework rule")
//! @yah:next("FOLLOW-ON: retarget the design doc")
//! @yah:ticket(R120-T1, "first chunk")
//! @yah:status(open)

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
//! @yah:relay(R200, "big epic")
//! @yah:kind(epic)
//! @yah:status(handoff)

pub mod epic_root;

mod r201 {
    //! @yah:relay(R201, "first child relay")
    //! @yah:parent(R200)
    //! @yah:status(handoff)
    //! @yah:ticket(R201-T1, "sub a")
    //! @yah:status(in-progress)
    //! @yah:ticket(R201-T2, "sub b")
    //! @yah:status(open)
    //! @yah:ticket(R201-T3, "sub c")
    //! @yah:status(open)
}

mod r202 {
    //! @yah:relay(R202, "second child relay")
    //! @yah:parent(R200)
    //! @yah:status(open)
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

        // Render via the canonical TicketBoard::to_prompt path — the per-ticket
        // shim (`Ticket::to_prompt_with_ctx`) is best-effort and can't see
        // grandchildren, which the epic prompt needs for the count suffix.
        let prompt = board
            .to_prompt("R200", yah_kg::prompt::PromptMode::Pickup)
            .expect("R200 is on the board");
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
            prompt.contains("yah board move R201 active"),
            "epic picker points at R201 (earliest live, handoff):\n{}",
            prompt
        );
    }

    /// Regression: a leaf ticket with no parent/children gets the classic
    /// flat prompt — no inherited sections, no container section.
    #[test]
    fn test_flat_ticket_unchanged() {
        let source = r#"
//! @yah:ticket(T999, "lonely")
//! @yah:status(open)

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
