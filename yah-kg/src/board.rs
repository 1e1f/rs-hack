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
//!
//! @yah:ticket(R028-F5, "Skill resolver: column/tag -> skill set, baked into prelude")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R028)
//! @yah:next("Extend SDLC rule engine to map (column, tags) -> skill list")
//! @yah:next("Default rules: review -> /review + /security-review; tag(security) -> /security-review")
//! @yah:next("Resolved skills inject into Prelude (R028-T2)")
//! @yah:verify("Open a ticket in Review column; prelude includes /review skill registration")
//! @arch:see(architecture/yah-agent-runtime.md)

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

/// Just the bits of a `WorkItem` the relay-derive pass needs. Lets us
/// hold relays + tickets in one lookup without fighting the borrow
/// checker over `&mut [WorkItem]` re-entry on the relay slice.
struct DeriveSnap {
    item_type: WorkItemType,
    status: Option<TicketStatus>,
    last_modified_ts: u64,
}

fn derive_status(
    id: &str,
    by_id: &HashMap<String, DeriveSnap>,
    children_by_parent: &HashMap<String, Vec<String>>,
    memo: &mut HashMap<String, TicketStatus>,
    visiting: &mut std::collections::HashSet<String>,
) -> TicketStatus {
    if let Some(s) = memo.get(id) {
        return *s;
    }
    let Some(snap) = by_id.get(id) else {
        return TicketStatus::Open;
    };
    let own = snap.status.unwrap_or(TicketStatus::Open);
    if visiting.contains(id) {
        return own;
    }
    if !matches!(snap.item_type, WorkItemType::Relay) {
        memo.insert(id.to_string(), own);
        return own;
    }
    let Some(children) = children_by_parent.get(id) else {
        memo.insert(id.to_string(), own);
        return own;
    };
    if children.is_empty() {
        memo.insert(id.to_string(), own);
        return own;
    }
    visiting.insert(id.to_string());
    let mut seen_active = false;
    let mut seen_handoff = false;
    let mut seen_open = false;
    let mut seen_review = false;
    for c in children {
        let s = derive_status(c, by_id, children_by_parent, memo, visiting);
        match s {
            TicketStatus::Claimed | TicketStatus::InProgress => seen_active = true,
            TicketStatus::Handoff => seen_handoff = true,
            TicketStatus::Open => seen_open = true,
            TicketStatus::Review | TicketStatus::Done => seen_review = true,
        }
    }
    visiting.remove(id);
    let derived = if seen_active {
        TicketStatus::InProgress
    } else if seen_handoff {
        TicketStatus::Handoff
    } else if seen_review && seen_open {
        TicketStatus::Handoff
    } else if seen_open {
        TicketStatus::Open
    } else {
        TicketStatus::Review
    };
    memo.insert(id.to_string(), derived);
    derived
}

fn derive_ts(
    id: &str,
    by_id: &HashMap<String, DeriveSnap>,
    children_by_parent: &HashMap<String, Vec<String>>,
    memo: &mut HashMap<String, u64>,
    visiting: &mut std::collections::HashSet<String>,
) -> u64 {
    if let Some(t) = memo.get(id) {
        return *t;
    }
    let Some(snap) = by_id.get(id) else {
        return 0;
    };
    let own = snap.last_modified_ts;
    if visiting.contains(id) {
        return own;
    }
    if !matches!(snap.item_type, WorkItemType::Relay) {
        memo.insert(id.to_string(), own);
        return own;
    }
    let Some(children) = children_by_parent.get(id) else {
        memo.insert(id.to_string(), own);
        return own;
    };
    if children.is_empty() {
        memo.insert(id.to_string(), own);
        return own;
    }
    visiting.insert(id.to_string());
    let mut max = own;
    for c in children {
        let t = derive_ts(c, by_id, children_by_parent, memo, visiting);
        if t > max {
            max = t;
        }
    }
    visiting.remove(id);
    memo.insert(id.to_string(), max);
    max
}

