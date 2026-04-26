//! @arch:layer(kg)
//! @arch:role(protocol)
//!
//! RPC request/response shapes for the `arch.*` namespace exposed by
//! the yah daemon. Transport-agnostic — the daemon may serve these
//! over JSON-RPC stdio (Tauri local), JSON over HTTP (browser), or
//! SSH-RPC (remote rig). All shapes here serialize to JSON.
//!
//! Method dispatch is by method name; the `RpcRequest`/`RpcResponse`
//! enums are provided as a convenience for the daemon's router and
//! for typed clients.

use crate::edge::{EdgeKind, EdgeOut};
use crate::ids::{NodeFull, NodeId, NodeRef};
use crate::kind::{Lang, NodeKind};
use serde::{Deserialize, Serialize};

// ---------- arch.roots ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Lang>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<NodeKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsResult {
    pub roots: Vec<NodeRef>,
}

// ---------- arch.subgraph ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgraphParams {
    pub root: NodeId,
    pub depth: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<EdgeKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<NodeKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<Lang>>,
    /// Hard cap on returned nodes. Daemon sets a default if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subgraph {
    pub root: NodeId,
    pub nodes: Vec<NodeRef>,
    pub edges: Vec<EdgeOut>,
    /// True if the result was capped by `node_limit` or `depth`.
    pub truncated: bool,
}

// ---------- arch.lookup ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupParams {
    /// Rig-relative path.
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResult {
    /// Innermost-first: a method node before the type that contains it,
    /// before the module, before the file.
    pub ids: Vec<NodeId>,
}

// ---------- arch.node ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeParams {
    pub id: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node: NodeFull,
}

// ---------- arch.neighbors ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborsParams {
    pub id: NodeId,
    pub dir: Direction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<EdgeKind>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborsResult {
    pub edges: Vec<EdgeOut>,
}

// ---------- arch.path ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathParams {
    pub from: NodeId,
    pub to: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edges: Option<Vec<EdgeKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub paths: Vec<Vec<EdgeOut>>,
}

// ---------- arch.search ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<NodeKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<Lang>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: NodeId,
    pub label: String,
    pub qualified: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
}

// ---------- arch.expand_macro (v2) ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandMacroParams {
    pub id: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandMacroResult {
    /// Items synthesized by this macro (each `synthetic = true`).
    pub generated: Vec<NodeRef>,
}

// ---------- arch.languages ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguagesResult {
    pub langs: Vec<Lang>,
}

// ---------- arch.stats ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResult {
    pub node_count: u64,
    pub edge_count: u64,
    pub by_lang: std::collections::BTreeMap<String, u64>,
    pub by_kind: std::collections::BTreeMap<String, u64>,
    /// Wall-clock time of the most recent full or incremental index.
    pub last_index_ms: Option<u64>,
}

// ---------- arch.reindex ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexParams {
    /// Default `all` if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ReindexScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ReindexScope {
    All,
    File { path: String },
    Subtree { root: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexResult {
    pub job_id: String,
}

// ---------- Method registry ----------

/// Canonical method names. Use these constants instead of stringly-typed
/// dispatch in routers and clients to avoid drift.
pub mod method {
    pub const ROOTS: &str = "arch.roots";
    pub const SUBGRAPH: &str = "arch.subgraph";
    pub const LOOKUP: &str = "arch.lookup";
    pub const NODE: &str = "arch.node";
    pub const NEIGHBORS: &str = "arch.neighbors";
    pub const PATH: &str = "arch.path";
    pub const SEARCH: &str = "arch.search";
    pub const EXPAND_MACRO: &str = "arch.expand_macro";
    pub const LANGUAGES: &str = "arch.languages";
    pub const STATS: &str = "arch.stats";
    pub const REINDEX: &str = "arch.reindex";
    pub const SUBSCRIBE: &str = "arch.subscribe";
}

/// Convenience tagged-union for routers that want to round-trip the
/// whole RPC surface as one type. Optional — call sites can also
/// dispatch on method strings directly and deserialize each param
/// type independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum RpcRequest {
    #[serde(rename = "arch.roots")]
    Roots(RootsParams),
    #[serde(rename = "arch.subgraph")]
    Subgraph(SubgraphParams),
    #[serde(rename = "arch.lookup")]
    Lookup(LookupParams),
    #[serde(rename = "arch.node")]
    Node(NodeParams),
    #[serde(rename = "arch.neighbors")]
    Neighbors(NeighborsParams),
    #[serde(rename = "arch.path")]
    Path(PathParams),
    #[serde(rename = "arch.search")]
    Search(SearchParams),
    #[serde(rename = "arch.expand_macro")]
    ExpandMacro(ExpandMacroParams),
    #[serde(rename = "arch.languages")]
    Languages,
    #[serde(rename = "arch.stats")]
    Stats,
    #[serde(rename = "arch.reindex")]
    Reindex(ReindexParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "result")]
pub enum RpcResponse {
    #[serde(rename = "arch.roots")]
    Roots(RootsResult),
    #[serde(rename = "arch.subgraph")]
    Subgraph(Subgraph),
    #[serde(rename = "arch.lookup")]
    Lookup(LookupResult),
    #[serde(rename = "arch.node")]
    Node(NodeResult),
    #[serde(rename = "arch.neighbors")]
    Neighbors(NeighborsResult),
    #[serde(rename = "arch.path")]
    Path(PathResult),
    #[serde(rename = "arch.search")]
    Search(SearchResult),
    #[serde(rename = "arch.expand_macro")]
    ExpandMacro(ExpandMacroResult),
    #[serde(rename = "arch.languages")]
    Languages(LanguagesResult),
    #[serde(rename = "arch.stats")]
    Stats(StatsResult),
    #[serde(rename = "arch.reindex")]
    Reindex(ReindexResult),
}
