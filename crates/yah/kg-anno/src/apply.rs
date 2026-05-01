//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Pass 4: apply parsed annotations to the graph.
//!
//! For each node with a doc string: parse `@yah:` directives, materialize
//! synthetic Tag nodes + Tag edges, resolve Flow targets by qualified-name
//! suffix, and write `AnnotationRef` values into the [`AnnotationIndex`].
//!
//! Tag node identity: `NodeId::compute(Lang::Rust, &tag.qualified(),
//! "<tag>")`. The Lang is a sentinel — Tag nodes are language-agnostic
//! but the contract requires assigning some `Lang`. The fixed
//! `"<tag>"` file marker keeps tag ids from colliding with structural
//! nodes that happen to share a qualified name.

use crate::index::AnnotationIndex;
use crate::parser::{parse_doc, RawAnnotation, WorkItemType};
use std::collections::HashSet;
use kg::anno::{AnnotationKind, AnnotationRef, TagRef, WorkItemAnno};
use kg::edge::{EdgeId, EdgeKind, EdgeOut};
use kg::ids::{NodeId, NodeRef, Span};
use kg::kind::{CommonKind, Lang, NodeKind};
use kg_store::Store;

const TAG_FILE_SENTINEL: &str = "<tag>";
const WORK_ITEM_FILE_SENTINEL: &str = "<work-item>";

/// One work-item touched by `apply_pass` / `apply_to_node`. The daemon
/// converts these into `RelayChanged` / `TicketChanged` events so the
/// Board UI can refresh per-relay rather than re-rendering the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchedWorkItem {
    pub item_type: WorkItemType,
    pub node: NodeId,
    pub work_item_id: String,
}

#[derive(Debug, Default, Clone)]
pub struct ApplySummary {
    pub nodes_scanned: u32,
    pub annotations_attached: u32,
    pub tag_edges_added: u32,
    pub flow_edges_added: u32,
    pub flows_unresolved: u32,
    pub parse_errors: u32,
    /// Work-items upserted during this pass, deduped by synthetic node id.
    pub touched_work_items: Vec<TouchedWorkItem>,
    /// Synthetic Tag/Relay/Ticket nodes removed by the orphan sweep that
    /// runs at the end of every full apply pass. A non-zero count means
    /// the prior pass had nodes that are no longer referenced — the sweep
    /// keeps `arch.list_*` from leaking authored work that has since been
    /// deleted from source.
    pub orphans_gced: u32,
}

/// Run the annotation pass over every node currently in `store`. Replaces
/// any previously-attached annotations and Tag/Flow edges for each node
/// — call this after each reindex.
///
/// `node_filter` lets the daemon scope work to a single file's nodes
/// during incremental reindex; pass `None` for "every node".
pub fn apply_pass(
    store: &mut Store,
    index: &mut AnnotationIndex,
    node_filter: Option<&HashSet<NodeId>>,
) -> ApplySummary {
    let mut summary = ApplySummary::default();

    // Snapshot the targets so we can mutate the store inside the loop.
    let targets: Vec<(NodeId, Option<String>)> = collect_targets(store, node_filter);

    let mut seen: HashSet<NodeId> = HashSet::new();
    for (id, doc) in targets {
        summary.nodes_scanned += 1;
        let result = apply_to_node(store, index, id, doc.as_deref());
        summary.annotations_attached += result.annotations_attached;
        summary.tag_edges_added += result.tag_edges_added;
        summary.flow_edges_added += result.flow_edges_added;
        summary.flows_unresolved += result.flows_unresolved;
        summary.parse_errors += result.parse_errors;
        for touched in result.touched_work_items {
            if seen.insert(touched.node) {
                summary.touched_work_items.push(touched);
            }
        }
    }

    summary.orphans_gced = gc_orphan_synthetics(store);
    summary
}

