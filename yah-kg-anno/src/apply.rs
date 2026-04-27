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
use crate::parser::{parse_doc, RawAnnotation};
use std::collections::HashSet;
use yah_kg::anno::{AnnotationKind, AnnotationRef, TagRef};
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg_store::Store;

const TAG_FILE_SENTINEL: &str = "<tag>";

#[derive(Debug, Default, Clone)]
pub struct ApplySummary {
    pub nodes_scanned: u32,
    pub annotations_attached: u32,
    pub tag_edges_added: u32,
    pub flow_edges_added: u32,
    pub flows_unresolved: u32,
    pub parse_errors: u32,
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

    for (id, doc) in targets {
        summary.nodes_scanned += 1;
        let result = apply_to_node(store, index, id, doc.as_deref());
        summary.annotations_attached += result.annotations_attached;
        summary.tag_edges_added += result.tag_edges_added;
        summary.flow_edges_added += result.flow_edges_added;
        summary.flows_unresolved += result.flows_unresolved;
        summary.parse_errors += result.parse_errors;
    }
    summary
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
        // Skip synthetic Tag nodes — they don't author annotations.
        if matches!(entry.kind, NodeKind::Common(CommonKind::Tag)) {
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
    // Wipe any previous Tag edges from this node — they're entirely
    // derived from the doc, so we re-build from scratch.
    let prev_tag_edges: Vec<EdgeId> = store
        .neighbors(node, yah_kg::rpc::Direction::Out, Some(&[EdgeKind::Tag]))
        .into_iter()
        .map(|e| e.id)
        .collect();
    for eid in prev_tag_edges {
        store.remove_edge(eid);
    }
    let prev_flow_edges: Vec<EdgeId> = store
        .neighbors(node, yah_kg::rpc::Direction::Out, Some(&[EdgeKind::Flow]))
        .into_iter()
        .map(|e| e.id)
        .collect();
    for eid in prev_flow_edges {
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

