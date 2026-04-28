//! End-to-end JSON/YAML indexer tests: parse a real-world fixture
//! string, push results into a `Store`, then exercise the queries the
//! daemon will expose under `arch.*`. Day-one fixtures mirror the
//! files this crate is meant to render in the architecture tab:
//! `package.json`, `tsconfig.json`, `tauri.conf.json`, plus a YAML
//! sample to cover anchor + alias detection.

use std::path::Path;
use yah_kg::edge::EdgeKind;
use yah_kg::indexer::LanguageIndexer;
use yah_kg::kind::{CommonKind, DocKind, Lang, NodeKind};
use yah_kg::rpc::Direction;
use yah_kg_json_yaml::{JsonIndexer, TomlIndexer, YamlIndexer};
use yah_kg_store::{Store, StoreSink};

fn build_json(path: &str, src: &str) -> Store {
    let mut store = Store::new();
    index_into(&mut store, path, src);
    store
}

fn index_into(store: &mut Store, path: &str, src: &str) {
    let mut sink = StoreSink::new(store);
    JsonIndexer::new()
        .index_file(Path::new(path), src, &mut sink)
        .expect("json indexer should accept the fixture");
}

fn build_yaml(path: &str, src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        YamlIndexer::new()
            .index_file(Path::new(path), src, &mut sink)
            .expect("yaml indexer should accept the fixture");
    }
    store
}

fn build_toml(path: &str, src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        TomlIndexer::new()
            .index_file(Path::new(path), src, &mut sink)
            .expect("toml indexer should accept the fixture");
    }
    store
}

fn find_by_label<'s>(store: &'s Store, file: &str, label: &str) -> Vec<&'s yah_kg::ids::NodeRef> {
    store
        .lookup(file, None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| n.label == label)
        .collect()
}

const PACKAGE_JSON: &str = r#"{
  "name": "yah-ui",
  "version": "0.7.0",
  "private": true,
  "scripts": {
    "dev": "bun run serve.ts",
    "build": "bun build src/index.ts"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  },
  "devDependencies": {
    "typescript": "^5.4.0"
  }
}
"#;

const TSCONFIG: &str = r#"{
  "$schema": "https://json.schemastore.org/tsconfig",
  "extends": "./tsconfig.base.json",
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true
  },
  "include": ["src/**/*.ts", "src/**/*.tsx"]
}
"#;

const TAURI_CONF: &str = r#"{
  "$schema": "../gen/schemas/desktop-schema.json",
  "productName": "yah",
  "version": "0.7.0",
  "identifier": "com.yah.app",
  "build": {
    "frontendDist": "../yah-ui/dist",
    "devUrl": "http://localhost:3000"
  },
  "app": {
    "windows": [
      { "title": "yah", "width": 1280, "height": 800 }
    ]
  }
}
"#;

#[test]
fn json_indexer_metadata() {
    let i = JsonIndexer::new();
    assert_eq!(i.lang(), Lang::Json);
    assert_eq!(i.extensions(), &["json"]);
}

#[test]
fn yaml_indexer_metadata() {
    let i = YamlIndexer::new();
    assert_eq!(i.lang(), Lang::Yaml);
    assert_eq!(i.extensions(), &["yaml", "yml"]);
}

#[test]
fn package_json_emits_file_document_and_nested_properties() {
    let store = build_json("yah-ui/package.json", PACKAGE_JSON);

    // File node.
    let files: Vec<_> = store
        .lookup("yah-ui/package.json", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
        .collect();
    assert_eq!(files.len(), 1, "expected exactly one File node");
    assert_eq!(files[0].label, "package.json");

    // Document node.
    let docs: Vec<_> = store
        .lookup("yah-ui/package.json", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::Document)))
        .collect();
    assert_eq!(docs.len(), 1, "expected exactly one Document node");

    // Top-level keys all present.
    for required in [
        "name",
        "version",
        "private",
        "scripts",
        "dependencies",
        "devDependencies",
    ] {
        let hits = find_by_label(&store, "yah-ui/package.json", required);
        assert!(
            hits.iter()
                .any(|n| matches!(n.kind, NodeKind::Doc(DocKind::Property))),
            "missing top-level Property node for {required}"
        );
    }

    // Nested key under dependencies should also be a Property.
    let react_hits = find_by_label(&store, "yah-ui/package.json", "react");
    assert!(
        react_hits
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Doc(DocKind::Property))),
        "missing nested Property node `react`"
    );
}

