//! Pass 3 cross-file `Imports` resolver for TS/TSX. Lay out a tiny
//! frontend, drive `walk_and_index`, then check that:
//!
//! * relative specifiers (`./Foo`, `../bar`) land on the right file,
//! * tsconfig `paths` substitutions resolve into the rig (`@/lib/x`),
//! * bare specifiers without a paths mapping (`react`) are dropped, and
//! * `index.ts` fallback works for directory specifiers.

use std::fs;
use tempfile::tempdir;
use yah_kg::edge::EdgeKind;
use yah_kg::ids::NodeFull;
use yah_kg::kind::{CommonKind, NodeKind};
use yah_kg::rpc::Direction;
use yah_kg_store::{walk_and_index, IndexerRegistry, Store};
use yah_kg_ts::TsIndexer;

fn registry() -> IndexerRegistry {
    let mut reg = IndexerRegistry::new();
    reg.register(Box::new(TsIndexer::new()));
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
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("a.ts"),
        r#"import { B } from "./b";
import * as React from "react";
export { Q } from "./q";

export const x = 1;
"#,
    )
    .unwrap();
    fs::write(src.join("b.ts"), "export const B = 1;\n").unwrap();
    fs::write(src.join("q.ts"), "export const Q = 2;\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let a = file_node(&store, "src/a.ts");
    let imports = a
        .properties
        .get("imports")
        .cloned()
        .expect("imports property");
    let lines: Vec<&str> = imports.split('\n').collect();
    assert!(lines.contains(&"./b"), "missing ./b: {imports}");
    assert!(
        lines.contains(&"react"),
        "bare specifier still recorded so external-edges remain a future option: {imports}"
    );
    assert!(
        lines.contains(&"./q"),
        "re-export source treated as an import: {imports}"
    );
}

#[test]
fn relative_specifier_resolves_against_importer_directory() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(
        src.join("index.ts"),
        r#"import { Inner } from "./nested/inner";
export const top = 1;
"#,
    )
    .unwrap();
    fs::write(
        src.join("nested").join("inner.ts"),
        r#"import { Up } from "../up";
export const Inner = 1;
"#,
    )
    .unwrap();
    fs::write(src.join("up.ts"), "export const Up = 1;\n").unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let mut top_targets = imports_targets(&store, "src/index.ts");
    top_targets.sort();
    assert_eq!(top_targets, vec!["src/nested/inner.ts".to_string()]);

    let mut inner_targets = imports_targets(&store, "src/nested/inner.ts");
    inner_targets.sort();
    assert_eq!(
        inner_targets,
        vec!["src/up.ts".to_string()],
        "../up should walk up one dir"
    );
}

#[test]
fn relative_specifier_with_tsx_and_index_fallback() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("widgets")).unwrap();
    fs::write(
        src.join("App.tsx"),
        r#"import { Button } from "./widgets";
import { Sidebar } from "./Sidebar";
export const App = 1;
"#,
    )
    .unwrap();
    fs::write(src.join("Sidebar.tsx"), "export const Sidebar = 1;\n").unwrap();
    fs::write(
        src.join("widgets").join("index.ts"),
        "export const Button = 1;\n",
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let mut targets = imports_targets(&store, "src/App.tsx");
    targets.sort();
    assert_eq!(
        targets,
        vec![
            "src/Sidebar.tsx".to_string(),
            "src/widgets/index.ts".to_string(),
        ]
    );
}

#[test]
fn tsconfig_paths_alias_resolves_into_rig() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("lib")).unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  }
}
"#,
    )
    .unwrap();
    fs::write(
        src.join("App.tsx"),
        r#"import { logger } from "@/lib/logger";
export const App = 1;
"#,
    )
    .unwrap();
    fs::write(
        src.join("lib").join("logger.ts"),
        "export const logger = {};\n",
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "src/App.tsx");
    assert_eq!(
        targets,
        vec!["src/lib/logger.ts".to_string()],
        "@/lib/logger should map to src/lib/logger.ts"
    );
}

#[test]
fn bare_specifier_without_paths_mapping_is_dropped() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "baseUrl": "." } }"#,
    )
    .unwrap();
    fs::write(
        src.join("App.tsx"),
        r#"import { useState } from "react";
import { foo } from "@scope/lib";
export const App = useState;
"#,
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "src/App.tsx");
    assert!(
        targets.is_empty(),
        "external bare specifiers should not produce in-store edges: {targets:?}"
    );
}

#[test]
fn nested_tsconfig_wins_over_outer_for_paths() {
    // Two tsconfigs: outer at rig root maps `@/*` → `outer/*`, inner under
    // `app/` maps `@/*` → `app/src/*`. A file in `app/src/` should pick the
    // inner mapping (longest prefix).
    let dir = tempdir().unwrap();
    let outer_src = dir.path().join("outer");
    let app_src = dir.path().join("app").join("src");
    fs::create_dir_all(&outer_src).unwrap();
    fs::create_dir_all(&app_src).unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@/*": ["outer/*"] }
  }
}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("app").join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@/*": ["src/*"] }
  }
}"#,
    )
    .unwrap();
    fs::write(outer_src.join("util.ts"), "export const x = 1;\n").unwrap();
    fs::write(app_src.join("util.ts"), "export const x = 2;\n").unwrap();
    fs::write(
        app_src.join("App.tsx"),
        r#"import { x } from "@/util";
export const App = x;
"#,
    )
    .unwrap();

    let mut store = Store::new();
    walk_and_index(dir.path(), &mut store, &registry()).unwrap();

    let targets = imports_targets(&store, "app/src/App.tsx");
    assert_eq!(
        targets,
        vec!["app/src/util.ts".to_string()],
        "inner tsconfig should win for files under its dir"
    );
}
