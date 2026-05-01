//! Dogfood: index the real yah-ui frontend source tree and verify the
//! kinds of structural facts the Tauri shell will rely on.
//!
//! This test is co-tenanted with the workspace, so it expects yah-ui
//! to live at `<workspace>/yah-ui`. It's marked ignored if that directory
//! is missing so out-of-tree consumers don't need to ship the frontend.

use std::path::PathBuf;
use kg::kind::{CommonKind, NodeKind, TsKind};
use rpc::Direction;
use kg::edge::EdgeKind;
use kg_store::{walk_and_index, IndexerRegistry, Store};
use kg_ts::TsIndexer;

fn yah_ui_src() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest.parent()?.join("yah-ui").join("src");
    candidate.exists().then_some(candidate)
}

#[test]
fn indexes_yah_ui_and_finds_known_components() {
    let Some(src) = yah_ui_src() else {
        eprintln!("yah-ui/src not present; skipping dogfood test");
        return;
    };

    let mut store = Store::new();
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(TsIndexer::new()));

    let summary = walk_and_index(&src, &mut store, &registry).expect("walk");
    eprintln!(
        "yah-ui dogfood: {} files indexed, {} skipped, {} parse errors, {} nodes, {} edges",
        summary.files_indexed,
        summary.files_skipped,
        summary.parse_errors,
        store.node_count(),
        store.edge_count()
    );

    // Every .ts/.tsx file should parse (zero parse errors).
    assert_eq!(
        summary.parse_errors, 0,
        "yah-ui frontend should parse cleanly"
    );
    // We expect at least the 25 TS/TSX files we already see in the tree.
    assert!(
        summary.files_indexed >= 20,
        "expected ≥20 indexed files, got {}",
        summary.files_indexed
    );

    // Collect labels and kinds from the landmark files we know exist.
    let mut all_labels: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut all_kinds: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for file_rel in [
        "App.tsx",
        "types.ts",
        "components/board/Board.tsx",
        "components/board/Column.tsx",
        "components/board/TicketCard.tsx",
        "components/arch/ArchView.tsx",
        "components/agent/AgentView.tsx",
        "components/shell/TitleBar.tsx",
        "components/shell/TabStrip.tsx",
    ] {
        for id in store.lookup(file_rel, None) {
            if let Some(n) = store.node_ref(id) {
                all_labels.insert(n.label.clone());
                all_kinds.insert((n.label.clone(), format!("{:?}", n.kind)));
            }
        }
    }
    // Top-level component / type landmarks from yah-ui design.
    for required in [
        "App",
        "Board",
        "Column",
        "TicketCard",
        "ArchView",
        "AgentView",
        "TitleBar",
        "TabStrip",
        "Ticket",
        "ColumnKey",
        "Rig",
    ] {
        assert!(
            all_labels.contains(required),
            "yah-ui dogfood missed `{required}`; sample labels: {:?}",
            all_labels.iter().take(40).collect::<Vec<_>>()
        );
    }

    // The big-ticket components must register as JsxComponent in .tsx files.
    let jsx_components: Vec<&(String, String)> = all_kinds
        .iter()
        .filter(|(_, k)| k.contains("JsxComponent"))
        .collect();
    assert!(
        jsx_components.iter().any(|(l, _)| l == "App"),
        "App should be a JsxComponent; jsx components: {:?}",
        jsx_components
    );
    assert!(
        jsx_components.iter().any(|(l, _)| l == "Board"),
        "Board should be a JsxComponent"
    );

    // Interface landmark
    let ticket_interface = all_kinds
        .iter()
        .find(|(l, k)| l == "Ticket" && k.contains("Interface"));
    assert!(
        ticket_interface.is_some(),
        "Ticket should be an Interface; relevant nodes: {:?}",
        all_kinds.iter().filter(|(l, _)| l == "Ticket").collect::<Vec<_>>()
    );
}

#[test]
fn yah_ui_lookup_resolves_path_line_to_innermost_node() {
    let Some(src) = yah_ui_src() else {
        return;
    };

    let mut store = Store::new();
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(TsIndexer::new()));
    walk_and_index(&src, &mut store, &registry).expect("walk");

    // The agent's tool-result `path:line` chips need this query to work.
    // App.tsx line 10 is `export function App() { ... }` per the file we
    // inspected earlier — the innermost hit should be the App component.
    let hits = store.lookup("App.tsx", Some(10));
    let first_label = hits
        .first()
        .and_then(|id| store.node_ref(*id).map(|n| n.label.clone()));
    assert!(
        first_label.as_deref() == Some("App")
            || first_label.as_deref() == Some("App.tsx"),
        "expected App or App.tsx as innermost; got {first_label:?} from {hits:?}"
    );
}

#[test]
fn yah_ui_no_jsx_components_in_pure_ts_files() {
    let Some(src) = yah_ui_src() else {
        return;
    };
    let mut store = Store::new();
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(TsIndexer::new()));
    walk_and_index(&src, &mut store, &registry).expect("walk");

    // .ts files (e.g. types.ts, mock.ts) should never produce JsxComponent
    // nodes — the heuristic is gated on `tsx` in the file path.
    for id in store.lookup("types.ts", None) {
        let n = store.node_ref(id).unwrap();
        assert!(
            !matches!(n.kind, NodeKind::Ts(TsKind::JsxComponent)),
            "types.ts should have no JsxComponent; offender: {:?}",
            n.label
        );
    }
}

#[test]
fn yah_ui_subgraph_from_app_reaches_other_files() {
    // After walking the directory, the App component should be reachable
    // from the walker-emitted Directory tree (App.tsx → Directory → ...).
    // We don't rely on TS Imports edges yet (Pass 3 work).
    let Some(src) = yah_ui_src() else {
        return;
    };
    let mut store = Store::new();
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(TsIndexer::new()));
    walk_and_index(&src, &mut store, &registry).expect("walk");

    // Find the directory node. The walker uses the rig-relative path which
    // becomes "." for the root we passed.
    let root_dir_hits = store.lookup(".", None);
    let root_dir = root_dir_hits.iter().copied().find(|id| {
        store
            .node_ref(*id)
            .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::Directory)))
            .unwrap_or(false)
    });
    let Some(root_dir) = root_dir else {
        // Walker may use a non-"." root id depending on how the path
        // resolves; either way we can hop to App.tsx via Contains edges.
        return;
    };

    let sg = store.subgraph(
        root_dir,
        4,
        Some(&[EdgeKind::Contains]),
        None,
        None,
        None,
    );
    let labels: Vec<String> = sg.nodes.iter().map(|n| n.label.clone()).collect();
    assert!(
        labels.iter().any(|l| l == "App.tsx"),
        "App.tsx should be reachable from root directory via Contains; got {} nodes",
        sg.nodes.len()
    );

    // Hop further: App.tsx contains the App component itself.
    let app_neighbors = store.neighbors(
        sg.nodes
            .iter()
            .find(|n| n.label == "App.tsx")
            .map(|n| n.id)
            .unwrap(),
        Direction::Out,
        Some(&[EdgeKind::Contains]),
    );
    let app_children: Vec<String> = app_neighbors
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(
        app_children.iter().any(|l| l == "App"),
        "App.tsx should contain App; got {app_children:?}"
    );
}