#[test]
fn package_json_scalar_values_are_recorded() {
    let store = build_json("yah-ui/package.json", PACKAGE_JSON);
    let name_id = store
        .lookup("yah-ui/package.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "name" && matches!(n.kind, NodeKind::Doc(DocKind::Property)))
                .unwrap_or(false)
        })
        .expect("name property");
    let full = store.node_full(name_id).unwrap();
    assert_eq!(full.properties.get("value").map(String::as_str), Some("yah-ui"));
    assert_eq!(
        full.properties.get("value_kind").map(String::as_str),
        Some("string")
    );

    // The dependencies key is an object — value_kind=object, no scalar value.
    let deps_id = store
        .lookup("yah-ui/package.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "dependencies"
                    && matches!(n.kind, NodeKind::Doc(DocKind::Property)))
                .unwrap_or(false)
        })
        .expect("dependencies property");
    let full = store.node_full(deps_id).unwrap();
    assert_eq!(
        full.properties.get("value_kind").map(String::as_str),
        Some("object")
    );
    assert!(full.properties.get("value").is_none());
}

#[test]
fn package_json_contains_edges_form_a_tree() {
    let store = build_json("yah-ui/package.json", PACKAGE_JSON);
    // Document → dependencies → react chain via Contains.
    let doc_id = store
        .lookup("yah-ui/package.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::Document)))
                .unwrap_or(false)
        })
        .unwrap();
    let doc_children = store.neighbors(doc_id, Direction::Out, Some(&[EdgeKind::Contains]));
    let child_labels: Vec<String> = doc_children
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(child_labels.contains(&"dependencies".to_string()));

    let deps_id = doc_children
        .iter()
        .find(|e| {
            store
                .node_ref(e.to)
                .map(|n| n.label == "dependencies")
                .unwrap_or(false)
        })
        .map(|e| e.to)
        .unwrap();
    let dep_children = store.neighbors(deps_id, Direction::Out, Some(&[EdgeKind::Contains]));
    let dep_labels: Vec<String> = dep_children
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(dep_labels.contains(&"react".to_string()));
    assert!(dep_labels.contains(&"react-dom".to_string()));
}

#[test]
fn tsconfig_extends_emits_schema_ref_and_refers_to_edge() {
    // Index the base config first so the cross-file Document target
    // exists in the store before the `extends` edge is emitted. This
    // mirrors the daemon's two-pass workspace walk.
    let mut store = Store::new();
    index_into(&mut store, "yah-ui/tsconfig.base.json", "{ \"compilerOptions\": {} }");
    index_into(&mut store, "yah-ui/tsconfig.json", TSCONFIG);
    let extends_id = store
        .lookup("yah-ui/tsconfig.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "extends" && matches!(n.kind, NodeKind::Doc(DocKind::SchemaRef)))
                .unwrap_or(false)
        })
        .expect("extends should be a SchemaRef node");

    let full = store.node_full(extends_id).unwrap();
    assert_eq!(
        full.properties.get("target").map(String::as_str),
        Some("./tsconfig.base.json")
    );
    assert_eq!(
        full.properties.get("ref_kind").map(String::as_str),
        Some("extends")
    );

    // RefersTo edge points at a Document-shaped id for the resolved file.
    let refs = store.neighbors(extends_id, Direction::Out, Some(&[EdgeKind::RefersTo]));
    assert_eq!(refs.len(), 1, "extends should emit one RefersTo edge");
    let target_qualified = format!("{}#", "yah-ui/tsconfig.base.json");
    let expected =
        yah_kg::ids::NodeId::compute(Lang::Json, &target_qualified, "yah-ui/tsconfig.base.json");
    assert_eq!(refs[0].to, expected);
}

