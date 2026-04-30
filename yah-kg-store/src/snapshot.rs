//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Serializable snapshot of [`Store`] contents.
//!
//! The store itself is not `Serialize` — it owns a `petgraph::StableDiGraph`
//! whose internal `NodeIndex` values are not portable across rebuilds. We
//! instead serialize the user-facing slices (nodes, edges, docs, properties)
//! and rebuild `graph` / `node_index` / `by_file` from them on load by
//! replaying through the public `upsert_*` API. Result: a snapshot is
//! lossless for query-relevant state but is not bit-identical to the
//! original instance (graph internal indices may differ).

use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use yah_kg::edge::EdgeOut;
use yah_kg::ids::{NodeId, NodeRef};

/// Snapshot format version. Bump on incompatible changes; `from_snapshot`
/// rejects mismatched versions so callers fall back to a full reindex.
pub const STORE_SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreSnapshot {
    pub version: u32,
    pub nodes: Vec<NodeRef>,
    pub edges: Vec<EdgeOut>,
    pub docs: Vec<(NodeId, String)>,
    pub properties: Vec<(NodeId, BTreeMap<String, String>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot version mismatch: file {file}, expected {expected}")]
    VersionMismatch { file: u32, expected: u32 },
}

impl Store {
    /// Capture the store's current contents as a `StoreSnapshot`. Read-only.
    pub fn to_snapshot(&self) -> StoreSnapshot {
        let mut nodes: Vec<NodeRef> = self.nodes_iter().cloned().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut edges: Vec<EdgeOut> = self.edges_iter().cloned().collect();
        edges.sort_by(|a, b| a.id.cmp(&b.id));
        let mut docs: Vec<(NodeId, String)> = self.docs_iter().map(|(k, v)| (*k, v.clone())).collect();
        docs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut properties: Vec<(NodeId, BTreeMap<String, String>)> = self
            .properties_iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        properties.sort_by(|a, b| a.0.cmp(&b.0));
        StoreSnapshot {
            version: STORE_SNAPSHOT_VERSION,
            nodes,
            edges,
            docs,
            properties,
        }
    }

    /// Replace the store's contents with `snap`. Rejects unknown versions.
    ///
    /// Bypasses the `upsert_node` / `upsert_edge` round-trip used during
    /// live indexing: the snapshot's node list is unique by `NodeId` (it
    /// came from a `HashMap` on the way out), so we pre-allocate every
    /// side map and write straight into them. Cuts per-node petgraph +
    /// HashMap rehash overhead — see R017-T7's verify-gate work.
    pub fn restore(&mut self, snap: StoreSnapshot) -> Result<(), SnapshotError> {
        if snap.version != STORE_SNAPSHOT_VERSION {
            return Err(SnapshotError::VersionMismatch {
                file: snap.version,
                expected: STORE_SNAPSHOT_VERSION,
            });
        }
        self.rebuild_from_parts(snap.nodes, snap.edges, snap.docs, snap.properties);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yah_kg::edge::{EdgeId, EdgeKind};
    use yah_kg::ids::Span;
    use yah_kg::kind::{CommonKind, Lang, NodeKind};

    fn mk_node(name: &str, file: &str, line: u32) -> NodeRef {
        let qualified = format!("{}::{}", file, name);
        NodeRef {
            id: NodeId::compute(Lang::Rust, &qualified, file),
            lang: Lang::Rust,
            kind: NodeKind::Common(CommonKind::Function),
            label: name.to_string(),
            qualified,
            file: file.to_string(),
            span: Span {
                start_line: line,
                start_col: 1,
                end_line: line + 5,
                end_col: 1,
            },
            synthetic: false,
        }
    }

    #[test]
    fn snapshot_round_trips_nodes_edges_docs_properties() {
        let mut a = Store::new();
        let n1 = mk_node("alpha", "src/lib.rs", 1);
        let n2 = mk_node("beta", "src/lib.rs", 10);
        let id1 = n1.id;
        let id2 = n2.id;
        a.upsert_node(n1);
        a.upsert_node(n2);
        let edge = EdgeOut {
            id: EdgeId::compute(id1, id2, &EdgeKind::Calls),
            from: id1,
            to: id2,
            kind: EdgeKind::Calls,
            annotations: vec![],
        };
        a.upsert_edge(edge);
        a.set_doc(id1, "doc-for-alpha".into());
        a.set_property(id1, "purity".into(), "pure".into());

        let snap = a.to_snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        let parsed: StoreSnapshot = serde_json::from_str(&json).unwrap();

        let mut b = Store::new();
        b.restore(parsed).unwrap();

        assert_eq!(b.node_count(), 2);
        assert_eq!(b.edge_count(), 1);
        let full = b.node_full(id1).unwrap();
        assert_eq!(full.doc.as_deref(), Some("doc-for-alpha"));
        assert_eq!(full.properties.get("purity").map(String::as_str), Some("pure"));

        // file index rebuilt: lookup still works
        let hits = b.lookup("src/lib.rs", Some(2));
        assert!(hits.contains(&id1));
    }

    #[test]
    fn restore_rejects_unknown_version() {
        let mut s = Store::new();
        let bad = StoreSnapshot {
            version: 999,
            nodes: vec![],
            edges: vec![],
            docs: vec![],
            properties: vec![],
        };
        assert!(matches!(
            s.restore(bad),
            Err(SnapshotError::VersionMismatch { .. })
        ));
    }
}
