//! End-to-end annotation tests: index a fixture file, run apply_pass,
//! query the resulting graph + annotation index.

use std::path::Path;
use yah_kg::anno::{AnnotationKind, TagRef, TicketStatus};
use yah_kg::edge::EdgeKind;
use yah_kg::indexer::LanguageIndexer;
use yah_kg::kind::{CommonKind, NodeKind};
use yah_kg::rpc::Direction;
use yah_kg_anno::{apply_pass, AnnotationIndex};
use yah_kg_rust::RustIndexer;
use yah_kg_store::{Store, StoreSink};

const SRC: &str = r#"
//! @yah:tag(layer:core)
//!
//! Crate-level mixer module.

/// Top-level mixer.
///
/// @yah:tag(audio, hot-path)
pub struct AudioMixer {
    pub gain: f32,
}

/// Frame buffer dispatcher.
///
/// @yah:flow(AudioMixer, "shared frame buffer")
pub struct Dispatcher;

/// @yah:rule(no-import-of: tag(view))
pub mod inner {}

/// Just docs, no annotations.
pub fn untagged() {}

/// Bad tag (space inside) should produce a parse error and not attach.
///
/// @yah:tag(invalid space)
pub fn malformed() {}
"#;

fn build() -> (Store, AnnotationIndex) {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        RustIndexer::new()
            .index_file(Path::new("src/lib.rs"), SRC, &mut sink)
            .expect("indexer succeeds");
    }
    let mut idx = AnnotationIndex::new();
    apply_pass(&mut store, &mut idx, None);
    (store, idx)
}

fn lookup_label(store: &Store, label: &str) -> Option<yah_kg::ids::NodeId> {
    store
        .all_node_refs()
        .find(|n| n.label == label)
        .map(|n| n.id)
}

#[test]
fn tag_creates_synthetic_tag_nodes_and_tag_edges() {
    let (store, _idx) = build();

    // The synthetic Tag nodes exist.
    let tag_nodes: Vec<&yah_kg::ids::NodeRef> = store
        .all_node_refs()
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::Tag)))
        .collect();
    let tag_qualified: Vec<&str> = tag_nodes.iter().map(|n| n.qualified.as_str()).collect();
    assert!(
        tag_qualified.contains(&"tag:layer:core"),
        "missing layer:core tag; got {tag_qualified:?}"
    );
    assert!(tag_qualified.contains(&"tag:audio"), "missing audio tag");
    assert!(tag_qualified.contains(&"tag:hot-path"), "missing hot-path tag");

    // The struct has Tag edges to audio + hot-path.
    let mixer = lookup_label(&store, "AudioMixer").expect("AudioMixer node");
    let outs = store.neighbors(mixer, Direction::Out, Some(&[EdgeKind::Tag]));
    let targets: Vec<String> = outs
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(
        targets.contains(&"audio".to_string())
            && targets.contains(&"hot-path".to_string()),
        "AudioMixer should be tagged audio + hot-path; got {targets:?}"
    );
}

#[test]
fn tag_attaches_to_file_node_for_inner_doc_comments() {
    let (store, _idx) = build();
    // `//! @yah:tag(layer:core)` at file top should attach to the File node.
    let file_id = lookup_label(&store, "lib.rs").expect("file node");
    let outs = store.neighbors(file_id, Direction::Out, Some(&[EdgeKind::Tag]));
    let targets: Vec<String> = outs
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(
        targets.contains(&"layer:core".to_string()),
        "file should be tagged layer:core; got {targets:?}"
    );
}

#[test]
fn flow_resolves_within_file_and_emits_flow_edge() {
    let (store, _idx) = build();
    let dispatcher = lookup_label(&store, "Dispatcher").expect("Dispatcher");
    let outs = store.neighbors(dispatcher, Direction::Out, Some(&[EdgeKind::Flow]));
    assert_eq!(outs.len(), 1, "Dispatcher should have one flow edge");
    assert_eq!(
        store.node_ref(outs[0].to).map(|n| n.label.as_str()),
        Some("AudioMixer")
    );
}

