//! Hand-picked TypeScript call sites with single, unambiguous resolutions.
//! Mirror of the yah-kg-rust calls_resolution suite. v1 only resolves
//! bare-identifier callees against same-namespace nodes; method calls
//! and property-callee shapes (`obj.foo()`, `Foo.bar()`) are dropped.

use std::path::Path;
use kg::edge::EdgeKind;
use kg::ids::NodeId;
use kg::indexer::LanguageIndexer;
use rpc::Direction;
use kg_store::{Store, StoreSink};
use kg_ts::TsIndexer;

const FILE: &str = "src/calls.ts";

fn index(src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        TsIndexer::new()
            .index_file(Path::new(FILE), src, &mut sink)
            .expect("valid TypeScript");
    }
    store
}

fn find_id(store: &Store, label: &str) -> NodeId {
    store
        .lookup(FILE, None)
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
fn function_calls_other_top_level_function() {
    let src = r#"
function helper(): void {}

function run(): void {
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
    let src = r#"
function run(): void {
  helper();
}

function helper(): void {}
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
fn arrow_function_const_calls_top_level_function() {
    // const Foo = () => helper(); — Foo is a top-level node; its body
    // is the arrow's body.
    let src = r#"
function helper(): void {}

const run = (): void => {
  helper();
};
"#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.contains(&"helper".to_string()),
        "expected Calls(run → helper) from arrow body; got {targets:?}"
    );
}

#[test]
fn method_call_does_not_emit_calls_edge() {
    // `obj.foo()` — receiver-type unknown, must not resolve to a same-
    // file `foo`.
    let src = r#"
function foo(): void {}

function run(): void {
  const x = { foo: () => 1 };
  x.foo();
}
"#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        !targets.contains(&"foo".to_string()),
        "x.foo() should not resolve to free fn foo; got {targets:?}"
    );
}

#[test]
fn property_call_on_namespaced_callee_skipped() {
    // `Foo.bar()` is a member_expression callee — skipped.
    let src = r#"
function bar(): void {}

const Foo = { bar: () => 1 };

function run(): void {
  Foo.bar();
}
"#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        !targets.contains(&"bar".to_string()),
        "Foo.bar() should not resolve to free fn bar; got {targets:?}"
    );
}

#[test]
fn unresolved_call_into_import_drops_silently() {
    let src = r#"
import { helper } from "./other";

function run(): void {
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
fn class_method_calls_top_level_function() {
    let src = r#"
function helper(): void {}

class Widget {
  kick(): void {
    helper();
  }
}
"#;
    let store = index(src);
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
fn calls_inside_nested_blocks_resolve() {
    let src = r#"
function helper(): void {}
function other(): void {}

function run(x: number): void {
  if (x > 0) {
    helper();
  } else {
    other();
  }
  for (let i = 0; i < x; i++) {
    helper();
  }
}
"#;
    let store = index(src);
    let run = find_id(&store, "run");
    let targets = calls_targets(&store, run);
    assert!(
        targets.contains(&"helper".to_string()),
        "expected helper from inside if/for; got {targets:?}"
    );
    assert!(
        targets.contains(&"other".to_string()),
        "expected other from else branch; got {targets:?}"
    );
}
