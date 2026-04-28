//! @arch:layer(kg)
//! @arch:role(graph)
//!
//! Board-recompute layer over `Vec<WorkItem>`.
//!
//! `WorkItem` (in [`crate::rpc`]) is the wire DTO the daemon ships from
//! `arch.list_tickets` / `arch.list_relays` / `arch.get_ticket`. It is
//! deliberately minimal — just structural anchors plus the parsed
//! annotation payload(s).
//!
//! Presentation concerns that need a *board-wide* walk (epic inference,
//! cross-anchor scalar conflicts, sub-ticket bucket counts) don't belong
//! on the wire DTO — they would denormalize the per-item shape, and the
//! daemon would have to recompute them on every single-item lookup.
//! Instead, this module decorates a `WorkItem` collection in-process at
//! the consumer (CLI, hack-board UI) producing a [`Board`] of
//! [`BoardItem`]s with `is_epic` / `epic_status` / `conflicts` filled in.
//!
//! Inputs are intentionally shape-agnostic: `Board::from_work_items`
//! takes one Vec for relays and one for tickets — the same shape the
//! RPC results already arrive in. No graph access required.

use crate::anno::{TicketStatus, WorkItemType};
use crate::rpc::{WorkItem, WorkItemAnchor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Computed verdict for an epic. Returned alongside the relay's own
/// status — epics' raw `@yah:status(...)` is ignored, the verdict below
/// is what the board surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpicStatus {
    /// At least one child relay is still live (not in review/done) —
    /// or the epic was just declared and has no children yet (treat as
    /// planning, not closed).
    Active,
    /// All child relays have reached review/done.
    Closed,
}

impl EpicStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Closed => "closed",
        }
    }
}

/// One scalar value disagreement, attributed to a specific anchor. When
/// the same work-item id is declared in multiple files with disagreeing
/// scalar metadata (e.g. `status(open)` here vs. `status(review)` there),
/// each observed value surfaces as a `FieldConflict` so the disagreement
/// is loud rather than silent.
///
/// The lex-first anchor's value is the deterministic winner stored on
/// `WorkItem::anno`; the entries here include both the winner and every
/// dissenter so the UI can render the full picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldConflict {
    pub value: String,
    pub file: String,
    pub line: u32,
}

/// Live-subticket counts under a relay, bucketed by status. Drives the
/// epic two-level walk and the "X open · Y in-flight · Z handoff" suffix
/// the pickup prompt renders for each child relay.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    fn bump(&mut self, status: Option<TicketStatus>) {
        match status {
            Some(TicketStatus::Open) => self.open += 1,
            Some(TicketStatus::Claimed) | Some(TicketStatus::InProgress) => self.in_flight += 1,
            Some(TicketStatus::Handoff) => self.handoff += 1,
            // None defaults to Open (no explicit status = not started)
            None => self.open += 1,
            Some(TicketStatus::Review) | Some(TicketStatus::Done) => {}
        }
    }

    /// Render as a compact inline string, e.g. `2 open · 1 in-flight`.
    /// Empty buckets are dropped. Returns `""` when totally idle (no
    /// live sub-tickets) so the caller can elide the suffix.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
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

/// A `WorkItem` with the board-layer recomputed fields attached. Cheap
/// to build and re-build — the source-of-truth is always the `WorkItem`
/// payload. Every computed field can be derived from a `Vec<WorkItem>`
/// alone (no graph access required).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardItem {
    /// The wire payload as the daemon shipped it. Includes per-anchor
    /// payloads on `item.anchors[i].anno`.
    pub item: WorkItem,
    /// True when this relay acts as an epic — either explicitly declared
    /// with `@yah:kind(epic)` or inferred from having bare-R child relays
    /// via `@yah:parent(self.id)`. Always `false` for tickets and for
    /// relays with compound ids (e.g. `R007-T1`).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub is_epic: bool,
    /// Computed verdict for epics (`None` for non-epics).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epic_status: Option<EpicStatus>,
    /// Parent id, possibly *inferred* from a compound id prefix when no
    /// explicit `@yah:parent(...)` is set (e.g. `R007-T1` infers parent
    /// `R007`). The original explicit value, if present, takes precedence
    /// over inference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_parent: Option<String>,
    /// Per-field disagreements when the same id appears with different
    /// scalar values across anchors. Keys are field names (`status`,
    /// `assignee`, `phase`, `title`, `kind`, `severity`, `parent`); values
    /// list every observed value with its anchor location.
    ///
    /// Empty for the common (single-anchor) case. Length > 0 means the
    /// next agent has an authorial drift to resolve — either dedupe to
    /// one home or renumber one of the occurrences.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub conflicts: BTreeMap<String, Vec<FieldConflict>>,
}