#[test]
fn tsconfig_schema_emits_conforms_to_edge() {
    // The schema lives at an absolute https URL — the walker still
    // produces a stub-target id so the edge has somewhere to land
    // once a future fetcher emits a Document for it. To make the
    // assertion concrete in a unit test, we hand-emit the target
    // node via a sibling indexer call.
    let mut store = Store::new();
    // Pre-seed the schema document by indexing a synthetic file at
    // the resolved path. (The walker uses `Lang::Json` for any
    // non-yaml suffix, so a `.json` stub matches.)
    index_into(
        &mut store,
        "https://json.schemastore.org/tsconfig",
        "{}",
    );
    index_into(&mut store, "yah-ui/tsconfig.json", TSCONFIG);
    let schema_id = store
        .lookup("yah-ui/tsconfig.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "$schema" && matches!(n.kind, NodeKind::Doc(DocKind::SchemaRef)))
                .unwrap_or(false)
        })
        .expect("$schema should be a SchemaRef node");
    let conforms = store.neighbors(schema_id, Direction::Out, Some(&[EdgeKind::ConformsTo]));
    assert_eq!(
        conforms.len(),
        1,
        "$schema should emit exactly one ConformsTo edge"
    );
}

#[test]
fn tauri_conf_relative_schema_resolves_via_dotdot() {
    let mut store = Store::new();
    index_into(&mut store, "app/gen/schemas/desktop-schema.json", "{}");
    index_into(&mut store, "app/tauri/tauri.conf.json", TAURI_CONF);
    let schema_id = store
        .lookup("app/tauri/tauri.conf.json", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "$schema")
                .unwrap_or(false)
        })
        .expect("$schema present");
    let conforms = store.neighbors(schema_id, Direction::Out, Some(&[EdgeKind::ConformsTo]));
    let target_qualified = format!("{}#", "app/gen/schemas/desktop-schema.json");
    let expected = yah_kg::ids::NodeId::compute(
        Lang::Json,
        &target_qualified,
        "app/gen/schemas/desktop-schema.json",
    );
    assert!(
        conforms.iter().any(|e| e.to == expected),
        "ConformsTo edge should resolve `../gen/...` against `app/tauri/`; got {:?}",
        conforms
            .iter()
            .map(|e| store.node_ref(e.to).map(|n| n.qualified.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn tauri_conf_array_indexes_become_property_nodes() {
    let store = build_json("app/tauri/tauri.conf.json", TAURI_CONF);
    // app.windows[0].title should be reachable as a Property node labeled "title".
    let titles = find_by_label(&store, "app/tauri/tauri.conf.json", "title");
    assert!(
        !titles.is_empty(),
        "expected a `title` property nested under windows[0]"
    );
}

const YAML_SRC: &str = r#"defaults: &defaults
  timeout: 30
  retries: 3

dev:
  <<: *defaults
  host: localhost

prod:
  <<: *defaults
  host: prod.example.com
"#;

#[test]
fn yaml_emits_anchor_node_and_aliases_resolve() {
    let store = build_yaml("config/services.yaml", YAML_SRC);
    // Anchor node for `&defaults`.
    let anchor: Vec<_> = store
        .lookup("config/services.yaml", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| matches!(n.kind, NodeKind::Doc(DocKind::Anchor)))
        .collect();
    assert_eq!(anchor.len(), 1, "exactly one Anchor node expected");
    assert_eq!(anchor[0].label, "defaults");

    // Two aliases (`*defaults` used twice) → two SchemaRef nodes that
    // each have a RefersTo edge pointing at the Anchor.
    let alias_refs: Vec<_> = store
        .lookup("config/services.yaml", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id).map(|n| (id, n)))
        .filter(|(_, n)| matches!(n.kind, NodeKind::Doc(DocKind::SchemaRef)))
        .filter(|(_, n)| n.label == "*defaults")
        .collect();
    assert_eq!(alias_refs.len(), 2, "two `*defaults` alias uses expected");

    let anchor_id = store
        .lookup("config/services.yaml", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Doc(DocKind::Anchor)))
                .unwrap_or(false)
        })
        .unwrap();
    for (id, _) in &alias_refs {
        let refs = store.neighbors(*id, Direction::Out, Some(&[EdgeKind::RefersTo]));
        assert!(
            refs.iter().any(|e| e.to == anchor_id),
            "alias should RefersTo the anchor"
        );
    }
}

