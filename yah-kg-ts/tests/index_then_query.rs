//! End-to-end TS/TSX indexer tests: parse a fixture string with
//! `TsIndexer`, push results into a `Store`, then exercise the queries
//! the daemon will expose under `arch.*`.

use std::path::Path;
use yah_kg::edge::EdgeKind;
use yah_kg::indexer::LanguageIndexer;
use yah_kg::kind::{CommonKind, Lang, NodeKind, TsKind};
use yah_kg::rpc::Direction;
use yah_kg_store::{Store, StoreSink};
use yah_kg_ts::TsIndexer;

const TS_SRC: &str = r#"// Domain types for the board.

export type ColumnKey = "open" | "active" | "handoff" | "review";

export interface Ticket {
  id: string;
  title: string;
  status: ColumnKey;
}

export interface Relay extends Ticket {
  children: Ticket[];
}

export enum Priority {
  Low,
  High,
}

export const FIXED_LIMIT = 100;

export function loadTickets(rigId: string): Ticket[] {
  return [];
}

export class TicketStore {
  private items: Ticket[] = [];

  add(t: Ticket): void {
    this.items.push(t);
  }

  count(): number {
    return this.items.length;
  }
}

export class FilteredStore extends TicketStore implements Iterable<Ticket> {
  [Symbol.iterator](): Iterator<Ticket> {
    return this.items[Symbol.iterator]();
  }
}

namespace internal {
  export const SECRET = 42;
  export function helper(): void {}
}
"#;

const TSX_SRC: &str = r#"import { useState } from "react";

interface BoardProps {
  tickets: Ticket[];
  onChange: (t: Ticket[]) => void;
}

export function Board({ tickets, onChange }: BoardProps) {
  const [filter, setFilter] = useState<string>("");
  return (
    <div className="board">
      <input value={filter} onChange={(e) => setFilter(e.target.value)} />
      <Column tickets={tickets} />
    </div>
  );
}

export const Column = ({ tickets }: { tickets: Ticket[] }) => {
  return <div>{tickets.length}</div>;
};

function lowercaseHelper(): number {
  return 1;
}

export default function Page() {
  return <Board tickets={[]} onChange={() => {}} />;
}
"#;

fn build_store_for(path: &str, src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        TsIndexer::new()
            .index_file(Path::new(path), src, &mut sink)
            .expect("indexer should accept the fixture");
    }
    store
}

#[test]
fn ts_emits_interface_type_alias_enum_function_class() {
    let store = build_store_for("src/types.ts", TS_SRC);

    let all = store.lookup("src/types.ts", None);
    let by_label: Vec<(String, NodeKind)> = all
        .iter()
        .filter_map(|id| {
            let n = store.node_ref(*id)?;
            Some((n.label.clone(), n.kind.clone()))
        })
        .collect();

    let names: std::collections::HashSet<&str> =
        by_label.iter().map(|(l, _)| l.as_str()).collect();
    for required in [
        "ColumnKey",
        "Ticket",
        "Relay",
        "Priority",
        "Low",
        "High",
        "FIXED_LIMIT",
        "loadTickets",
        "TicketStore",
        "FilteredStore",
        "internal",
        "SECRET",
        "helper",
        "add",
        "count",
    ] {
        assert!(
            names.contains(required),
            "missing {required}; saw {:?}",
            names
        );
    }

    // Kind-level checks
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Ticket" && matches!(k, NodeKind::Ts(TsKind::Interface)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "ColumnKey" && matches!(k, NodeKind::Ts(TsKind::TypeAlias)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Priority" && matches!(k, NodeKind::Ts(TsKind::Enum)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "TicketStore" && matches!(k, NodeKind::Common(CommonKind::Type)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Low" && matches!(k, NodeKind::Common(CommonKind::Variant)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "loadTickets" && matches!(k, NodeKind::Common(CommonKind::Function)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "FIXED_LIMIT" && matches!(k, NodeKind::Common(CommonKind::Constant)))
    );
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "internal" && matches!(k, NodeKind::Common(CommonKind::Module)))
    );
}

