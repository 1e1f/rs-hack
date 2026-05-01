//! Pass 3 cross-file `Imports` resolver: lay out a tiny multi-file crate,
//! drive `walk_and_index`, then verify `crate::`/`super::`/`self::`
//! shapes resolve to the right target files.

use std::fs;
use tempfile::tempdir;
use kg::edge::EdgeKind;
use kg::ids::NodeFull;
use kg::kind::{CommonKind, NodeKind};
use rpc::Direction;
use kg_rust::RustIndexer;
use kg_store::{walk_and_index, IndexerRegistry, Store};

fn registry() -> IndexerRegistry {
    let mut reg = IndexerRegistry::new();
    reg.register(Box::new(RustIndexer::new()));
    reg
}

fn file_node(store: &Store, rel: &str) -> NodeFull {
    let id = store
        .lookup(rel, None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("file node missing: {rel}"));
    store.node_full(id).expect("full")
}

fn imports_targets(store: &Store, from_rel: &str) -> Vec<String> {
    let from = file_node(store, from_rel).node.id;
    store
        .neighbors(from, Direction::Out, Some(&[EdgeKind::Imports]))
        .into_iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.file.clone()))
        .collect()
}

#[test]
fn collects_top_level_imports_as_property() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        r#"
use crate::foo::Bar;
use crate::bar::{Quux, baz::Qux};
use std::collections::HashMap;

pub fn root() {}
"#,
    )
    .unwrap();
    fs::write(src.join("foo.rs"), "pub struct Bar;\n").unwrap();
    fs::write(
        src.join("bar.rs"),
        "pub struct Quux;\npub mod baz { pub struct Qux; }\n",
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let lib = file_node(&store, "src/lib.rs");
    let imports = lib
        .properties
        .get("imports")
        .cloned()
        .expect("imports property");
    let lines: Vec<&str> = imports.split('\n').collect();
    assert!(
        lines.contains(&"crate::foo::Bar"),
        "missing crate::foo::Bar: {imports}"
    );
    assert!(
        lines.contains(&"crate::bar::Quux"),
        "missing crate::bar::Quux: {imports}"
    );
    assert!(
        lines.contains(&"crate::bar::baz::Qux"),
        "missing nested grouped path: {imports}"
    );
    assert!(
        lines.contains(&"std::collections::HashMap"),
        "external paths still collected so external-dep edges remain a future option: {imports}"
    );
}

#[test]
fn resolves_crate_paths_to_target_file() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        "use crate::foo::Bar;\nuse crate::bar::Quux;\n",
    )
    .unwrap();
    fs::write(src.join("foo.rs"), "pub struct Bar;\n").unwrap();
    fs::write(src.join("bar.rs"), "pub struct Quux;\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let mut targets = imports_targets(&store, "src/lib.rs");
    targets.sort();
    assert_eq!(targets, vec!["src/bar.rs".to_string(), "src/foo.rs".into()]);
}

#[test]
fn super_walks_up_one_module() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("foo")).unwrap();
    fs::write(src.join("lib.rs"), "pub mod foo;\n").unwrap();
    fs::write(
        src.join("foo").join("mod.rs"),
        "pub mod bar;\npub struct Sibling;\n",
    )
    .unwrap();
    fs::write(
        src.join("foo").join("bar.rs"),
        "use super::Sibling;\nuse super::super::Other;\n",
    )
    .unwrap();
    fs::write(src.join("Other.rs"), "// stub for super::super resolution\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let mut targets = imports_targets(&store, "src/foo/bar.rs");
    targets.sort();
    // `super::Sibling` should land on src/foo/mod.rs (the parent module file).
    // `super::super::Other` should resolve to src/lib.rs (crate root) — `Other`
    // would be an item there, so the resolver backs off to the containing
    // module file.
    assert!(
        targets.contains(&"src/foo/mod.rs".to_string()),
        "expected super:: → foo/mod.rs in {targets:?}"
    );
    assert!(
        targets.contains(&"src/lib.rs".to_string()),
        "expected super::super:: → lib.rs in {targets:?}"
    );
}

#[test]
fn self_resolves_to_current_module_subpath() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("foo")).unwrap();
    fs::write(src.join("lib.rs"), "pub mod foo;\n").unwrap();
    fs::write(
        src.join("foo").join("mod.rs"),
        "pub mod bar;\nuse self::bar::Inner;\n",
    )
    .unwrap();
    fs::write(src.join("foo").join("bar.rs"), "pub struct Inner;\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "src/foo/mod.rs");
    assert!(
        targets.contains(&"src/foo/bar.rs".to_string()),
        "expected self::bar → foo/bar.rs in {targets:?}"
    );
}

#[test]
fn external_paths_skipped() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        "use std::collections::HashMap;\nuse serde::Serialize;\n",
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "src/lib.rs");
    assert!(
        targets.is_empty(),
        "no in-store target for external paths: {targets:?}"
    );
}

#[test]
fn glob_paths_resolve_to_module_file() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("lib.rs"), "use crate::foo::*;\n").unwrap();
    fs::write(src.join("foo.rs"), "pub struct Bar;\npub struct Baz;\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "src/lib.rs");
    assert_eq!(
        targets,
        vec!["src/foo.rs".to_string()],
        "glob should resolve to foo.rs: {targets:?}"
    );
}