/// Recomputed board view over a complete relay+ticket set. Built by
/// [`Board::from_work_items`] in O(n) over the input vecs.
///
/// `BoardItem`s are stored sorted by id so iteration order matches the
/// classic CLI output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Board {
    pub items: Vec<BoardItem>,
}

impl Board {
    /// Build a recompute view from the daemon's `arch.list_relays` +
    /// `arch.list_tickets` payloads. Both are taken by value — the
    /// caller usually has them as one-shot RPC results, no need to
    /// borrow.
    pub fn from_work_items(relays: Vec<WorkItem>, tickets: Vec<WorkItem>) -> Self {
        let mut items: Vec<BoardItem> = Vec::with_capacity(relays.len() + tickets.len());
        for w in relays {
            items.push(initial_board_item(w));
        }
        for w in tickets {
            items.push(initial_board_item(w));
        }
        items.sort_by(|a, b| a.item.id.cmp(&b.item.id));

        // First sweep: surface conflicts from per-anchor disagreements.
        for it in &mut items {
            it.conflicts = collect_conflicts(&it.item);
        }

        // Second sweep: epic inference + status derivation. Epic status
        // depends on every child's `status`, which lives on each item's
        // payload, so we have to look across the board.
        resolve_epics(&mut items);
        Board { items }
    }

    pub fn get(&self, id: &str) -> Option<&BoardItem> {
        self.items.iter().find(|i| i.item.id == id)
    }

    pub fn epics(&self) -> impl Iterator<Item = &BoardItem> {
        self.items.iter().filter(|i| i.is_epic)
    }

    pub fn relays(&self) -> impl Iterator<Item = &BoardItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.item.item_type, WorkItemType::Relay))
    }

    pub fn tickets(&self) -> impl Iterator<Item = &BoardItem> {
        self.items
            .iter()
            .filter(|i| matches!(i.item.item_type, WorkItemType::Ticket))
    }

    /// Direct children of `id` — items whose `effective_parent` is `id`.
    /// Includes both bare-R child relays (under an epic) and compound
    /// sub-tickets (under their relay).
    pub fn children_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a BoardItem> + 'a {
        self.items
            .iter()
            .filter(move |i| i.effective_parent.as_deref() == Some(id))
    }

    /// Bucketed live-child counts under `id`. Only counts non-terminal
    /// statuses; review/done are excluded. Useful for the epic two-level
    /// walk where each child relay's own sub-tickets need to be
    /// summarized in one line of the parent's pickup prompt.
    pub fn child_live_counts(&self, id: &str) -> ChildLiveCounts {
        let mut counts = ChildLiveCounts::default();
        for child in self.children_of(id) {
            counts.bump(child.item.anno.status);
        }
        counts
    }
}

fn initial_board_item(w: WorkItem) -> BoardItem {
    let effective_parent = w.anno.parent.clone().or_else(|| infer_parent(&w.id));
    BoardItem {
        item: w,
        is_epic: false,
        epic_status: None,
        effective_parent,
        conflicts: BTreeMap::new(),
    }
}

/// `R007-T1` → `Some("R007")`. Bare ids → `None`.
fn infer_parent(id: &str) -> Option<String> {
    id.split_once('-').map(|(p, _)| p.to_string())
}