#[test]
fn ts_extends_edge_resolves_within_file() {
    let store = build_store_for("src/types.ts", TS_SRC);

    let all = store.lookup("src/types.ts", None);
    let relay_id = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "Relay")
                .unwrap_or(false)
        })
        .expect("Relay interface");
    let out = store.neighbors(relay_id, Direction::Out, Some(&[EdgeKind::Extends]));
    assert!(
        !out.is_empty(),
        "Relay should have an Extends edge to Ticket"
    );
    assert_eq!(
        store.node_ref(out[0].to).map(|n| n.label.as_str()),
        Some("Ticket")
    );

    // Class extends class
    let filtered_id = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "FilteredStore")
                .unwrap_or(false)
        })
        .expect("FilteredStore class");
    let out = store.neighbors(filtered_id, Direction::Out, Some(&[EdgeKind::Extends]));
    assert!(out.iter().any(|e| store
        .node_ref(e.to)
        .map(|n| n.label == "TicketStore")
        .unwrap_or(false)));
}

#[test]
fn ts_class_methods_emit_defines_edges() {
    let store = build_store_for("src/types.ts", TS_SRC);
    let all = store.lookup("src/types.ts", None);
    let store_id = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "TicketStore")
                .unwrap_or(false)
        })
        .expect("TicketStore");
    let defines = store.neighbors(store_id, Direction::Out, Some(&[EdgeKind::Defines]));
    let method_labels: Vec<String> = defines
        .iter()
        .filter_map(|e| store.node_ref(e.to).map(|n| n.label.clone()))
        .collect();
    assert!(
        method_labels.contains(&"add".to_string())
            && method_labels.contains(&"count".to_string()),
        "expected add+count defined; got {method_labels:?}"
    );
}

#[test]
fn tsx_capitalized_function_becomes_jsx_component() {
    let store = build_store_for("src/Board.tsx", TSX_SRC);
    let all = store.lookup("src/Board.tsx", None);

    let by_label: Vec<(String, NodeKind)> = all
        .iter()
        .filter_map(|id| {
            let n = store.node_ref(*id)?;
            Some((n.label.clone(), n.kind.clone()))
        })
        .collect();

    // Component (named function declaration)
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Board" && matches!(k, NodeKind::Ts(TsKind::JsxComponent))),
        "Board should be a JsxComponent; got {by_label:?}"
    );
    // Component via const arrow function
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Column" && matches!(k, NodeKind::Ts(TsKind::JsxComponent))),
        "Column (const arrow) should be JsxComponent; got {by_label:?}"
    );
    // Lowercase function should NOT be a component
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "lowercaseHelper" && matches!(k, NodeKind::Common(CommonKind::Function))),
        "lowercaseHelper should be a plain Function; got {by_label:?}"
    );
    // Default-exported component
    assert!(
        by_label
            .iter()
            .any(|(l, k)| l == "Page" && matches!(k, NodeKind::Ts(TsKind::JsxComponent))),
        "Page should be a JsxComponent; got {by_label:?}"
    );
    // Page should be marked default_export.
    let page_id = all
        .iter()
        .copied()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| n.label == "Page")
                .unwrap_or(false)
        })
        .unwrap();
    let full = store.node_full(page_id).unwrap();
    assert_eq!(
        full.properties.get("default_export").map(|s| s.as_str()),
        Some("true")
    );
}

#[test]
fn tsx_file_marked_with_tsx_property() {
    let store = build_store_for("src/Board.tsx", TSX_SRC);
    // The file node carries a `tsx = "true"` property so consumers can
    // distinguish .tsx from plain .ts without re-parsing the path.
    let file_id = store
        .lookup("src/Board.tsx", None)
        .into_iter()
        .find(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                .unwrap_or(false)
        })
        .unwrap();
    let full = store.node_full(file_id).unwrap();
    assert_eq!(
        full.properties.get("tsx").map(|s| s.as_str()),
        Some("true")
    );
}

#[test]
fn ts_indexer_metadata_is_correct() {
    let i = TsIndexer::new();
    assert_eq!(i.lang(), Lang::Ts);
    assert_eq!(i.extensions(), &["ts", "tsx"]);
}

#[test]
fn ts_lookup_returns_innermost_first() {
    let store = build_store_for("src/types.ts", TS_SRC);
    // Line 30 is inside the `add` method body of TicketStore (the
    // `this.items.push(t);` statement).
    let hits = store.lookup("src/types.ts", Some(30));
    let first_label = hits
        .first()
        .and_then(|id| store.node_ref(*id).map(|n| n.label.clone()));
    // Innermost should be the method, not the class or file.
    assert_eq!(
        first_label.as_deref(),
        Some("add"),
        "innermost-first should surface `add`; got {hits:?}"
    );
}
