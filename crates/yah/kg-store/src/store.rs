//! @arch:layer(kg)
//! @arch:role(graph)
//!
//! In-memory `Store` backing the `arch.*` RPC surface.

use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction as PgDirection;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use kg::edge::{EdgeId, EdgeKind, EdgeOut};
use kg::ids::{NodeFull, NodeId, NodeRef, Span};
use kg::kind::{Lang, NodeKind};
use rpc::{Direction, Subgraph};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("node not found: {0}")]
    NodeNotFound(NodeId),
}

/// In-memory knowledge graph.
///
/// `Store` owns the petgraph and all side indices. Cloning is intentionally
/// not derived — the graph is mutated in place from a single owner (the
/// daemon) and queries borrow.
pub struct Store {
    graph: StableDiGraph<NodeId, EdgeId>,
    node_index: HashMap<NodeId, NodeIndex>,
    nodes: HashMap<NodeId, NodeRef>,
    edges: HashMap<EdgeId, EdgeOut>,
    docs: HashMap<NodeId, String>,
    properties: HashMap<NodeId, BTreeMap<String, String>>,
    by_file: HashMap<String, Vec<(Span, NodeId)>>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    pub fn new() -> Self {
        Self {
            graph: StableDiGraph::new(),
            node_index: HashMap::new(),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            docs: HashMap::new(),
            properties: HashMap::new(),
            by_file: HashMap::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Insert a node, or replace its `NodeRef` (and refresh the file index)
    /// if one already exists. Returns `true` when the node is new.
    pub fn upsert_node(&mut self, node: NodeRef) -> bool {
        let id = node.id;
        let is_new = !self.node_index.contains_key(&id);
        if is_new {
            let idx = self.graph.add_node(id);
            self.node_index.insert(id, idx);
        } else {
            let prev_file = self.nodes.get(&id).map(|n| n.file.clone());
            if let Some(file) = prev_file {
                self.remove_from_file_index(&file, id);
            }
        }
        self.by_file
            .entry(node.file.clone())
            .or_default()
            .push((node.span, id));
        self.nodes.insert(id, node);
        is_new
    }

    /// Insert an edge. Returns `true` when the edge is new. Both endpoints
    /// must already exist; otherwise the edge is dropped silently (the
    /// indexer pipeline guarantees ordering by emitting nodes first).
    pub fn upsert_edge(&mut self, edge: EdgeOut) -> bool {
        if self.edges.contains_key(&edge.id) {
            return false;
        }
        let (Some(&from), Some(&to)) = (
            self.node_index.get(&edge.from),
            self.node_index.get(&edge.to),
        ) else {
            return false;
        };
        self.graph.add_edge(from, to, edge.id);
        self.edges.insert(edge.id, edge);
        true
    }

    pub fn remove_node(&mut self, id: NodeId) -> Option<NodeRef> {
        let idx = self.node_index.remove(&id)?;
        let edges_to_drop: Vec<EdgeId> = self
            .graph
            .edges(idx)
            .map(|e| *e.weight())
            .chain(
                self.graph
                    .edges_directed(idx, PgDirection::Incoming)
                    .map(|e| *e.weight()),
            )
            .collect();
        for eid in edges_to_drop {
            self.edges.remove(&eid);
        }
        self.graph.remove_node(idx);
        let node = self.nodes.remove(&id);
        self.docs.remove(&id);
        self.properties.remove(&id);
        if let Some(n) = &node {
            self.remove_from_file_index(&n.file, id);
        }
        node
    }

    pub fn remove_edge(&mut self, id: EdgeId) -> Option<EdgeOut> {
        let edge = self.edges.remove(&id)?;
        let from_idx = self.node_index.get(&edge.from).copied();
        if let Some(idx) = from_idx {
            let target = self
                .graph
                .edges(idx)
                .find(|e| *e.weight() == id)
                .map(|e| e.id());
            if let Some(eid) = target {
                self.graph.remove_edge(eid);
            }
        }
        Some(edge)
    }

    pub fn set_doc(&mut self, id: NodeId, doc: String) {
        self.docs.insert(id, doc);
    }

    pub fn set_property(&mut self, id: NodeId, key: String, value: String) {
        self.properties.entry(id).or_default().insert(key, value);
    }

    pub fn node_ref(&self, id: NodeId) -> Option<&NodeRef> {
        self.nodes.get(&id)
    }

    /// Iterator over every node currently in the store. Order is
    /// unspecified (HashMap iteration order). Used by the annotation
    /// applier to find which nodes have docs to scan.
    pub fn all_node_refs(&self) -> impl Iterator<Item = &NodeRef> {
        self.nodes.values()
    }

    /// Internal iterators backing the snapshot serializer. Order is
    /// unspecified — `to_snapshot` sorts before emitting.
    pub(crate) fn nodes_iter(&self) -> impl Iterator<Item = &NodeRef> {
        self.nodes.values()
    }

    pub(crate) fn edges_iter(&self) -> impl Iterator<Item = &EdgeOut> {
        self.edges.values()
    }

    pub(crate) fn docs_iter(&self) -> impl Iterator<Item = (&NodeId, &String)> {
        self.docs.iter()
    }

    pub(crate) fn properties_iter(&self) -> impl Iterator<Item = (&NodeId, &BTreeMap<String, String>)> {
        self.properties.iter()
    }

    pub fn node_full(&self, id: NodeId) -> Option<NodeFull> {
        let node = self.nodes.get(&id)?.clone();
        Some(NodeFull {
            node,
            doc: self.docs.get(&id).cloned(),
            properties: self.properties.get(&id).cloned().unwrap_or_default(),
            annotations: Vec::new(),
        })
    }

    /// Innermost-first lookup: returns nodes whose span contains `line`,
    /// sorted by ascending span size (the most specific node first).
    pub fn lookup(&self, file: &str, line: Option<u32>) -> Vec<NodeId> {
        let Some(entries) = self.by_file.get(file) else {
            return Vec::new();
        };
        let mut hits: Vec<&(Span, NodeId)> = match line {
            Some(l) => entries.iter().filter(|(s, _)| s.contains_line(l)).collect(),
            None => entries.iter().collect(),
        };
        hits.sort_by_key(|(s, _)| (s.end_line - s.start_line, s.start_line));
        hits.into_iter().map(|(_, id)| *id).collect()
    }

    pub fn neighbors(
        &self,
        id: NodeId,
        dir: Direction,
        edge_filter: Option<&[EdgeKind]>,
    ) -> Vec<EdgeOut> {
        let Some(&idx) = self.node_index.get(&id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let dirs: &[PgDirection] = match dir {
            Direction::Out => &[PgDirection::Outgoing],
            Direction::In => &[PgDirection::Incoming],
            Direction::Both => &[PgDirection::Outgoing, PgDirection::Incoming],
        };
        for d in dirs {
            for eref in self.graph.edges_directed(idx, *d) {
                let eid = *eref.weight();
                if let Some(edge) = self.edges.get(&eid) {
                    if edge_kind_matches(&edge.kind, edge_filter) {
                        out.push(edge.clone());
                    }
                }
            }
        }
        out
    }

    /// BFS subgraph from `root` to `depth` hops. Walks outgoing edges only;
    /// the daemon can re-call with `Direction::In` for the reverse view.
    /// Filters apply post-traversal so a depth-2 walk still terminates at
    /// the requested depth even when some edges are filtered out.
    pub fn subgraph(
        &self,
        root: NodeId,
        depth: u8,
        edge_filter: Option<&[EdgeKind]>,
        kind_filter: Option<&[NodeKind]>,
        lang_filter: Option<&[Lang]>,
        node_limit: Option<u32>,
    ) -> Subgraph {
        let limit = node_limit.unwrap_or(2_000) as usize;
        let mut nodes_out = Vec::new();
        let mut edges_out = Vec::new();
        let mut seen_nodes: HashSet<NodeId> = HashSet::new();
        let mut seen_edges: HashSet<EdgeId> = HashSet::new();
        let mut truncated = false;

        let Some(&root_idx) = self.node_index.get(&root) else {
            return Subgraph {
                root,
                nodes: nodes_out,
                edges: edges_out,
                truncated: false,
            };
        };

        let mut queue: VecDeque<(NodeIndex, u8)> = VecDeque::new();
        queue.push_back((root_idx, 0));
        seen_nodes.insert(root);
        if let Some(n) = self.nodes.get(&root) {
            if node_passes(n, kind_filter, lang_filter) {
                nodes_out.push(n.clone());
            }
        }

        while let Some((idx, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            for eref in self.graph.edges(idx) {
                let eid = *eref.weight();
                let Some(edge) = self.edges.get(&eid) else {
                    continue;
                };
                if !edge_kind_matches(&edge.kind, edge_filter) {
                    continue;
                }
                let target = eref.target();
                let target_id = self.graph[target];
                if seen_nodes.insert(target_id) {
                    if let Some(n) = self.nodes.get(&target_id) {
                        if node_passes(n, kind_filter, lang_filter) {
                            if nodes_out.len() >= limit {
                                truncated = true;
                                break;
                            }
                            nodes_out.push(n.clone());
                        }
                    }
                    queue.push_back((target, d + 1));
                }
                if seen_edges.insert(eid) {
                    edges_out.push(edge.clone());
                }
            }
            if truncated {
                break;
            }
        }

        Subgraph {
            root,
            nodes: nodes_out,
            edges: edges_out,
            truncated,
        }
    }

    /// Top-level entry-point nodes. With no filter this returns every
    /// `Directory` and every `File` whose parent isn't itself in the
    /// graph — the natural roots for the UI's first render.
    pub fn roots(
        &self,
        lang_filter: Option<&[Lang]>,
        kind_filter: Option<&[NodeKind]>,
    ) -> Vec<NodeRef> {
        let mut out: Vec<NodeRef> = self
            .nodes
            .values()
            .filter(|n| node_passes(n, kind_filter, lang_filter))
            .filter(|n| {
                if kind_filter.is_some() {
                    return true;
                }
                let Some(&idx) = self.node_index.get(&n.id) else {
                    return false;
                };
                self.graph
                    .edges_directed(idx, PgDirection::Incoming)
                    .all(|e| !matches!(self.edges.get(e.weight()).map(|x| &x.kind),
                        Some(EdgeKind::Contains)))
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| a.qualified.cmp(&b.qualified));
        out
    }

    pub fn stats(&self) -> StoreStats {
        let mut by_lang: BTreeMap<String, u64> = BTreeMap::new();
        let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
        for n in self.nodes.values() {
            *by_lang
                .entry(serde_json::to_string(&n.lang).unwrap_or_default())
                .or_insert(0) += 1;
            *by_kind
                .entry(short_kind_label(&n.kind))
                .or_insert(0) += 1;
        }
        StoreStats {
            node_count: self.nodes.len() as u64,
            edge_count: self.edges.len() as u64,
            by_lang,
            by_kind,
        }
    }

    fn remove_from_file_index(&mut self, file: &str, id: NodeId) {
        if let Some(entries) = self.by_file.get_mut(file) {
            entries.retain(|(_, nid)| *nid != id);
            if entries.is_empty() {
                self.by_file.remove(file);
            }
        }
    }

    /// Bulk-rebuild the store from snapshot parts. Called by
    /// [`Store::restore`]. Pre-allocates every side map and bypasses the
    /// per-node dedupe checks `upsert_node` would otherwise perform —
    /// `to_snapshot` already iterates a `HashMap<NodeId, _>` so the
    /// incoming `Vec<NodeRef>` is unique by construction. Edges with a
    /// missing endpoint are dropped silently (same contract as
    /// `upsert_edge`); a `to_snapshot` round-trip never produces them.
    pub(crate) fn rebuild_from_parts(
        &mut self,
        nodes: Vec<NodeRef>,
        edges: Vec<EdgeOut>,
        docs: Vec<(NodeId, String)>,
        properties: Vec<(NodeId, BTreeMap<String, String>)>,
    ) {
        let n_nodes = nodes.len();
        let n_edges = edges.len();

        self.graph = StableDiGraph::with_capacity(n_nodes, n_edges);
        self.node_index = HashMap::with_capacity(n_nodes);
        self.nodes = HashMap::with_capacity(n_nodes);
        self.edges = HashMap::with_capacity(n_edges);
        self.docs = HashMap::with_capacity(docs.len());
        self.properties = HashMap::with_capacity(properties.len());
        self.by_file = HashMap::new();

        for node in nodes {
            let id = node.id;
            let idx = self.graph.add_node(id);
            self.node_index.insert(id, idx);
            match self.by_file.get_mut(&node.file) {
                Some(v) => v.push((node.span, id)),
                None => {
                    self.by_file
                        .insert(node.file.clone(), vec![(node.span, id)]);
                }
            }
            self.nodes.insert(id, node);
        }

        for edge in edges {
            let (Some(&from), Some(&to)) = (
                self.node_index.get(&edge.from),
                self.node_index.get(&edge.to),
            ) else {
                continue;
            };
            self.graph.add_edge(from, to, edge.id);
            self.edges.insert(edge.id, edge);
        }

        for (id, doc) in docs {
            self.docs.insert(id, doc);
        }

        for (id, props) in properties {
            self.properties.insert(id, props);
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub node_count: u64,
    pub edge_count: u64,
    pub by_lang: BTreeMap<String, u64>,
    pub by_kind: BTreeMap<String, u64>,
}

fn node_passes(
    node: &NodeRef,
    kind_filter: Option<&[NodeKind]>,
    lang_filter: Option<&[Lang]>,
) -> bool {
    if let Some(langs) = lang_filter {
        if !langs.contains(&node.lang) {
            return false;
        }
    }
    if let Some(kinds) = kind_filter {
        if !kinds.iter().any(|k| k == &node.kind) {
            return false;
        }
    }
    true
}

fn edge_kind_matches(kind: &EdgeKind, filter: Option<&[EdgeKind]>) -> bool {
    match filter {
        None => true,
        Some(allowed) => allowed.iter().any(|k| k == kind),
    }
}

fn short_kind_label(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Common(c) => format!("common::{:?}", c).to_lowercase(),
        NodeKind::Rust(r) => format!("rust::{:?}", r).to_lowercase(),
        NodeKind::Ts(t) => format!("ts::{:?}", t).to_lowercase(),
        NodeKind::Doc(d) => format!("doc::{:?}", d).to_lowercase(),
        NodeKind::Koda(k) => format!("koda::{:?}", k).to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kg::edge::EdgeId;
    use kg::kind::{CommonKind, Lang, NodeKind};

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
    fn upsert_node_then_lookup_by_line() {
        let mut store = Store::new();
        let n = mk_node("foo", "src/a.rs", 10);
        let id = n.id;
        assert!(store.upsert_node(n));

        let hits = store.lookup("src/a.rs", Some(12));
        assert_eq!(hits, vec![id]);

        let miss = store.lookup("src/a.rs", Some(99));
        assert!(miss.is_empty());
    }

    #[test]
    fn lookup_returns_innermost_first() {
        let mut store = Store::new();
        let outer = NodeRef {
            id: NodeId::compute(Lang::Rust, "outer", "f"),
            lang: Lang::Rust,
            kind: NodeKind::Common(CommonKind::Module),
            label: "outer".into(),
            qualified: "outer".into(),
            file: "f".into(),
            span: Span { start_line: 1, start_col: 1, end_line: 100, end_col: 1 },
            synthetic: false,
        };
        let inner = NodeRef {
            id: NodeId::compute(Lang::Rust, "inner", "f"),
            lang: Lang::Rust,
            kind: NodeKind::Common(CommonKind::Function),
            label: "inner".into(),
            qualified: "inner".into(),
            file: "f".into(),
            span: Span { start_line: 10, start_col: 1, end_line: 20, end_col: 1 },
            synthetic: false,
        };
        let outer_id = outer.id;
        let inner_id = inner.id;
        store.upsert_node(outer);
        store.upsert_node(inner);

        let hits = store.lookup("f", Some(15));
        assert_eq!(hits, vec![inner_id, outer_id]);
    }

    #[test]
    fn upsert_edge_requires_endpoints() {
        let mut store = Store::new();
        let a = mk_node("a", "f", 1);
        let b = mk_node("b", "f", 2);
        let edge = EdgeOut {
            id: EdgeId::compute(a.id, b.id, &EdgeKind::Calls),
            from: a.id,
            to: b.id,
            kind: EdgeKind::Calls,
            annotations: vec![],
        };
        // Endpoints not yet inserted; edge is dropped.
        assert!(!store.upsert_edge(edge.clone()));
        store.upsert_node(a);
        store.upsert_node(b);
        assert!(store.upsert_edge(edge.clone()));
        assert!(!store.upsert_edge(edge)); // dedupe
        assert_eq!(store.edge_count(), 1);
    }

    #[test]
    fn subgraph_walks_to_depth() {
        let mut store = Store::new();
        let mut prev: Option<NodeId> = None;
        let mut ids = Vec::new();
        for i in 0..5 {
            let n = mk_node(&format!("n{}", i), "f", i as u32 * 10);
            let id = n.id;
            ids.push(id);
            store.upsert_node(n);
            if let Some(p) = prev {
                let e = EdgeOut {
                    id: EdgeId::compute(p, id, &EdgeKind::Calls),
                    from: p,
                    to: id,
                    kind: EdgeKind::Calls,
                    annotations: vec![],
                };
                store.upsert_edge(e);
            }
            prev = Some(id);
        }
        let sg = store.subgraph(ids[0], 2, None, None, None, None);
        // root + 2 hops → 3 nodes
        assert_eq!(sg.nodes.len(), 3);
        assert!(!sg.truncated);
    }

    #[test]
    fn remove_node_drops_incident_edges() {
        let mut store = Store::new();
        let a = mk_node("a", "f", 1);
        let b = mk_node("b", "f", 2);
        let aid = a.id;
        let bid = b.id;
        store.upsert_node(a);
        store.upsert_node(b);
        let e = EdgeOut {
            id: EdgeId::compute(aid, bid, &EdgeKind::Calls),
            from: aid,
            to: bid,
            kind: EdgeKind::Calls,
            annotations: vec![],
        };
        store.upsert_edge(e);
        assert_eq!(store.edge_count(), 1);
        store.remove_node(aid);
        assert_eq!(store.node_count(), 1);
        assert_eq!(store.edge_count(), 0);
    }
}