/// Look across all anchors of a `WorkItem` and surface scalar fields
/// where the lex-first anchor (the canonical winner) disagrees with any
/// later anchor. Both the winner and the dissenter are recorded so the
/// UI can render the disagreement symmetrically.
fn collect_conflicts(w: &WorkItem) -> BTreeMap<String, Vec<FieldConflict>> {
    if w.anchors.len() < 2 {
        return BTreeMap::new();
    }
    let winner = &w.anchors[0];
    let mut conflicts: BTreeMap<String, Vec<FieldConflict>> = BTreeMap::new();
    for dissenter in &w.anchors[1..] {
        compare_scalar(
            "title",
            opt_str(&Some(winner.anno.title.clone())),
            opt_str(&Some(dissenter.anno.title.clone())),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "kind",
            opt_str(&winner.anno.kind),
            opt_str(&dissenter.anno.kind),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "status",
            winner.anno.status.map(|s| s.as_str().to_string()),
            dissenter.anno.status.map(|s| s.as_str().to_string()),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "assignee",
            opt_str(&winner.anno.assignee),
            opt_str(&dissenter.anno.assignee),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "phase",
            opt_str(&winner.anno.phase),
            opt_str(&dissenter.anno.phase),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "parent",
            opt_str(&winner.anno.parent),
            opt_str(&dissenter.anno.parent),
            winner,
            dissenter,
            &mut conflicts,
        );
        compare_scalar(
            "severity",
            opt_str(&winner.anno.severity),
            opt_str(&dissenter.anno.severity),
            winner,
            dissenter,
            &mut conflicts,
        );
    }
    conflicts
}

fn opt_str(o: &Option<String>) -> Option<String> {
    o.clone()
}

fn compare_scalar(
    field: &str,
    winner_val: Option<String>,
    dissenter_val: Option<String>,
    winner: &WorkItemAnchor,
    dissenter: &WorkItemAnchor,
    conflicts: &mut BTreeMap<String, Vec<FieldConflict>>,
) {
    // Only flag when both anchors actually carried a value AND the values differ.
    // A missing value at one anchor isn't a conflict — the other anchor's value wins.
    let (Some(w), Some(d)) = (winner_val, dissenter_val) else {
        return;
    };
    if w == d {
        return;
    }
    let entry = conflicts.entry(field.to_string()).or_default();
    let winner_conflict = FieldConflict {
        value: w,
        file: winner.file.clone(),
        line: winner.line,
    };
    if !entry.contains(&winner_conflict) {
        entry.push(winner_conflict);
    }
    let dissenter_conflict = FieldConflict {
        value: d,
        file: dissenter.file.clone(),
        line: dissenter.line,
    };
    if !entry.contains(&dissenter_conflict) {
        entry.push(dissenter_conflict);
    }
}

/// Walk the board, mark every relay that acts as an epic, and compute
/// its derived [`EpicStatus`] from its children's statuses.
///
/// Two ways to qualify as an epic:
/// 1. Explicit `@yah:kind(epic)` on the relay
/// 2. At least one *bare-R-id* relay declares `@yah:parent(self.id)`
///
/// Sub-tickets with compound ids (`R007-T1`) don't make their parent an
/// epic — they make it an ordinary relay-with-subtickets. Epics
/// coordinate *between* relays, not within one.
fn resolve_epics(items: &mut [BoardItem]) {
    // Collect bare-R child statuses by parent id. Compound-id children
    // are excluded — they don't promote their parent to epic.
    let mut epic_children: HashMap<String, Vec<Option<TicketStatus>>> = HashMap::new();
    for it in items.iter() {
        if !matches!(it.item.item_type, WorkItemType::Relay) {
            continue;
        }
        if it.item.id.contains('-') {
            continue;
        }
        if let Some(parent) = &it.effective_parent {
            epic_children
                .entry(parent.clone())
                .or_default()
                .push(it.item.anno.status);
        }
    }

    for it in items.iter_mut() {
        if !matches!(it.item.item_type, WorkItemType::Relay) {
            continue;
        }
        // Compound-id relays (R007-T1) are sub-tickets, never epics.
        if it.item.id.contains('-') {
            continue;
        }
        let explicit = it.item.anno.kind.as_deref() == Some("epic");
        let children = epic_children.get(&it.item.id);
        if !explicit && children.is_none() {
            continue;
        }
        it.is_epic = true;
        it.epic_status = Some(compute_epic_status(children.map(|v| v.as_slice())));
    }
}