/// Drop synthetic Tag/Relay/Ticket nodes that no structural node still
/// references. Runs after a full apply pass — by then every surviving
/// annotation has rebuilt its outgoing Tag/Anchors/ParentItem edges, so
/// any synthetic without sustaining incoming edges is genuinely orphan.
///
/// Sustaining edges by node kind:
/// - `Tag`: incoming `EdgeKind::Tag` from a structural anchor.
/// - `Relay`/`Ticket`: incoming `EdgeKind::Anchors` from a structural
///   anchor *or* incoming `EdgeKind::ParentItem` from a child work-item
///   (so a parent stub stays alive while its children still reference
///   it; the stub-only filter in `build_work_item` keeps it out of the
///   surfaced list).
///
/// Iterates to a fixpoint: removing a Relay/Ticket cascades its outgoing
/// `ParentItem` edge, which can in turn orphan the parent stub.
fn gc_orphan_synthetics(store: &mut Store) -> u32 {
    let mut total = 0u32;
    loop {
        let candidates: Vec<(NodeId, NodeKind)> = store
            .all_node_refs()
            .filter(|n| {
                n.synthetic
                    && matches!(
                        n.kind,
                        NodeKind::Common(CommonKind::Tag)
                            | NodeKind::Common(CommonKind::Relay)
                            | NodeKind::Common(CommonKind::Ticket)
                    )
            })
            .map(|n| (n.id, n.kind.clone()))
            .collect();
        let mut removed = 0u32;
        for (id, kind) in candidates {
            let needed: &[EdgeKind] = match kind {
                NodeKind::Common(CommonKind::Tag) => &[EdgeKind::Tag],
                NodeKind::Common(CommonKind::Relay)
                | NodeKind::Common(CommonKind::Ticket) => {
                    &[EdgeKind::Anchors, EdgeKind::ParentItem]
                }
                _ => continue,
            };
            let incoming = store.neighbors(id, rpc::Direction::In, Some(needed));
            if incoming.is_empty() {
                store.remove_node(id);
                removed += 1;
            }
        }
        total += removed;
        if removed == 0 {
            break;
        }
    }
    total
}

fn collect_targets(
    store: &Store,
    filter: Option<&HashSet<NodeId>>,
) -> Vec<(NodeId, Option<String>)> {
    // We need every node in the store. The Store doesn't expose an
    // iterator; use stats() / lookup over each known file. The cheap
    // path: drain all by_file entries.
    let mut out = Vec::new();
    for entry in store.all_node_refs() {
        if let Some(f) = filter {
            if !f.contains(&entry.id) {
                continue;
            }
        }
        // Skip synthetic overlay nodes — Tag, Relay, and Ticket nodes
        // never carry source docs.
        if matches!(
            entry.kind,
            NodeKind::Common(CommonKind::Tag)
                | NodeKind::Common(CommonKind::Relay)
                | NodeKind::Common(CommonKind::Ticket)
        ) {
            continue;
        }
        let doc = store.node_full(entry.id).and_then(|n| n.doc);
        out.push((entry.id, doc));
    }
    out
}

#[derive(Debug, Default)]
pub struct PerNodeOutcome {
    pub annotations_attached: u32,
    pub tag_edges_added: u32,
    pub flow_edges_added: u32,
    pub flows_unresolved: u32,
    pub parse_errors: u32,
    /// Work-items upserted while processing this anchor — one per
    /// `@yah:relay`/`@yah:ticket` block.
    pub touched_work_items: Vec<TouchedWorkItem>,
}