#[test]
fn annotation_index_carries_typed_annotations_per_node() {
    let (store, idx) = build();

    let mixer = lookup_label(&store, "AudioMixer").unwrap();
    let anns = idx.get(mixer);
    assert_eq!(anns.len(), 2, "audio + hot-path");
    let names: Vec<TagRef> = anns
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Tag(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&TagRef::new("audio")));
    assert!(names.contains(&TagRef::new("hot-path")));
    // Source provenance is populated.
    assert_eq!(anns[0].source_file, "src/lib.rs");
    assert!(anns[0].source_line > 0);

    let dispatcher = lookup_label(&store, "Dispatcher").unwrap();
    let dispatch_anns = idx.get(dispatcher);
    let flow = dispatch_anns
        .iter()
        .find_map(|a| match &a.kind {
            AnnotationKind::Flow {
                to_qualified,
                reason,
            } => Some((to_qualified.clone(), reason.clone())),
            _ => None,
        })
        .expect("flow annotation");
    assert_eq!(flow.0, "AudioMixer");
    assert_eq!(flow.1.as_deref(), Some("shared frame buffer"));

    // Rule annotation roundtrips even though we don't validate yet.
    let inner = lookup_label(&store, "inner").unwrap();
    let inner_anns = idx.get(inner);
    let rule = inner_anns
        .iter()
        .find_map(|a| match &a.kind {
            AnnotationKind::Rule { rule_kind, args } => Some((rule_kind.clone(), args.clone())),
            _ => None,
        })
        .expect("rule annotation");
    assert_eq!(rule.0, "no-import-of");
    assert_eq!(rule.1, vec!["tag(view)".to_string()]);
}

#[test]
fn untagged_node_has_no_annotations_or_tag_edges() {
    let (store, idx) = build();
    let untagged = lookup_label(&store, "untagged").unwrap();
    assert!(idx.get(untagged).is_empty());
    let outs = store.neighbors(
        untagged,
        Direction::Out,
        Some(&[EdgeKind::Tag, EdgeKind::Flow]),
    );
    assert!(outs.is_empty());
}

#[test]
fn malformed_tag_does_not_attach() {
    let (store, idx) = build();
    let malformed = lookup_label(&store, "malformed").unwrap();
    // The bad `@yah:tag(invalid space)` doesn't produce an annotation.
    let tags: Vec<&yah_kg::anno::AnnotationRef> = idx
        .get(malformed)
        .iter()
        .filter(|a| matches!(a.kind, AnnotationKind::Tag(_)))
        .collect();
    assert!(
        tags.is_empty(),
        "malformed tag should not attach; got {tags:?}"
    );
    let outs = store.neighbors(malformed, Direction::Out, Some(&[EdgeKind::Tag]));
    assert!(outs.is_empty());
}

#[test]
fn rerunning_apply_pass_replaces_previous_state() {
    // Re-applying the pass should be idempotent — same nodes, same edges,
    // no duplicate AnnotationRefs in the index.
    let (mut store, mut idx) = build();
    let before_tag_edges = count_tag_edges(&store);
    let before_index_len = idx.len();

    apply_pass(&mut store, &mut idx, None);
    apply_pass(&mut store, &mut idx, None);

    assert_eq!(count_tag_edges(&store), before_tag_edges);
    assert_eq!(idx.len(), before_index_len);
}

fn count_tag_edges(store: &Store) -> usize {
    let mut count = 0;
    for n in store.all_node_refs() {
        count += store
            .neighbors(n.id, Direction::Out, Some(&[EdgeKind::Tag]))
            .len();
    }
    count
}

const TICKET_SRC: &str = r#"
//! @yah:relay(R042, "Sample relay for tests")
//! @yah:status(in-progress)
//! @yah:assignee(agent:claude)
//! @yah:parent(R013)
//! @yah:phase(P2)
//! @yah:handoff("First chunk landed.")
//! @yah:next("Wire up the next bit")
//! @yah:next("Then ship it")
//! @yah:gotcha("watcher races on save")
//!
//! @yah:ticket(R042-T1, "Sub-task")
//! @yah:status(open)
//! @yah:parent(R042)
//! @yah:kind(bug)
//! @yah:severity(high)
//! @yah:verify("cargo test")

