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
//!

use crate::anno::{WorkItemAnno, WorkItemType};
use crate::edge::{EdgeKind, EdgeOut};
use crate::ids::{NodeFull, NodeId, NodeRef};
use crate::kind::{Lang, NodeKind};
use crate::prompt::PromptMode;
use crate::validate::{Scope, Violation};
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

// ---------- arch.list_tickets / arch.list_relays / arch.get_ticket ----------

/// One source location backing a synthetic Relay/Ticket node. A given
/// work-item ID may appear in multiple files (rare, but legal); each
/// `Anchors` edge surfaces here so the UI can offer "open all sources"
/// and warn about drift.
///
/// `anno` carries this anchor's parsed payload — when the same id appears
/// in multiple files with disagreeing scalars, each anchor preserves its
/// own view. The board-recompute layer (`yah_kg::board`) consumes the
/// per-anchor payloads to surface field-level conflicts; the daemon picks
/// the lex-first anchor's payload as the convenience `WorkItem::anno`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemAnchor {
    /// Structural node carrying the `@yah:relay`/`@yah:ticket` annotation.
    pub node: NodeId,
    /// Rig-relative path of that structural node.
    pub file: String,
    /// 1-based line of the structural anchor's span start.
    pub line: u32,
    /// The annotation payload as parsed at this anchor. With one anchor
    /// per work-item this matches `WorkItem::anno`; with multiple anchors
    /// the per-anchor copies are what the board-recompute layer compares
    /// to detect field disagreements.
    #[serde(default)]
    pub anno: WorkItemAnno,
}

/// Wire shape for one synthetic Relay/Ticket as seen from the RPC layer.
/// Combines the synthetic node's identity (`node`, `id`, `item_type`),
/// the parsed annotation payload (`anno`), and the structural anchors
/// (`anchors`) the work-item lives on. Used uniformly by
/// `arch.list_tickets`, `arch.list_relays`, and `arch.get_ticket`.
///
/// `anno` is the lex-first anchor's payload (deterministic winner across
/// multi-file declarations). The full per-anchor payloads live on
/// `anchors[i].anno` for callers that need to detect or render scalar
/// disagreements — see [`crate::board`].
///
/// `last_modified_ts` is unix seconds of the most recent event recorded
/// in `.yah/events/<shard>.jsonl` for this id (status moves, scans,
/// anything that touches the ticket). When no shard exists the daemon
/// substitutes the source file's mtime so freshly-claimed tickets sort
/// sensibly; `0` means neither was resolvable (e.g. cold-start before
/// boot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub node: NodeId,
    pub item_type: WorkItemType,
    pub anno: WorkItemAnno,
    pub anchors: Vec<WorkItemAnchor>,
    #[serde(default)]
    pub last_modified_ts: u64,
}

/// `arch.list_tickets` takes no parameters today. Reserved for filters
/// (parent relay, status, assignee) once the UI grows them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTicketsParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTicketsResult {
    pub tickets: Vec<WorkItem>,
}

/// `arch.list_relays` takes no parameters today. Reserved for filters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListRelaysParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRelaysResult {
    pub relays: Vec<WorkItem>,
}

/// `arch.get_ticket` looks up by the bare work-item ID — the same string
/// authors write in `@yah:ticket(R042-T1, ...)` (no `ticket:` prefix).
/// Returns `None` when no synthetic Ticket node bears that id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTicketParams {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTicketResult {
    pub ticket: Option<WorkItem>,
}

// ---------- arch.ticket_prompt ----------

/// `arch.ticket_prompt` renders the canonical pickup or review markdown for
/// one work-item id. The CLI's `yah board show <id> --prompt` and the
/// Tauri client's "Prompt"/"Review" buttons both call into this RPC so
/// they cannot drift on prompt shape — the rendering lives in
/// [`crate::prompt`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketPromptParams {
    /// Bare work-item id, same shape authors write in `@yah:ticket(R042-T1, ...)`.
    pub id: String,
    /// Pickup (next-agent briefing) or Review (verifier framing). Defaults
    /// to Pickup when omitted on the wire.
    #[serde(default)]
    pub mode: PromptMode,
}

/// `markdown` is `None` when no work-item bears `params.id` (mirrors
/// [`GetTicketResult::ticket`]'s null-when-missing convention). The UI
/// surfaces the miss as a transient toast rather than throwing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketPromptResult {
    pub markdown: Option<String>,
}

// ---------- arch.validate ----------

/// Run the rule validator across the requested slice of the graph. Scope
/// defaults to [`Scope::All`] when absent — the common interactive case
/// (validate everything).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidateParams {
    #[serde(default)]
    pub scope: Scope,
}

/// One validation pass returns zero or more violations. Empty `violations`
/// means every authored `@yah:rule(...)` in scope passed; the UI can render
/// that as a green check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResult {
    pub violations: Vec<Violation>,
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
    pub const LIST_TICKETS: &str = "arch.list_tickets";
    pub const LIST_RELAYS: &str = "arch.list_relays";
    pub const GET_TICKET: &str = "arch.get_ticket";
    pub const VALIDATE: &str = "arch.validate";
    pub const TICKET_PROMPT: &str = "arch.ticket_prompt";
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
    #[serde(rename = "arch.list_tickets")]
    ListTickets(ListTicketsParams),
    #[serde(rename = "arch.list_relays")]
    ListRelays(ListRelaysParams),
    #[serde(rename = "arch.get_ticket")]
    GetTicket(GetTicketParams),
    #[serde(rename = "arch.validate")]
    Validate(ValidateParams),
    #[serde(rename = "arch.ticket_prompt")]
    TicketPrompt(TicketPromptParams),
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
    #[serde(rename = "arch.list_tickets")]
    ListTickets(ListTicketsResult),
    #[serde(rename = "arch.list_relays")]
    ListRelays(ListRelaysResult),
    #[serde(rename = "arch.get_ticket")]
    GetTicket(GetTicketResult),
    #[serde(rename = "arch.validate")]
    Validate(ValidateResult),
    #[serde(rename = "arch.ticket_prompt")]
    TicketPrompt(TicketPromptResult),
}
