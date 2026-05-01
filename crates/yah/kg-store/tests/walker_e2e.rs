//! End-to-end walker test: lay down a tiny rig on disk, drive
//! `walk_and_index` with the registered `RustIndexer`, then query the
//! store like the daemon will.

use std::fs;
use tempfile::tempdir;
use kg::edge::EdgeKind;
use kg::indexer::LanguageIndexer;
use kg::kind::{CommonKind, NodeKind};
use kg_rust::RustIndexer;
use kg_store::{walk_and_index, IndexerRegistry, Store};

#[test]
fn walks_directory_and_dispatches_to_rust_indexer() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        "pub struct Foo;\npub fn bar() {}\n",
    )
    .unwrap();
    fs::write(
        src.join("nested.rs"),
        "pub trait Mixer { fn mix(&self); }\n",
    )
    .unwrap();
    // A non-Rust file should be skipped.
    fs::write(src.join("README.md"), "not rust").unwrap();
    // A target/ directory should be skipped entirely.
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("noise.rs"), "fn ignored() {}\n").unwrap();

    let mut store = Store::new();
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(RustIndexer::new()));

    let summary = walk_and_index(dir.path(), &mut store, &registry).unwrap();

    assert_eq!(summary.files_indexed, 2, "lib.rs + nested.rs");
    assert_eq!(summary.files_skipped, 1, "README.md");
    assert_eq!(summary.parse_errors, 0);

    // Find the Foo struct via the file index, not by walking.
    let lib_hits = store.lookup("src/lib.rs", None);
    let labels: Vec<String> = lib_hits
        .iter()
        .filter_map(|id| store.node_ref(*id).map(|n| n.label.clone()))
        .collect();
    assert!(labels.iter().any(|l| l == "Foo"));
    assert!(labels.iter().any(|l| l == "bar"));

    // target/ entries shouldn't be in the store.
    let ignored = store.lookup("target/noise.rs", None);
    assert!(
        ignored.is_empty(),
        "target/ should be skipped: {ignored:?}"
    );

    // The walker should have emitted Contains edges from the src directory
    // node into both files.
    let dir_hits = store.lookup("src", None);
    let dir_id = dir_hits
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::Directory)))
                .unwrap_or(false)
        })
        .expect("src/ directory node");
    let out = store.neighbors(dir_id, rpc::Direction::Out, Some(&[EdgeKind::Contains]));
    let targets: Vec<String> = out
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(
        targets.contains(&"lib.rs".to_string())
            && targets.contains(&"nested.rs".to_string()),
        "expected lib.rs + nested.rs as children of src/, got {targets:?}"
    );
}

#[test]
fn registry_extension_lookup_is_case_insensitive() {
    let mut reg = IndexerRegistry::new();
    reg.register(Box::new(RustIndexer::new()));
    assert!(reg.for_extension("rs").is_some());
    assert!(reg.for_extension("RS").is_some());
    assert!(reg.for_extension("py").is_none());
    let langs = reg.languages();
    assert_eq!(langs, vec![RustIndexer::new().lang()]);
}