#[test]
fn yaml_property_nodes_include_merged_keys() {
    let store = build_yaml("config/services.yaml", YAML_SRC);
    // `serde_yaml` resolves `<<` merge keys, so `host` shows up under
    // both dev and prod, while `timeout` (from the anchor) is also
    // present under each. We just check `host` and `timeout` exist
    // somewhere in the tree.
    assert!(!find_by_label(&store, "config/services.yaml", "host").is_empty());
    assert!(!find_by_label(&store, "config/services.yaml", "timeout").is_empty());
}

#[test]
fn malformed_json_returns_parse_error() {
    let mut store = Store::new();
    let mut sink = StoreSink::new(&mut store);
    let err = JsonIndexer::new()
        .index_file(Path::new("bad.json"), "{ not valid", &mut sink)
        .unwrap_err();
    match err {
        yah_kg::indexer::IndexError::Parse { path, .. } => {
            assert_eq!(path, "bad.json");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

const CARGO_TOML: &str = r#"[package]
name = "yah-kg-json-yaml"
version = "0.7.0"
edition = "2021"

[dependencies]
yah-kg = { path = "../yah-kg" }
serde_json = "1.0"
serde_yaml = "0.9"
toml = "0.8"

[dev-dependencies]
yah-kg-store = { path = "../yah-kg-store" }
"#;

#[test]
fn toml_indexer_metadata() {
    let i = TomlIndexer::new();
    assert_eq!(i.lang(), Lang::Toml);
    assert_eq!(i.extensions(), &["toml"]);
}

#[test]
fn cargo_toml_emits_file_document_and_table_properties() {
    let store = build_toml("yah-kg-json-yaml/Cargo.toml", CARGO_TOML);

    let files: Vec<_> = store
        .lookup("yah-kg-json-yaml/Cargo.toml", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].label, "Cargo.toml");

    let docs: Vec<_> = store
        .lookup("yah-kg-json-yaml/Cargo.toml", None)
        .into_iter()
        .filter_map(|id| store.node_ref(id))
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::Document)))
        .collect();
    assert_eq!(docs.len(), 1);

    for required in ["package", "dependencies", "dev-dependencies"] {
        let hits = find_by_label(&store, "yah-kg-json-yaml/Cargo.toml", required);
        assert!(
            hits.iter()
                .any(|n| matches!(n.kind, NodeKind::Doc(DocKind::Property))),
            "missing top-level Property node for {required}"
        );
    }

    // Nested key under [dependencies] should also be a Property.
    let hits = find_by_label(&store, "yah-kg-json-yaml/Cargo.toml", "yah-kg");
    assert!(
        hits.iter()
            .any(|n| matches!(n.kind, NodeKind::Doc(DocKind::Property))),
        "missing nested Property node `yah-kg`"
    );
}