fn compute_epic_status(children: Option<&[Option<TicketStatus>]>) -> EpicStatus {
    let Some(statuses) = children else {
        // Explicit epic with no children yet — treat as planning.
        return EpicStatus::Active;
    };
    if statuses.is_empty() {
        return EpicStatus::Active;
    }
    let any_live = statuses.iter().any(|s| match s {
        Some(TicketStatus::Review) | Some(TicketStatus::Done) => false,
        _ => true,
    });
    if any_live {
        EpicStatus::Active
    } else {
        EpicStatus::Closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anno::WorkItemAnno;
    use crate::ids::NodeId;
    use crate::kind::{CommonKind, Lang, NodeKind};

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

    fn anno_for(id: &str) -> WorkItemAnno {
        WorkItemAnno {
            id: id.to_string(),
            title: format!("title-of-{id}"),
            ..Default::default()
        }
    }

    fn item(id: &str, item_type: WorkItemType, anchors: Vec<WorkItemAnchor>) -> WorkItem {
        let _ = NodeKind::Common(CommonKind::Ticket); // reference to ensure import is alive
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
    fn epic_inferred_from_bare_r_child() {
        let r013 = item(
            "R013",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, anno_for("R013"))],
        );
        let r017 = {
            let mut a = anno_for("R017");
            a.parent = Some("R013".into());
            a.status = Some(TicketStatus::Open);
            item("R017", WorkItemType::Relay, vec![anchor("b.rs", 1, a)])
        };
        let board = Board::from_work_items(vec![r013, r017], vec![]);
        let r013 = board.get("R013").expect("R013 in board");
        assert!(r013.is_epic);
        assert_eq!(r013.epic_status, Some(EpicStatus::Active));
    }

    #[test]
    fn epic_explicit_with_no_children_is_active() {
        let mut a = anno_for("R013");
        a.kind = Some("epic".into());
        let r013 = item("R013", WorkItemType::Relay, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![r013], vec![]);
        let r013 = board.get("R013").unwrap();
        assert!(r013.is_epic);
        assert_eq!(r013.epic_status, Some(EpicStatus::Active));
    }

    #[test]
    fn epic_closes_when_all_children_review_or_done() {
        let r013 = item(
            "R013",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, anno_for("R013"))],
        );
        let mk_child = |id: &str, status: TicketStatus| {
            let mut a = anno_for(id);
            a.parent = Some("R013".into());
            a.status = Some(status);
            item(id, WorkItemType::Relay, vec![anchor("b.rs", 1, a)])
        };
        let board = Board::from_work_items(
            vec![
                r013,
                mk_child("R016", TicketStatus::Review),
                mk_child("R017", TicketStatus::Done),
            ],
            vec![],
        );
        let r013 = board.get("R013").unwrap();
        assert!(r013.is_epic);
        assert_eq!(r013.epic_status, Some(EpicStatus::Closed));
    }

    #[test]
    fn compound_subtickets_do_not_promote_relay_to_epic() {
        let r017 = item(
            "R017",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, anno_for("R017"))],
        );
        let mut sub_anno = anno_for("R017-T1");
        sub_anno.parent = Some("R017".into());
        sub_anno.status = Some(TicketStatus::Open);
        let r017_t1 = item(
            "R017-T1",
            WorkItemType::Ticket,
            vec![anchor("b.rs", 1, sub_anno)],
        );
        let board = Board::from_work_items(vec![r017], vec![r017_t1]);
        let r017 = board.get("R017").unwrap();
        assert!(!r017.is_epic, "compound sub-ticket parent shouldn't become epic");
    }

    #[test]
    fn parent_inferred_from_compound_id_when_not_explicit() {
        let mut a = anno_for("R017-T1");
        a.parent = None; // intentionally absent
        let item = item("R017-T1", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![item]);
        let it = board.get("R017-T1").unwrap();
        assert_eq!(it.effective_parent.as_deref(), Some("R017"));
    }

    #[test]
    fn explicit_parent_wins_over_inferred() {
        let mut a = anno_for("R017-T1");
        a.parent = Some("R013".into()); // overrides the R017 prefix
        let item = item("R017-T1", WorkItemType::Ticket, vec![anchor("a.rs", 1, a)]);
        let board = Board::from_work_items(vec![], vec![item]);
        let it = board.get("R017-T1").unwrap();
        assert_eq!(it.effective_parent.as_deref(), Some("R013"));
    }

    #[test]
    fn cross_anchor_status_disagreement_surfaces_as_conflict() {
        let mut a1 = anno_for("R042");
        a1.status = Some(TicketStatus::Open);
        let mut a2 = anno_for("R042");
        a2.status = Some(TicketStatus::Review);
        let r042 = item(
            "R042",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, a1), anchor("b.rs", 1, a2)],
        );
        let board = Board::from_work_items(vec![r042], vec![]);
        let r042 = board.get("R042").unwrap();
        let status_conflicts = r042.conflicts.get("status").expect("status conflict");
        assert_eq!(status_conflicts.len(), 2);
        let values: Vec<&str> = status_conflicts.iter().map(|c| c.value.as_str()).collect();
        assert!(values.contains(&"open"));
        assert!(values.contains(&"review"));
    }

    #[test]
    fn single_anchor_has_no_conflicts() {
        let r042 = item(
            "R042",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, anno_for("R042"))],
        );
        let board = Board::from_work_items(vec![r042], vec![]);
        let r042 = board.get("R042").unwrap();
        assert!(r042.conflicts.is_empty());
    }

    #[test]
    fn agreeing_anchors_have_no_conflicts() {
        let mut a1 = anno_for("R042");
        a1.status = Some(TicketStatus::Open);
        let mut a2 = anno_for("R042");
        a2.status = Some(TicketStatus::Open);
        let r042 = item(
            "R042",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, a1), anchor("b.rs", 1, a2)],
        );
        let board = Board::from_work_items(vec![r042], vec![]);
        let r042 = board.get("R042").unwrap();
        assert!(r042.conflicts.is_empty(), "agreement is not conflict");
    }

    #[test]
    fn child_live_counts_buckets_by_status() {
        let r017 = item(
            "R017",
            WorkItemType::Relay,
            vec![anchor("a.rs", 1, anno_for("R017"))],
        );
        let mk_sub = |id: &str, status: TicketStatus| {
            let mut a = anno_for(id);
            a.parent = Some("R017".into());
            a.status = Some(status);
            item(id, WorkItemType::Ticket, vec![anchor("b.rs", 1, a)])
        };
        let board = Board::from_work_items(
            vec![r017],
            vec![
                mk_sub("R017-T1", TicketStatus::Open),
                mk_sub("R017-T2", TicketStatus::InProgress),
                mk_sub("R017-T3", TicketStatus::Handoff),
                mk_sub("R017-T4", TicketStatus::Review), // excluded from live counts
            ],
        );
        let counts = board.child_live_counts("R017");
        assert_eq!(counts.open, 1);
        assert_eq!(counts.in_flight, 1);
        assert_eq!(counts.handoff, 1);
        assert_eq!(counts.total(), 3);
    }

    #[test]
    fn epic_status_serializes_to_kebab_lowercase() {
        let json = serde_json::to_string(&EpicStatus::Active).unwrap();
        assert_eq!(json, "\"active\"");
        let json = serde_json::to_string(&EpicStatus::Closed).unwrap();
        assert_eq!(json, "\"closed\"");
    }

    #[test]
    fn child_live_counts_describe_drops_empty_buckets() {
        let mut counts = ChildLiveCounts::default();
        counts.open = 2;
        assert_eq!(counts.describe(), "2 open");
        counts.handoff = 1;
        assert_eq!(counts.describe(), "2 open · 1 handoff");
    }
}