pub fn carrier() {}
"#;

#[test]
fn relay_and_ticket_round_trip_through_annotation_index() {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        RustIndexer::new()
            .index_file(Path::new("src/lib.rs"), TICKET_SRC, &mut sink)
            .expect("indexer succeeds");
    }
    let mut idx = AnnotationIndex::new();
    apply_pass(&mut store, &mut idx, None);

    // Both blocks live on the file's module-level `//!` doc — File node.
    let file_node = lookup_label(&store, "lib.rs").expect("file node");
    let anns = idx.get(file_node);

    let relay = anns
        .iter()
        .find_map(|a| match &a.kind {
            AnnotationKind::Relay(w) => Some(w),
            _ => None,
        })
        .expect("relay annotation");
    assert_eq!(relay.id, "R042");
    assert_eq!(relay.title, "Sample relay for tests");
    assert_eq!(relay.status, Some(TicketStatus::InProgress));
    assert_eq!(relay.assignee.as_deref(), Some("agent:claude"));
    assert_eq!(relay.parent.as_deref(), Some("R013"));
    assert_eq!(relay.phase.as_deref(), Some("P2"));
    assert_eq!(relay.handoff, vec!["First chunk landed.".to_string()]);
    assert_eq!(
        relay.next_steps,
        vec!["Wire up the next bit".to_string(), "Then ship it".to_string()]
    );
    assert_eq!(relay.gotchas, vec!["watcher races on save".to_string()]);

    let ticket = anns
        .iter()
        .find_map(|a| match &a.kind {
            AnnotationKind::Ticket(w) => Some(w),
            _ => None,
        })
        .expect("ticket annotation");
    assert_eq!(ticket.id, "R042-T1");
    assert_eq!(ticket.status, Some(TicketStatus::Open));
    assert_eq!(ticket.parent.as_deref(), Some("R042"));
    assert_eq!(ticket.kind.as_deref(), Some("bug"));
    assert_eq!(ticket.severity.as_deref(), Some("high"));
    assert_eq!(ticket.verify, vec!["cargo test".to_string()]);
}

// ---------- Pass 2: synthetic Relay/Ticket nodes + edges ----------

fn build_with_tickets() -> (Store, AnnotationIndex, yah_kg_anno::ApplySummary) {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        RustIndexer::new()
            .index_file(Path::new("src/lib.rs"), TICKET_SRC, &mut sink)
            .expect("indexer succeeds");
    }
    let mut idx = AnnotationIndex::new();
    let summary = apply_pass(&mut store, &mut idx, None);
    (store, idx, summary)
}

fn lookup_qualified(store: &Store, qualified: &str) -> Option<yah_kg::ids::NodeId> {
    store
        .all_node_refs()
        .find(|n| n.qualified == qualified)
        .map(|n| n.id)
}

#[test]
fn work_items_become_synthetic_graph_nodes() {
    let (store, _idx, _summary) = build_with_tickets();

    let relay = lookup_qualified(&store, "relay:R042").expect("synthetic relay node");
    let ticket = lookup_qualified(&store, "ticket:R042-T1").expect("synthetic ticket node");
    // Relay R013 is referenced as the relay's parent — should exist as a stub.
    let parent_relay = lookup_qualified(&store, "relay:R013").expect("synthetic parent relay stub");

    let relay_ref = store.node_ref(relay).unwrap();
    assert!(matches!(
        relay_ref.kind,
        NodeKind::Common(CommonKind::Relay)
    ));
    assert!(relay_ref.synthetic);
    assert_eq!(relay_ref.label, "R042");

    let ticket_ref = store.node_ref(ticket).unwrap();
    assert!(matches!(
        ticket_ref.kind,
        NodeKind::Common(CommonKind::Ticket)
    ));
    assert!(ticket_ref.synthetic);

    let parent_ref = store.node_ref(parent_relay).unwrap();
    assert!(matches!(
        parent_ref.kind,
        NodeKind::Common(CommonKind::Relay)
    ));
}