#[test]
fn cargo_toml_scalar_values_and_kinds_recorded() {
    let store = build_toml("yah-kg-json-yaml/Cargo.toml", CARGO_TOML);
    // package.name → "yah-kg-json-yaml" with value_kind=string.
    let name_id = store
        .lookup("yah-kg-json-yaml/Cargo.toml", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "name" && matches!(n.kind, NodeKind::Doc(DocKind::Property)))
                .unwrap_or(false)
        })
        .expect("name property");
    let full = store.node_full(name_id).unwrap();
    assert_eq!(
        full.properties.get("value").map(String::as_str),
        Some("yah-kg-json-yaml")
    );
    assert_eq!(
        full.properties.get("value_kind").map(String::as_str),
        Some("string")
    );

    // [dependencies] table is a value_kind=object Property node.
    let deps_id = store
        .lookup("yah-kg-json-yaml/Cargo.toml", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "dependencies"
                    && matches!(n.kind, NodeKind::Doc(DocKind::Property)))
                .unwrap_or(false)
        })
        .expect("dependencies property");
    let full = store.node_full(deps_id).unwrap();
    assert_eq!(
        full.properties.get("value_kind").map(String::as_str),
        Some("object")
    );
}

#[test]
fn json_property_nodes_carry_per_key_spans() {
    // tree-sitter-json gives us precise (line, col) per pair, so
    // Property nodes for top-level + nested keys land on the line
    // of their `key: value` pair — not the whole file. This is what
    // lets the architecture tab click into nested config keys.
    let store = build_json("yah-ui/package.json", PACKAGE_JSON);

    // Helper: find the unique Property node with this label.
    let prop_span = |label: &str| -> yah_kg::ids::Span {
        let id = store
            .lookup("yah-ui/package.json", None)
            .into_iter()
            .find(|id| {
                store
                    .node_ref(*id)
                    .map(|n| {
                        n.label == label
                            && matches!(n.kind, NodeKind::Doc(DocKind::Property))
                    })
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("missing property {label}"));
        store.node_ref(id).unwrap().span
    };

    // PACKAGE_JSON layout (1-indexed):
    //   1: {
    //   2:   "name": "yah-ui",
    //   3:   "version": "0.7.0",
    //   9:   "dependencies": {
    //  10:     "react": "^18.2.0",
    let name = prop_span("name");
    assert_eq!(name.start_line, 2, "`name` lives on line 2");
    assert_ne!(
        name.end_line,
        store
            .node_ref(
                store
                    .lookup("yah-ui/package.json", None)
                    .into_iter()
                    .find(|id| store
                        .node_ref(*id)
                        .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                        .unwrap_or(false))
                    .unwrap()
            )
            .unwrap()
            .span
            .end_line,
        "name span must NOT cover the whole file anymore"
    );

    let version = prop_span("version");
    assert_eq!(version.start_line, 3);

    // Nested key still gets a precise line.
    let react = prop_span("react");
    assert_eq!(react.start_line, 10, "nested `react` lives on line 10");
}

#[test]
fn yaml_and_toml_properties_still_use_file_wide_span_until_followup() {
    // Per-key span tracking is JSON-only in this pass — YAML and TOML
    // don't yet have a span source wired (yaml-rust2 events / toml_edit
    // are the planned follow-ups). This test pins the current behavior
    // so the follow-up PR notices when it flips.
    let store = build_yaml("config/services.yaml", YAML_SRC);
    let host_id = store
        .lookup("config/services.yaml", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| {
                    n.label == "host" && matches!(n.kind, NodeKind::Doc(DocKind::Property))
                })
                .unwrap_or(false)
        })
        .expect("at least one host property");
    let host_span = store.node_ref(host_id).unwrap().span;
    let file_id = store
        .lookup("config/services.yaml", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                .unwrap_or(false)
        })
        .unwrap();
    let file_span = store.node_ref(file_id).unwrap().span;
    assert_eq!(
        host_span, file_span,
        "YAML Property nodes still pin to the file-wide span until the yaml-rust2 events pass lands"
    );
}

#[test]
fn malformed_toml_returns_parse_error() {
    let mut store = Store::new();
    let mut sink = StoreSink::new(&mut store);
    let err = TomlIndexer::new()
        .index_file(Path::new("bad.toml"), "[unterminated", &mut sink)
        .unwrap_err();
    match err {
        yah_kg::indexer::IndexError::Parse { path, .. } => {
            assert_eq!(path, "bad.toml");
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}