/// Derive an effective `status` for every relay that has children, then
/// roll up `last_modified_ts` as the max across the relay and every
/// descendant. Mirrors `yah-ui/src/lib/relay-status.ts:withDerivedRelayFields`
/// so the daemon's `arch.list_relays` ships the same view the desktop
/// client computes locally — once children exist the source-authored
/// `@yah:status(...)` becomes display-only.
///
/// Precedence (matches the TS `bucketOf` mapping):
/// * any child in `claimed` | `in-progress` → `in-progress`
/// * else any child in `handoff`              → `handoff`
/// * else (any in `review`/`done` AND any `open`) → `handoff`
///   (partial-completion checkpoint; "not started" would be misleading)
/// * else any child in `open`                 → `open`
/// * else (every child in `review` | `done`)  → `review`
///
/// Childless relays are left untouched. Tickets are read-only inputs —
/// they can't have their own children, but they ARE descendants of the
/// relays we're deriving for.
pub fn apply_derived_relay_fields(relays: &mut [WorkItem], tickets: &[WorkItem]) {
    if relays.is_empty() {
        return;
    }

    let mut by_id: HashMap<String, DeriveSnap> =
        HashMap::with_capacity(relays.len() + tickets.len());
    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for w in relays.iter().chain(tickets.iter()) {
        by_id.insert(
            w.id.clone(),
            DeriveSnap {
                item_type: w.item_type,
                status: w.anno.status,
                last_modified_ts: w.last_modified_ts,
            },
        );
        if let Some(p) = &w.anno.parent {
            children_by_parent
                .entry(p.clone())
                .or_default()
                .push(w.id.clone());
        }
    }

    let mut status_memo: HashMap<String, TicketStatus> = HashMap::new();
    let mut ts_memo: HashMap<String, u64> = HashMap::new();
    let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();

    for relay in relays.iter_mut() {
        let Some(children) = children_by_parent.get(&relay.id) else {
            continue;
        };
        if children.is_empty() {
            continue;
        }
        let s = derive_status(
            &relay.id,
            &by_id,
            &children_by_parent,
            &mut status_memo,
            &mut visiting,
        );
        relay.anno.status = Some(s);
        let t = derive_ts(
            &relay.id,
            &by_id,
            &children_by_parent,
            &mut ts_memo,
            &mut visiting,
        );
        relay.last_modified_ts = t;
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

    fn relay_with(id: &str, status: Option<TicketStatus>, parent: Option<&str>) -> WorkItem {
        let mut a = anno_for(id);
        a.status = status;
        a.parent = parent.map(String::from);
        item(id, WorkItemType::Relay, vec![anchor("a.rs", 1, a)])
    }

    fn ticket_with(id: &str, status: Option<TicketStatus>, parent: Option<&str>) -> WorkItem {
        let mut a = anno_for(id);
        a.status = status;
        a.parent = parent.map(String::from);
        item(id, WorkItemType::Ticket, vec![anchor("b.rs", 1, a)])
    }

    #[test]
    fn derive_relay_status_picks_active_when_any_child_in_progress() {
        let mut relays = vec![relay_with("R017", Some(TicketStatus::Open), None)];
        let tickets = vec![
            ticket_with("R017-T1", Some(TicketStatus::InProgress), Some("R017")),
            ticket_with("R017-T2", Some(TicketStatus::Open), Some("R017")),
        ];
        apply_derived_relay_fields(&mut relays, &tickets);
        assert_eq!(relays[0].anno.status, Some(TicketStatus::InProgress));
    }

    #[test]
    fn derive_relay_status_partial_completion_reads_as_handoff() {
        let mut relays = vec![relay_with("R017", Some(TicketStatus::Open), None)];
        let tickets = vec![
            ticket_with("R017-T1", Some(TicketStatus::Review), Some("R017")),
            ticket_with("R017-T2", Some(TicketStatus::Open), Some("R017")),
        ];
        apply_derived_relay_fields(&mut relays, &tickets);
        assert_eq!(relays[0].anno.status, Some(TicketStatus::Handoff));
    }

    #[test]
    fn derive_relay_status_all_review_or_done_is_review() {
        let mut relays = vec![relay_with("R017", Some(TicketStatus::Open), None)];
        let tickets = vec![
            ticket_with("R017-T1", Some(TicketStatus::Review), Some("R017")),
            ticket_with("R017-T2", Some(TicketStatus::Done), Some("R017")),
        ];
        apply_derived_relay_fields(&mut relays, &tickets);
        assert_eq!(relays[0].anno.status, Some(TicketStatus::Review));
    }

    #[test]
    fn derive_relay_status_skips_relay_without_children() {
        let mut relays = vec![relay_with("R017", Some(TicketStatus::Handoff), None)];
        apply_derived_relay_fields(&mut relays, &[]);
        // Source status preserved when no children exist.
        assert_eq!(relays[0].anno.status, Some(TicketStatus::Handoff));
    }

    #[test]
    fn derive_relay_status_recurses_through_child_relays() {
        // Epic R013 → child relay R017 → ticket R017-T1(in-progress).
        // R017 alone derives in-progress; R013 picks it up via R017.
        let mut relays = vec![
            relay_with("R013", Some(TicketStatus::Open), None),
            relay_with("R017", Some(TicketStatus::Open), Some("R013")),
        ];
        let tickets = vec![ticket_with(
            "R017-T1",
            Some(TicketStatus::InProgress),
            Some("R017"),
        )];
        apply_derived_relay_fields(&mut relays, &tickets);
        let r013 = relays.iter().find(|r| r.id == "R013").unwrap();
        let r017 = relays.iter().find(|r| r.id == "R017").unwrap();
        assert_eq!(r017.anno.status, Some(TicketStatus::InProgress));
        assert_eq!(r013.anno.status, Some(TicketStatus::InProgress));
    }

    #[test]
    fn derive_relay_ts_rolls_up_max_across_descendants() {
        let mut relays = vec![{
            let mut w = relay_with("R017", Some(TicketStatus::Open), None);
            w.last_modified_ts = 1_000;
            w
        }];
        let tickets = vec![
            {
                let mut w = ticket_with("R017-T1", Some(TicketStatus::Open), Some("R017"));
                w.last_modified_ts = 5_000;
                w
            },
            {
                let mut w = ticket_with("R017-T2", Some(TicketStatus::Open), Some("R017"));
                w.last_modified_ts = 3_000;
                w
            },
        ];
        apply_derived_relay_fields(&mut relays, &tickets);
        assert_eq!(relays[0].last_modified_ts, 5_000);
    }

    #[test]
    fn derive_relay_status_is_idempotent() {
        // Running derivation twice should produce the same result —
        // critical for the UI which still calls withDerivedRelayFields
        // on data the daemon has already derived.
        let mut relays = vec![relay_with("R017", Some(TicketStatus::Open), None)];
        let tickets = vec![ticket_with(
            "R017-T1",
            Some(TicketStatus::Handoff),
            Some("R017"),
        )];
        apply_derived_relay_fields(&mut relays, &tickets);
        let after_first = relays[0].anno.status;
        apply_derived_relay_fields(&mut relays, &tickets);
        assert_eq!(relays[0].anno.status, after_first);
        assert_eq!(after_first, Some(TicketStatus::Handoff));
    }
}
