//! End-to-end: parse a Rust source string with `RustIndexer`, push the
//! results into a `Store`, then exercise the queries the daemon will
//! expose under `arch.subgraph` / `arch.lookup` / `arch.neighbors`.

use std::path::Path;
use kg::edge::EdgeKind;
use kg::indexer::LanguageIndexer;
use kg::kind::{CommonKind, Lang, NodeKind, RustKind};
use rpc::Direction;
use kg_rust::RustIndexer;
use kg_store::{Store, StoreSink};

const SRC: &str = r#"
//! Toy mixer crate.

/// Top-level mixer struct.
#[derive(Debug, Clone)]
pub struct AudioMixer {
    pub gain: f32,
    name: String,
}

pub trait Mixer {
    fn mix(&self, lhs: f32, rhs: f32) -> f32;
    fn name(&self) -> &str { "default" }
}

impl Mixer for AudioMixer {
    fn mix(&self, lhs: f32, rhs: f32) -> f32 {
        (lhs + rhs) * self.gain
    }
}

impl AudioMixer {
    pub fn new(gain: f32) -> Self {
        AudioMixer { gain, name: String::new() }
    }
}

pub enum Channel {
    Left,
    Right,
    Mid,
}

mod inner {
    pub fn helper() {}

    macro_rules! noop {
        () => {};
    }
}
"#;

fn build_store() -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        let indexer = RustIndexer::new();
        indexer
            .index_file(Path::new("src/mixer.rs"), SRC, &mut sink)
            .expect("indexer should succeed on valid Rust");
    }
    store
}

#[test]
fn indexer_emits_file_struct_trait_impl_enum_module() {
    let store = build_store();

    // Lookup at lines we know exist gets us the relevant nodes.
    let at_struct = store.lookup("src/mixer.rs", Some(6));
    assert!(!at_struct.is_empty(), "should hit AudioMixer at line 6");

    // Query `arch.subgraph` from the file root, depth 2 — should reach the
    // struct, trait, impls, enum, and module.
    let file_id = at_struct
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                .unwrap_or(false)
        })
        .expect("file node should be reachable from any line lookup");

    let sg = store.subgraph(file_id, 3, None, None, None, None);
    let kinds: Vec<NodeKind> = sg.nodes.iter().map(|n| n.kind.clone()).collect();
    let labels: Vec<String> = sg.nodes.iter().map(|n| n.label.clone()).collect();

    assert!(
        labels.iter().any(|l| l == "AudioMixer"),
        "missing AudioMixer in {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "Mixer"),
        "missing Mixer trait in {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "Channel"),
        "missing Channel enum in {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "inner"),
        "missing inner module in {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "Left"),
        "missing variant Left in {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "gain"),
        "missing field gain in {labels:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, NodeKind::Rust(RustKind::Trait))),
        "no trait kind in {kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, NodeKind::Rust(RustKind::Impl))),
        "no impl kind in {kinds:?}"
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, NodeKind::Rust(RustKind::MacroDecl(_)))),
        "no macro_rules! decl in {kinds:?}"
    );
}

#[test]
fn impl_for_and_impl_of_trait_edges_resolve_within_module() {
    let store = build_store();

    // Find the trait impl block (Mixer for AudioMixer) and verify both
    // ImplFor → AudioMixer and ImplOfTrait → Mixer edges are in the store.
    let all = store.lookup("src/mixer.rs", None);
    let impl_node = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| {
                    matches!(n.kind, NodeKind::Rust(RustKind::Impl))
                        && n.label.starts_with("impl_Mixer_for_AudioMixer")
                })
                .unwrap_or(false)
        })
        .expect("trait impl should be indexed");

    let out = store.neighbors(impl_node, Direction::Out, None);
    let kinds: Vec<EdgeKind> = out.iter().map(|e| e.kind.clone()).collect();
    assert!(
        kinds.contains(&EdgeKind::ImplFor),
        "ImplFor missing: {kinds:?}"
    );
    assert!(
        kinds.contains(&EdgeKind::ImplOfTrait),
        "ImplOfTrait missing: {kinds:?}"
    );

    // The impl-for target is AudioMixer; the impl-of-trait target is Mixer.
    let impl_for = out
        .iter()
        .find(|e| e.kind == EdgeKind::ImplFor)
        .expect("impl_for edge");
    let impl_of = out
        .iter()
        .find(|e| e.kind == EdgeKind::ImplOfTrait)
        .expect("impl_of_trait edge");

    assert_eq!(
        store.node_ref(impl_for.to).map(|n| n.label.as_str()),
        Some("AudioMixer")
    );
    assert_eq!(
        store.node_ref(impl_of.to).map(|n| n.label.as_str()),
        Some("Mixer")
    );
}

#[test]
fn lookup_returns_innermost_first() {
    let store = build_store();
    // Line 18 is inside `fn mix` inside the trait impl block.
    let hits = store.lookup("src/mixer.rs", Some(18));
    let labels: Vec<String> = hits
        .iter()
        .filter_map(|id| store.node_ref(*id).map(|n| n.label.clone()))
        .collect();
    // First hit should be the most specific (the method), not the file.
    assert!(
        labels.first().is_some_and(|l| l == "mix"),
        "expected innermost-first to surface `mix`; got {labels:?}"
    );
    let kinds: Vec<NodeKind> = hits
        .iter()
        .filter_map(|id| store.node_ref(*id).map(|n| n.kind.clone()))
        .collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, NodeKind::Common(CommonKind::File))),
        "file should still appear among results: {labels:?}"
    );
}

#[test]
fn derives_recorded_as_property() {
    let store = build_store();
    let all = store.lookup("src/mixer.rs", None);
    let mixer_id = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "AudioMixer")
                .unwrap_or(false)
        })
        .expect("AudioMixer node");
    let full = store.node_full(mixer_id).expect("full node");
    let derives = full.properties.get("derives").cloned().unwrap_or_default();
    assert!(
        derives.contains("Debug") && derives.contains("Clone"),
        "expected Debug,Clone in derives property; got {derives:?}"
    );
    let type_kind = full
        .properties
        .get("type_kind")
        .cloned()
        .unwrap_or_default();
    assert_eq!(type_kind, "struct");
}

#[test]
fn parse_error_surfaces_as_index_error() {
    let mut store = Store::new();
    let bad = "fn broken( {";
    let err = {
        let mut sink = StoreSink::new(&mut store);
        RustIndexer::new()
            .index_file(Path::new("src/bad.rs"), bad, &mut sink)
            .expect_err("invalid rust should error")
    };
    let msg = format!("{}", err);
    assert!(msg.contains("src/bad.rs"), "error should mention path: {msg}");
    assert_eq!(store.node_count(), 0, "no nodes should leak from parse failure");
}

#[test]
fn rust_indexer_metadata_is_correct() {
    let i = RustIndexer::new();
    assert_eq!(i.lang(), Lang::Rust);
    assert_eq!(i.extensions(), &["rs"]);
}