/// Apply annotations for a single node. Public for callers that already
/// know which nodes need a refresh (e.g. the daemon during a per-file
/// reindex). Replaces any previously attached annotations + Tag edges.
pub fn apply_to_node(
    store: &mut Store,
    index: &mut AnnotationIndex,
    node: NodeId,
    doc: Option<&str>,
) -> PerNodeOutcome {
    // Wipe any previous overlay edges from this node — Tag, Flow, and
    // Anchors are all entirely derived from the doc, so we re-build from
    // scratch on every reapply.
    let prev_overlay_edges: Vec<EdgeId> = store
        .neighbors(
            node,
            rpc::Direction::Out,
            Some(&[EdgeKind::Tag, EdgeKind::Flow, EdgeKind::Anchors]),
        )
        .into_iter()
        .map(|e| e.id)
        .collect();
    for eid in prev_overlay_edges {
        store.remove_edge(eid);
    }
    index.remove(node);

    let Some(doc) = doc.filter(|d| !d.is_empty()) else {
        return PerNodeOutcome::default();
    };

    let (parsed, errors) = parse_doc(doc);
    let mut outcome = PerNodeOutcome {
        parse_errors: errors.len() as u32,
        ..Default::default()
    };

    // The anchor's source file/line we report on each AnnotationRef. The
    // file is the structural node's file; the line is the node's start
    // line + the parser's line_offset (parser counts within the doc; the
    // doc started at or near the node's first line — close enough for
    // human-pointing purposes).
    let anchor_file = store
        .node_ref(node)
        .map(|n| n.file.clone())
        .unwrap_or_default();
    let anchor_start = store
        .node_ref(node)
        .map(|n| n.span.start_line)
        .unwrap_or(1);

    let mut anns = Vec::new();
    for p in parsed {
        match p.anno {
            RawAnnotation::Tag(tags) => {
                for tag in tags {
                    ensure_tag_node(store, &tag);
                    let tag_id = tag_node_id(&tag);
                    let edge = EdgeOut {
                        id: EdgeId::compute(node, tag_id, &EdgeKind::Tag),
                        from: node,
                        to: tag_id,
                        kind: EdgeKind::Tag,
                        annotations: vec![],
                    };
                    if store.upsert_edge(edge) {
                        outcome.tag_edges_added += 1;
                    }
                    anns.push(AnnotationRef {
                        anchor: node,
                        source_file: anchor_file.clone(),
                        source_line: anchor_start + p.line_offset.saturating_sub(1),
                        kind: AnnotationKind::Tag(tag),
                    });
                    outcome.annotations_attached += 1;
                }
            }
            RawAnnotation::Flow {
                from_qualified: _,
                to_qualified,
                reason,
            } => {
                if let Some(target_id) = resolve_qualified(store, &to_qualified) {
                    let edge = EdgeOut {
                        id: EdgeId::compute(node, target_id, &EdgeKind::Flow),
                        from: node,
                        to: target_id,
                        kind: EdgeKind::Flow,
                        annotations: vec![],
                    };
                    if store.upsert_edge(edge) {
                        outcome.flow_edges_added += 1;
                    }
                } else {
                    outcome.flows_unresolved += 1;
                }
                anns.push(AnnotationRef {
                    anchor: node,
                    source_file: anchor_file.clone(),
                    source_line: anchor_start + p.line_offset.saturating_sub(1),
                    kind: AnnotationKind::Flow {
                        to_qualified,
                        reason,
                    },
                });
                outcome.annotations_attached += 1;
            }
            RawAnnotation::Rule { rule_kind, args } => {
                anns.push(AnnotationRef {
                    anchor: node,
                    source_file: anchor_file.clone(),
                    source_line: anchor_start + p.line_offset.saturating_sub(1),
                    kind: AnnotationKind::Rule { rule_kind, args },
                });
                outcome.annotations_attached += 1;
            }
            RawAnnotation::WorkItem { item_type, anno } => {
                let synthetic_id = upsert_work_item_node(store, item_type, &anno);
                // Anchor: structural node → synthetic work-item.
                let anchor_edge = EdgeOut {
                    id: EdgeId::compute(node, synthetic_id, &EdgeKind::Anchors),
                    from: node,
                    to: synthetic_id,
                    kind: EdgeKind::Anchors,
                    annotations: vec![],
                };
                store.upsert_edge(anchor_edge);
                // Parent: synthetic → parent synthetic. Wipe any prior
                // ParentItem edges out of this synthetic node first so the
                // parent field is the only source of truth.
                let prev_parent_edges: Vec<EdgeId> = store
                    .neighbors(
                        synthetic_id,
                        rpc::Direction::Out,
                        Some(&[EdgeKind::ParentItem]),
                    )
                    .into_iter()
                    .map(|e| e.id)
                    .collect();
                for eid in prev_parent_edges {
                    store.remove_edge(eid);
                }
                if let Some(parent_id_str) = anno.parent.as_deref() {
                    let parent_kind = work_item_kind_from_id(parent_id_str);
                    let parent_node = upsert_work_item_stub(store, parent_kind, parent_id_str);
                    let parent_edge = EdgeOut {
                        id: EdgeId::compute(synthetic_id, parent_node, &EdgeKind::ParentItem),
                        from: synthetic_id,
                        to: parent_node,
                        kind: EdgeKind::ParentItem,
                        annotations: vec![],
                    };
                    store.upsert_edge(parent_edge);
                }
                outcome.touched_work_items.push(TouchedWorkItem {
                    item_type,
                    node: synthetic_id,
                    work_item_id: anno.id.clone(),
                });
                let kind = match item_type {
                    WorkItemType::Relay => AnnotationKind::Relay(anno),
                    WorkItemType::Ticket => AnnotationKind::Ticket(anno),
                };
                anns.push(AnnotationRef {
                    anchor: node,
                    source_file: anchor_file.clone(),
                    source_line: anchor_start + p.line_offset.saturating_sub(1),
                    kind,
                });
                outcome.annotations_attached += 1;
            }
        }
    }

    if !anns.is_empty() {
        index.set(node, anns);
    }
    outcome
}