#[test]
fn parent_item_edge_links_ticket_to_relay() {
    let (store, _idx, _summary) = build_with_tickets();
    let ticket = lookup_qualified(&store, "ticket:R042-T1").unwrap();
    let parents = store.neighbors(ticket, Direction::Out, Some(&[EdgeKind::ParentItem]));
    assert_eq!(parents.len(), 1);
    let target = store.node_ref(parents[0].to).unwrap();
    assert_eq!(target.qualified, "relay:R042");
}

#[test]
fn anchors_edge_points_from_structural_anchor_to_synthetic_node() {
    let (store, _idx, _summary) = build_with_tickets();
    let file = lookup_label(&store, "lib.rs").expect("file node hosting the doc");
    let anchored = store.neighbors(file, Direction::Out, Some(&[EdgeKind::Anchors]));
    let qualified: Vec<&str> = anchored
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.qualified.as_str()))
        .collect();
    assert!(
        qualified.contains(&"relay:R042"),
        "file should anchor relay:R042; got {qualified:?}"
    );
    assert!(
        qualified.contains(&"ticket:R042-T1"),
        "file should anchor ticket:R042-T1; got {qualified:?}"
    );
}

#[test]
fn touched_work_items_are_reported_in_summary() {
    let (_store, _idx, summary) = build_with_tickets();
    let ids: Vec<&str> = summary
        .touched_work_items
        .iter()
        .map(|t| t.work_item_id.as_str())
        .collect();
    assert!(ids.contains(&"R042"), "summary should list R042; got {ids:?}");
    assert!(
        ids.contains(&"R042-T1"),
        "summary should list R042-T1; got {ids:?}"
    );
    // No duplicates: each touched node id appears once even on a full pass.
    let mut node_ids: Vec<_> = summary.touched_work_items.iter().map(|t| t.node).collect();
    node_ids.sort();
    let before = node_ids.len();
    node_ids.dedup();
    assert_eq!(before, node_ids.len(), "touched_work_items should be deduped");
}

fn count_anchor_edges(store: &Store) -> usize {
    let ids: Vec<_> = store.all_node_refs().map(|n| n.id).collect();
    let mut total = 0;
    for id in ids {
        total += store
            .neighbors(id, Direction::Out, Some(&[EdgeKind::Anchors]))
            .len();
    }
    total
}

#[test]
fn reapplying_does_not_duplicate_anchor_or_parent_edges() {
    let (mut store, mut idx, _) = build_with_tickets();
    let before = count_anchor_edges(&store);
    apply_pass(&mut store, &mut idx, None);
    apply_pass(&mut store, &mut idx, None);
    let after = count_anchor_edges(&store);
    assert_eq!(before, after);
}

#[test]
fn orphan_synthetic_tag_node_is_gced_when_last_tag_edge_disappears() {
    use yah_kg_store::{reindex_file, IndexerRegistry};

    let dir = tempfile::tempdir().unwrap();
    let rel = "src/lib.rs";
    let abs = dir.path().join(rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, "/// @yah:tag(audio)\npub struct Mixer;\n").unwrap();

    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(RustIndexer::new()));
    let mut store = Store::new();
    yah_kg_store::walk_and_index(dir.path(), &mut store, &registry).unwrap();
    let mut idx = AnnotationIndex::new();
    apply_pass(&mut store, &mut idx, None);

    assert!(
        lookup_qualified(&store, "tag:audio").is_some(),
        "synthetic Tag node should exist while the @yah:tag is in source"
    );

    // Drop the tag, reindex, reapply.
    std::fs::write(&abs, "pub struct Mixer;\n").unwrap();
    reindex_file(dir.path(), rel, &mut store, &registry).unwrap();
    let scope: std::collections::HashSet<_> = store.lookup(rel, None).into_iter().collect();
    let summary = apply_pass(&mut store, &mut idx, Some(&scope));

    assert!(
        lookup_qualified(&store, "tag:audio").is_none(),
        "synthetic tag:audio should be gced once nothing references it"
    );
    assert!(
        summary.orphans_gced >= 1,
        "summary should record the orphan sweep; got {}",
        summary.orphans_gced
    );
}

