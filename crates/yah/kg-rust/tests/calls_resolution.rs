//! Hand-picked Rust call sites with single, unambiguous resolutions.
//! Each fixture pins down one shape we *do* want a `Calls` edge for and
//! one or more shapes we explicitly skip (multi-segment paths, method
//! calls, calls into imports). The store silently drops dangling edges,
//! so absence of an edge is the contract for "we couldn't resolve."

use std::path::Path;
use kg::edge::EdgeKind;
use kg::ids::NodeId;
use kg::indexer::LanguageIndexer;
use rpc::Direction;
use kg_rust::RustIndexer;
use kg_store::{Store, StoreSink};

fn index(src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        RustIndexer::new()
            .index_file(Path::new("src/calls.rs"), src, &mut sink)
            .expect("valid Rust");
    }
    store
}

fn find_id(store: &Store, label: &str) -> NodeId {
    store
        .lookup("src/calls.rs", None)
        .into_iter()
        .find(|id| store.node_ref(*id).map(|n| n.label == label).unwrap_or(false))
        .unwrap_or_else(|| panic!("no node labelled {label}"))
}

fn calls_targets(store: &Store, from: NodeId) -> Vec<String> {
    store
        .neighbors(from, Direction::Out, Some(&[EdgeKind::Calls]))
        .into_iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect()
}

#[test]
fn free_fn_calls_other_free_fn_in_same_module() {
    let src = r#"
        fn helper() {}

        fn run() {
            helper();
        }
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.contains(&"helper".to_string()),
        "expected Calls(run → helper); got {targets:?}"
    );
}

#[test]
fn forward_call_resolves_after_definition_emitted() {
    // Caller appears before callee in source — the deferred-emit pass
    // should still find the target by the time edges flush.
    let src = r#"
        fn run() {
            helper();
        }

        fn helper() {}
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.contains(&"helper".to_string()),
        "expected forward Calls(run → helper); got {targets:?}"
    );
}

#[test]
fn impl_method_calling_free_fn_in_same_module() {
    let src = r#"
        struct Widget;

        fn helper() {}

        impl Widget {
            fn kick(&self) {
                helper();
            }
        }
    "#;
    let store = index(src);
    // `kick` is a Method; lookup-by-label will find it.
    let kick = find_id(&store, "kick");
    let helper = find_id(&store, "helper");
    let edges = store.neighbors(kick, Direction::Out, Some(&[EdgeKind::Calls]));
    assert!(
        edges.iter().any(|e| e.to == helper),
        "expected Calls(kick → helper); got {:?}",
        edges
            .iter()
            .map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn method_call_does_not_emit_calls_edge() {
    // `x.foo()` needs receiver-type inference we don't have. We must
    // not emit a Calls edge — the existence of a same-module `foo` fn
    // would let an over-eager resolver guess wrong.
    let src = r#"
        fn foo() {}

        fn run(x: &str) {
            x.len();
            x.foo();
        }
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        !targets.contains(&"foo".to_string()),
        "method call x.foo() should not be resolved to free fn foo; got {targets:?}"
    );
    assert!(
        targets.is_empty(),
        "no calls expected from run; got {targets:?}"
    );
}

#[test]
fn multi_segment_path_call_does_not_emit_edge() {
    // `Foo::new()` could be a same-module type's associated fn but
    // resolving that needs us to walk impl blocks for `new`. v1 skips
    // multi-segment paths rather than emit a guessed edge.
    let src = r#"
        struct Foo;

        impl Foo {
            fn new() -> Self { Foo }
        }

        fn run() {
            Foo::new();
        }
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.is_empty(),
        "Foo::new() is multi-segment; no Calls expected; got {targets:?}"
    );
}

#[test]
fn unresolved_call_into_import_drops_silently() {
    // No same-module `helper` fn exists — the path would be an import
    // reference the store can't see. The edge should be dropped, leaving
    // run with zero outgoing Calls edges.
    let src = r#"
        use crate::other::helper;

        fn run() {
            helper();
        }
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.is_empty(),
        "imported helper is not in-store; expected no Calls; got {targets:?}"
    );
}

#[test]
fn nested_module_call_does_not_leak_to_outer_fn() {
    // `inner::worker` calls `noop()`; that bare ident resolves *within
    // the inner module*, not to the outer-scope name. Because the outer
    // module has no `noop`, and the inner module has no `noop` either,
    // the edge is dropped — no false attribution to outer names.
    let src = r#"
        fn outer() {}

        mod inner {
            fn worker() {
                noop();
            }
        }
    "#;
    let store = index(src);
    let worker = find_id(&store, "worker");
    let targets = calls_targets(&store, worker);
    assert!(
        targets.is_empty(),
        "inner::worker should not resolve noop to anything; got {targets:?}"
    );
}

#[test]
fn calls_inside_match_arms_and_closures_resolve() {
    let src = r#"
        fn helper() {}
        fn other() {}

        fn run(x: i32) {
            let f = || helper();
            match x {
                0 => other(),
                _ => helper(),
            }
        }
    "#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.contains(&"helper".to_string()),
        "calls inside closure / match arm should resolve helper; got {targets:?}"
    );
    assert!(
        targets.contains(&"other".to_string()),
        "match arm call to other should resolve; got {targets:?}"
    );
}