fn tag_node_id(tag: &TagRef) -> NodeId {
    NodeId::compute(Lang::Rust, &tag.qualified(), TAG_FILE_SENTINEL)
}

fn ensure_tag_node(store: &mut Store, tag: &TagRef) {
    let id = tag_node_id(tag);
    if store.node_ref(id).is_some() {
        return;
    }
    store.upsert_node(NodeRef {
        id,
        lang: Lang::Rust,
        kind: NodeKind::Common(CommonKind::Tag),
        label: tag.label(),
        qualified: tag.qualified(),
        file: TAG_FILE_SENTINEL.to_string(),
        span: Span::point(0, 0),
        synthetic: true,
    });
}

fn work_item_qualified(item_type: WorkItemType, id: &str) -> String {
    match item_type {
        WorkItemType::Relay => format!("relay:{id}"),
        WorkItemType::Ticket => format!("ticket:{id}"),
    }
}

fn work_item_node_id(item_type: WorkItemType, id: &str) -> NodeId {
    NodeId::compute(
        Lang::Rust,
        &work_item_qualified(item_type, id),
        WORK_ITEM_FILE_SENTINEL,
    )
}

/// Convention from the hack-board ID scheme: bare `R042` is a relay,
/// compound `R042-T1` is a ticket. Used to infer the kind of a parent
/// reference, which doesn't carry a header line of its own.
fn work_item_kind_from_id(id: &str) -> WorkItemType {
    if id.contains('-') {
        WorkItemType::Ticket
    } else {
        WorkItemType::Relay
    }
}

/// Upsert the synthetic node for a fully-known work item, picking up
/// title/qualified name from the annotation payload.
fn upsert_work_item_node(
    store: &mut Store,
    item_type: WorkItemType,
    anno: &WorkItemAnno,
) -> NodeId {
    let id = work_item_node_id(item_type, &anno.id);
    let qualified = work_item_qualified(item_type, &anno.id);
    let label = anno.id.clone();
    let kind = match item_type {
        WorkItemType::Relay => CommonKind::Relay,
        WorkItemType::Ticket => CommonKind::Ticket,
    };
    store.upsert_node(NodeRef {
        id,
        lang: Lang::Rust,
        kind: NodeKind::Common(kind),
        label,
        qualified,
        file: WORK_ITEM_FILE_SENTINEL.to_string(),
        span: Span::point(0, 0),
        synthetic: true,
    });
    id
}

/// Upsert a synthetic placeholder for a referenced work item we haven't
/// scanned the body of yet (e.g. the parent in `@yah:parent(R013)`). If
/// the real work-item is later encountered, `upsert_work_item_node`
/// overwrites this with the full payload.
fn upsert_work_item_stub(
    store: &mut Store,
    item_type: WorkItemType,
    id_str: &str,
) -> NodeId {
    let id = work_item_node_id(item_type, id_str);
    if store.node_ref(id).is_some() {
        return id;
    }
    let kind = match item_type {
        WorkItemType::Relay => CommonKind::Relay,
        WorkItemType::Ticket => CommonKind::Ticket,
    };
    store.upsert_node(NodeRef {
        id,
        lang: Lang::Rust,
        kind: NodeKind::Common(kind),
        label: id_str.to_string(),
        qualified: work_item_qualified(item_type, id_str),
        file: WORK_ITEM_FILE_SENTINEL.to_string(),
        span: Span::point(0, 0),
        synthetic: true,
    });
    id
}

/// Resolve a `to_qualified` string from a flow annotation to a NodeId.
///
/// Strategy: prefer an exact qualified-name match; fall back to a unique
/// suffix match (qualified ends with `::needle`). If the suffix match is
/// ambiguous we drop the edge — flows are high-signal annotations and
/// should be authored unambiguously.
fn resolve_qualified(store: &Store, needle: &str) -> Option<NodeId> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let mut exact: Option<NodeId> = None;
    let mut suffix: Vec<NodeId> = Vec::new();
    for n in store.all_node_refs() {
        if n.qualified == needle {
            exact = Some(n.id);
            break;
        }
        if n.qualified.ends_with(&format!("::{needle}")) {
            suffix.push(n.id);
        }
    }
    if let Some(id) = exact {
        return Some(id);
    }
    if suffix.len() == 1 {
        return suffix.into_iter().next();
    }
    None
}