#[test]
fn orphan_synthetic_relay_and_stub_parent_are_gced_after_file_deletion() {
    use yah_kg_store::{reindex_file, IndexerRegistry};

    let dir = tempfile::tempdir().unwrap();
    let rel = "src/lib.rs";
    let abs = dir.path().join(rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, TICKET_SRC).unwrap();

    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(RustIndexer::new()));
    let mut store = Store::new();
    yah_kg_store::walk_and_index(dir.path(), &mut store, &registry).unwrap();
    let mut idx = AnnotationIndex::new();
    apply_pass(&mut store, &mut idx, None);

    // Authored relay/ticket and the stub parent (R013) all live now.
    assert!(lookup_qualified(&store, "relay:R042").is_some());
    assert!(lookup_qualified(&store, "ticket:R042-T1").is_some());
    assert!(
        lookup_qualified(&store, "relay:R013").is_some(),
        "parent stub for R013 should exist while a child references it"
    );

    // Delete the file → reindex drops every structural node it owned →
    // synthetic Relay/Ticket nodes lose their last Anchors edges → their
    // outgoing ParentItem edges cascade away → the parent stub becomes
    // orphan in the second iteration of the GC fixpoint.
    std::fs::remove_file(&abs).unwrap();
    reindex_file(dir.path(), rel, &mut store, &registry).unwrap();
    let summary = apply_pass(&mut store, &mut idx, None);

    assert!(
        lookup_qualified(&store, "relay:R042").is_none(),
        "authored relay should be gced once its anchor disappears"
    );
    assert!(
        lookup_qualified(&store, "ticket:R042-T1").is_none(),
        "authored ticket should be gced once its anchor disappears"
    );
    assert!(
        lookup_qualified(&store, "relay:R013").is_none(),
        "stub parent should be gced once nothing references it"
    );
    assert!(
        summary.orphans_gced >= 3,
        "expected >=3 orphans (relay, ticket, parent stub); got {}",
        summary.orphans_gced
    );
}

#[test]
fn changing_parent_field_replaces_parent_item_edge() {
    // First state: ticket parents R042.
    let (store, _idx, _) = build_with_tickets();
    let ticket = lookup_qualified(&store, "ticket:R042-T1").unwrap();
    let initial = store.neighbors(ticket, Direction::Out, Some(&[EdgeKind::ParentItem]));
    assert_eq!(initial.len(), 1);
    assert_eq!(
        store.node_ref(initial[0].to).unwrap().qualified,
        "relay:R042"
    );

    // Re-index with parent flipped to R013.
    const REPARENTED: &str = r#"
//! @yah:relay(R042, "Sample relay for tests")
//! @yah:status(in-progress)
//!
//! @yah:ticket(R042-T1, "Sub-task")
//! @yah:status(open)
//! @yah:parent(R013)

pub fn carrier() {}
"#;
    let mut store2 = Store::new();
    {
        let mut sink = StoreSink::new(&mut store2);
        RustIndexer::new()
            .index_file(Path::new("src/lib.rs"), REPARENTED, &mut sink)
            .expect("indexer succeeds");
    }
    let mut idx2 = AnnotationIndex::new();
    apply_pass(&mut store2, &mut idx2, None);
    let ticket2 = lookup_qualified(&store2, "ticket:R042-T1").unwrap();
    let parents = store2.neighbors(ticket2, Direction::Out, Some(&[EdgeKind::ParentItem]));
    assert_eq!(parents.len(), 1);
    assert_eq!(
        store2.node_ref(parents[0].to).unwrap().qualified,
        "relay:R013",
        "parent edge should point at the new parent relay only"
    );
}
