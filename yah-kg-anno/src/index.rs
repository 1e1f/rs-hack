//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Side index from `NodeId` to its `AnnotationRef`s.
//!
//! Annotations live both in the graph (as Tag/Flow edges) and in this
//! side index (as typed `AnnotationRef` values). The index powers
//! `arch.node`'s `annotations` field — the UI fetches one node and gets
//! its full overlay in one round-trip without traversing the graph.
//!
//! @yah:ticket(R017-F4, "Relay/Ticket annotation kinds in yah-kg-anno (richer payloads than Tag/Flow/Rule)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R017)
//! @yah:handoff("Passes 1–5 of R017-F4 landed: WorkItemAnno parser + synthetic Relay/Ticket nodes (Pass 1–2), arch.list_tickets/list_relays/get_ticket RPC + orphan-GC sweep (Pass 3), yah-kg::board recompute layer (Pass 4), and now the CLI swap (Pass 5) — yah/src/arch/ticket.rs's TicketBoard::from_annotations delegates cross-anchor recompute (epic inference, scalar conflicts) to yah_kg::board::Board::from_work_items. Per-file fold_file + PartialTicket survive; build_item/merge_scalar/record_conflict/extend_dedup/resolve_epics/compute_epic_status are gone. A Sidecar carries depends_on/see_also/target and the cross-anchor vec union (handoff/next/cleanup/verify/gotchas/assumes) since Board only reads anchors[0].anno from the wire DTO. yah-kg added as a path dep on yah/Cargo.toml. yah lib 147/147, arch_dogfood 26/26, arch_non_rust_extract 10/10, yah-kg 20/20, yah-kg-anno 19/19, yah-kg-daemon e2e 12/13 (macOS gotcha unchanged); yah board show/status dogfood clean. Remaining: hack-board frontend swap onto arch.list_tickets / arch.list_relays / arch.get_ticket — Tauri-track agent owns it.")
//! @yah:next("Re-target the hack-board frontend to arch.list_tickets / arch.list_relays / arch.get_ticket — owned by the yah-ui / Tauri-as-server track. After the frontend lands, archive R017-F4.")
//! @yah:gotcha("yah-kg-daemon test `reindex_after_disk_delete_wipes_file_nodes` is pre-existing red on macOS — relativize() canonicalize-fallback when the file is gone makes reindex_path early-return. Unrelated to this pass.")
//!
//! @yah:ticket(R028-F1, "Annotation schema: @yah:think(...) + @yah:engine(...)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R028)
//! @yah:think(deep)
//! @yah:engine(claude:opus-4-7)
//! @yah:next("Add Think/Engine annotation kinds alongside existing Tag/Flow/Rule")
//! @yah:next("Reader exposes resolved values to prelude assembler")
//! @yah:verify("yah scan finds @yah:think(deep) on a test ticket and reports it via board ticket --json")
//! @yah:handoff("Schema added end-to-end. yah-kg/src/anno.rs: new ThinkBudget enum (Deep|Standard|Fast|Budget{tokens}) and EngineRef struct (provider:Option<model>) with parse/as_payload/is_claude helpers; WorkItemAnno gains optional think/engine fields. yah-kg-anno/src/parser.rs: 'think' and 'engine' join the modifier whitelist; WorkItemBuilder::apply_modifier delegates to ThinkBudget::parse / EngineRef::parse, surfacing malformed payloads as ParseError without aborting the block. Legacy CLI track (yah/src/arch/): new ArchKind::Think(ThinkBudget) and ArchKind::Engine(EngineRef) variants, ArchKind::parse cases (malformed payloads degrade to ArchKind::Unknown), is_hack_relevant + arch::graph match arms updated, PartialTicket+fold_file+partial_to_anchor+board_item_to_ticket+ticket_to_work_item all carry the new fields, and the legacy Ticket struct exposes them with skip-if-None serde so existing JSON consumers don't see noise. Tests: 8 new (5 yah-kg::anno parse/round-trip, 4 yah-kg-anno parser, 2 yah arch::ticket end-to-end). Dogfooded on R028-F1 itself (added @yah:think(deep) + @yah:engine(claude:opus-4-7) above) — yah board tickets --format json | jq '.[].think' shows {mode:deep} and engine {provider:claude,model:opus-4-7}, satisfying the verify literally. Workspace: cargo test --workspace green.")
//! @yah:next("R028-F2 prelude assembler reads anno.think/engine; engine.is_claude() drives the runner-dispatch matrix at the Tauri command surface (R018-F3)")
//! @yah:next("Frontend: surface @yah:engine on the ticket card as an engine chip; @yah:think as a thinking-budget badge")

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use yah_kg::anno::AnnotationRef;
use yah_kg::ids::NodeId;

#[derive(Debug, Default, Clone)]
pub struct AnnotationIndex {
    by_node: HashMap<NodeId, Vec<AnnotationRef>>,
}

/// Serializable form of [`AnnotationIndex`]. Sorted by node id on emit so
/// snapshot files diff cleanly. Wholly replaces in-memory state on
/// [`AnnotationIndex::restore`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationIndexSnapshot {
    pub entries: Vec<(NodeId, Vec<AnnotationRef>)>,
}

impl AnnotationIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn to_snapshot(&self) -> AnnotationIndexSnapshot {
        let mut entries: Vec<(NodeId, Vec<AnnotationRef>)> = self
            .by_node
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        AnnotationIndexSnapshot { entries }
    }

    pub fn restore(&mut self, snap: AnnotationIndexSnapshot) {
        self.by_node.clear();
        for (id, anns) in snap.entries {
            if !anns.is_empty() {
                self.by_node.insert(id, anns);
            }
        }
    }

    /// Wholesale-replace the annotations attached to `node`. Called by the
    /// applier when a file is reindexed — annotations on a node are
    /// always derived from one source location, so atomic replace is
    /// the right semantics.
    pub fn set(&mut self, node: NodeId, anns: Vec<AnnotationRef>) {
        if anns.is_empty() {
            self.by_node.remove(&node);
        } else {
            self.by_node.insert(node, anns);
        }
    }

    pub fn get(&self, node: NodeId) -> &[AnnotationRef] {
        self.by_node
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn remove(&mut self, node: NodeId) {
        self.by_node.remove(&node);
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &[AnnotationRef])> {
        self.by_node.iter().map(|(k, v)| (*k, v.as_slice()))
    }

    pub fn len(&self) -> usize {
        self.by_node.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_node.is_empty()
    }
}
