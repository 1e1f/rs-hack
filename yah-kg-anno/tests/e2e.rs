//! End-to-end annotation tests: index a fixture file, run apply_pass,
//! query the resulting graph + annotation index.

use std::path::Path;
use yah_kg::anno::{AnnotationKind, TagRef};
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
