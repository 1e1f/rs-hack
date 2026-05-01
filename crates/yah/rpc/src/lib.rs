//! @arch:layer(rpc)
//! @arch:role(protocol)
//!
//! RPC request/response shapes for the `arch.*` / `file.*` / `dir.*`
//! namespaces exposed by the yah daemon. Transport-agnostic — the daemon
//! may serve these over JSON-RPC stdio (Tauri local), JSON over HTTP
//! (browser), or SSH-RPC (remote rig). All shapes here serialize to JSON.
//!
//! Method dispatch is by method name; the `RpcRequest`/`RpcResponse`
//! enums are provided as a convenience for the daemon's router and
//! for typed clients.
//!
//! Identity, edge, and node-kind types live in [`kg`] (the contract crate
//! for the knowledge graph itself); this crate is a thin layer that
//! composes them into the wire shapes a transport actually sees.
//!

use kg::edge::{EdgeKind, EdgeOut};
use kg::ids::{NodeFull, NodeId, NodeRef};
use kg::kind::{Lang, NodeKind};
use kg::prelude::Prelude;
use kg::prompt::PromptMode;
use kg::validate::{Scope, Violation};
use serde::{Deserialize, Serialize};

// Aggregate graph-view DTOs and traversal-direction filter live in `kg`
// (where the prelude assembler, store, and board recompute consume them
// directly). Re-export here so existing `rpc::Direction`/`rpc::Subgraph`/
// `rpc::WorkItem` call sites keep resolving.
pub use kg::board::{WorkItem, WorkItemAnchor};
pub use kg::edge::Direction;
pub use kg::subgraph::Subgraph;

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

// `WorkItem` and `WorkItemAnchor` are owned by `kg::board` (re-exported at
// the top of this module) so the in-process board-recompute layer can
// consume them without taking a transport dep on `rpc`.

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
/// [`kg::prompt`].
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

// ---------- arch.move_ticket ----------

/// `arch.move_ticket` rewrites a ticket's `@yah:status(...)` line in
/// source to mirror a column drag-and-drop in the UI. The daemon
/// validates the transition (open→active, active→{open,handoff,review},
/// handoff→{active,review}, review→handoff), rewrites the source file
/// at the ticket's primary anchor, and triggers an
/// `IndexReason::AgentEdit` reindex so subscribers see the resulting
/// `WorkItemChanged` event with the right cause.
///
/// `to_bucket` is the column the renderer dropped onto: `open`,
/// `active`, `handoff`, or `review`. Canonical statuses (`in-progress`,
/// `done`) are not accepted on the wire — the daemon owns the
/// bucket→status mapping so the UI doesn't have to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveTicketParams {
    /// Bare work-item id, same shape authors write in
    /// `@yah:ticket(R042-T1, ...)`.
    pub id: String,
    /// Target column. One of `open` | `active` | `handoff` | `review`.
    pub to_bucket: String,
}

/// Successful column move. `from_status` is what the source file held
/// before the rewrite; `to_status` is what was just written. `file` is
/// rig-relative so the renderer can show a clickable source link
/// without knowing the rig root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveTicketResult {
    pub id: String,
    pub from_status: String,
    pub to_status: String,
    pub file: String,
    pub line: u32,
}

// ---------- arch.archive_ticket ----------

/// `arch.archive_ticket` strips the `@yah:*` annotation lines that own
/// `id` from source and appends a single `archived` event to the
/// per-relay shard at `.yah/events/<shard>.jsonl`. Sub-tickets share
/// their parent's shard. The event carries a full `WorkItem` snapshot
/// plus the stripped source lines so the audit log alone is enough to
/// reconstruct or rehydrate the ticket later.
///
/// Validation mirrors `yah board archive` and the legacy hack-board
/// server: claimed/in-progress tickets must move to review/handoff
/// first, and epics with live children are rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveTicketParams {
    pub id: String,
}

/// Successful archive. `file`/`line` point at the (now-removed)
/// declaration's original location for UI breadcrumbs; `removed_lines`
/// is how many `@yah:*` lines were stripped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveTicketResult {
    pub id: String,
    pub file: String,
    pub line: u32,
    pub removed_lines: u32,
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

// ---------- arch.list_authored_files ----------

/// `arch.list_authored_files` enumerates `.mmd` (raw mermaid) and `.md`
/// (markdown, with `~~~mermaid` fences specialized by the renderer) files
/// under `<rig_root>/.yah/arch/authored/`. The daemon walks the directory
/// each call — cheap, and avoids needing to thread a watcher through.
/// Returns rig-relative paths so the renderer can echo them back to
/// `arch.read_authored_file` without manipulating the rig root itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListAuthoredFilesParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAuthoredFilesResult {
    pub files: Vec<AuthoredFile>,
}

/// One entry under `.yah/arch/authored/`. `rel_path` is rooted at the rig
/// (e.g. `.yah/arch/authored/yah-managed-rigs-topology.mmd`); `name` is the
/// basename without extension for picker display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthoredFile {
    pub rel_path: String,
    pub name: String,
    pub bytes: u64,
}

// ---------- arch.read_authored_file ----------

/// `arch.read_authored_file` returns the raw contents of one file under
/// `.yah/arch/authored/`. The daemon enforces the sandbox: any `rel_path`
/// that, after canonicalization, escapes that directory is rejected as a
/// `Conflict` rather than read. This is the dominant safety property of
/// the method — the renderer never gets to read arbitrary files via this
/// command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadAuthoredFileParams {
    /// Rig-relative path as returned by `arch.list_authored_files`. Must
    /// resolve inside `<rig_root>/.yah/arch/authored/`.
    pub rel_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadAuthoredFileResult {
    pub rel_path: String,
    pub content: String,
    pub bytes: u64,
}

// ---------- file.read ----------

/// Read bytes from a file under the rig root. The path is rig-relative —
/// the daemon canonicalizes it and rejects anything that resolves outside
/// `<rig_root>` (no `..` escapes, no symlink-to-outside).
///
/// `range` lets the renderer page through a file larger than the soft cap
/// (5MB). When omitted, the daemon reads from offset 0 and clips the
/// response at the cap with `truncated = true`. With `range` present,
/// the cap does not apply — the caller has decided to page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadParams {
    /// Rig-relative path. POSIX separators.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<FileReadRange>,
}

/// Byte range for a paged read. `len` is a `u32` because Monaco models
/// past a few MB are unworkable; we cap individual chunks well below
/// that.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FileReadRange {
    pub offset: u64,
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEncoding {
    /// `content` is the file slice as a UTF-8 string. Chosen when the
    /// requested bytes are valid UTF-8.
    Utf8,
    /// `content` is the file slice base64-encoded (standard alphabet, no
    /// padding stripped). Chosen for bytes that aren't valid UTF-8 — the
    /// renderer can show the binary fallback view.
    Base64,
}

/// Result of [`method::FILE_READ`]. `content` is decoded
/// per `encoding`. `bytes` is the size of the slice in `content` (after
/// decode); `total_bytes` is the file's full size on disk so the
/// renderer can show progress when paging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadResult {
    pub path: String,
    pub content: String,
    pub encoding: FileEncoding,
    /// Number of raw bytes represented by `content`.
    pub bytes: u32,
    /// Total file size on disk. `bytes <= total_bytes` always.
    pub total_bytes: u64,
    /// Offset of the first returned byte. `0` when `range` was absent.
    pub offset: u64,
    /// True if the read reached the end of the file.
    pub eof: bool,
    /// True if the soft cap clipped the read (no `range` and file
    /// exceeded the 5MB cap). Mutually exclusive with `eof` when set.
    pub truncated: bool,
}

// ---------- file.write ----------

/// Write bytes to a file under the rig root with optimistic concurrency.
/// The path is rig-relative — same canonicalization rules as
/// [`FileReadParams::path`].
///
/// `expected_mtime_ms` discriminates the two operations:
///
/// - `None` — create a new file. The write fails with a Conflict if the
///   target already exists. Renderers pair this with the buffer-state
///   "new untitled file" path.
/// - `Some(t)` — update an existing file. The write fails with a Conflict
///   ("mtime mismatch: ...") if the target's current mtime differs from
///   `t`. The renderer's contract: capture `mtime_ms` from the prior
///   `file.read` (or the most recent `file.event`) and pass it back as
///   `expected_mtime_ms`. The "file changed on disk" prompt branches on
///   this Conflict.
///
/// `content` is interpreted per `encoding` — utf8 strings or
/// base64-encoded bytes. The daemon writes atomically (temp file +
/// rename) so a partial buffer is never visible to readers or the
/// `notify` watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteParams {
    /// Rig-relative path. POSIX separators.
    pub path: String,
    /// Bytes to write, encoded per `encoding`.
    pub content: String,
    /// How `content` is encoded. `Utf8` interprets `content` as a UTF-8
    /// string; `Base64` decodes it as standard-alphabet base64 before
    /// writing.
    pub encoding: FileEncoding,
    /// Optimistic-concurrency token from the prior `file.read` /
    /// `file.event`. `None` means "create new file"; `Some(t)` means
    /// "update existing file whose mtime is `t`". See type docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_mtime_ms: Option<i64>,
}

/// Result of [`method::FILE_WRITE`]. `mtime_ms` is the
/// freshly-written file's mtime — the renderer stashes it as the next
/// `expected_mtime_ms` and uses it to dedup the watch event that the
/// write itself fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteResult {
    pub path: String,
    /// Modification time in milliseconds since the Unix epoch of the
    /// file as the daemon left it. `None` only on platforms/filesystems
    /// that don't report mtime — matches `DirEntry::mtime_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<i64>,
    /// Number of raw bytes written (post-decode).
    pub bytes: u64,
    /// True if the write created a new file; false if it overwrote.
    pub created: bool,
}

// ---------- dir.list ----------

/// One-shot listing of a directory under the rig root. The path is
/// rig-relative; the daemon canonicalizes it and rejects anything that
/// resolves outside `<rig_root>` (no `..` escapes, no symlink-to-outside).
/// An empty `path` (or `.`) lists the rig root itself.
///
/// This is *not* a recursive walk — it returns the immediate children only.
/// Tree views compose this with `dir.watch` (R033-T3) to keep their model
/// in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirListParams {
    /// Rig-relative path. POSIX separators. Empty string lists the rig root.
    pub path: String,
}

/// Kind of a directory entry. `Other` covers fifos, sockets, block/char
/// devices — anything the renderer can't open as a file. Symlinks are
/// reported by their *target* kind so a symlink-to-directory shows up as
/// `Dir` in the tree (matching VS Code's behaviour); the `is_symlink`
/// flag preserves the link-vs-real distinction for callers that care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirEntryKind {
    File,
    Dir,
    Other,
}

/// One entry returned by `dir.list`. `name` is the basename only (no path
/// separators); join it with the parent's `path` to address the entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: DirEntryKind,
    /// File size in bytes for regular files; `0` for directories.
    pub size: u64,
    /// Modification time in milliseconds since the Unix epoch. `None` if
    /// the platform/filesystem can't report it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<i64>,
    /// True when the source entry is a symlink. The reported `kind` is
    /// the *target's* kind, not `Symlink` — broken symlinks come back as
    /// `Other` with `is_symlink: true`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_symlink: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirListResult {
    /// Echo of the requested path (rig-relative, normalized to POSIX
    /// separators). Empty string when the rig root was listed.
    pub path: String,
    /// Entries sorted by `(kind == Dir ? 0 : 1, name)` — directories
    /// first, then files, each group case-insensitive lexicographic. The
    /// renderer can re-sort but having a stable default keeps tests and
    /// snapshot-based diffs deterministic.
    pub entries: Vec<DirEntry>,
}

// ---------- file.watch / dir.watch / file.unwatch ----------

/// Subscribe to filesystem changes under a specific file. The path is
/// rig-relative; the daemon canonicalizes it and rejects anything that
/// resolves outside `<rig_root>` or doesn't currently exist as a regular
/// file. (A future revision may relax the existence check so a watcher
/// can be armed before the file is created — punted for v1.)
///
/// Returns a [`WatchResult`] whose `id` is the handle the renderer
/// passes to [`UnwatchParams`] when it's done. Until unwatched, the
/// daemon emits `file.event` JSON-RPC notifications carrying the
/// handle's id whenever the underlying file changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatchParams {
    /// Rig-relative path to a regular file. POSIX separators.
    pub path: String,
}

/// Subscribe to filesystem changes under a directory. Recursive — every
/// descendant is reported. Path rules match [`FileWatchParams::path`]
/// except the target must canonicalize to a directory (the rig root
/// itself is fair game; pass the empty string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirWatchParams {
    /// Rig-relative path to a directory. Empty string watches the rig root.
    pub path: String,
}

/// Result of either [`method::FILE_WATCH`] or
/// [`method::DIR_WATCH`]. The handle id is per-rig-process
/// stable until the daemon restarts; renderers should re-watch on
/// reconnect rather than caching ids across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchResult {
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnwatchParams {
    pub id: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnwatchResult {}

// ---------- arch.assemble_prelude ----------

/// `arch.assemble_prelude` builds the per-ticket cached prefix the agent
/// runtime injects into the system prompt every turn (R028). The daemon
/// gathers everything pure [`kg::prelude::assemble`] needs — board,
/// KG slice rooted at the ticket's primary anchor, and bodies of any
/// `@arch:see` docs short enough to inline — and calls the assembler.
///
/// Returns `prelude: None` when no work-item bears `params.id` (mirrors
/// [`GetTicketResult::ticket`]'s null-when-missing convention).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblePreludeParams {
    /// Bare work-item id, same shape authors write in `@yah:ticket(R042-T1, ...)`.
    pub id: String,
    /// KG slice depth around the ticket's primary anchor. Defaults to `2`
    /// when omitted — enough to surface immediate dependencies without
    /// blowing the token budget on a deep walk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kg_depth: Option<u8>,
    /// Forwarded to [`kg::prelude::PreludeOptions::max_tokens`]. Daemon
    /// applies the `PreludeOptions` default when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Forwarded to [`kg::prelude::PreludeOptions::kg_node_limit`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kg_node_limit: Option<u32>,
    /// Forwarded to [`kg::prelude::PreludeOptions::arch_inline_max_bytes`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch_inline_max_bytes: Option<u32>,
}

impl AssemblePreludeParams {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kg_depth: None,
            max_tokens: None,
            kg_node_limit: None,
            arch_inline_max_bytes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblePreludeResult {
    pub prelude: Option<Prelude>,
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
    pub const MOVE_TICKET: &str = "arch.move_ticket";
    pub const ARCHIVE_TICKET: &str = "arch.archive_ticket";
    pub const LIST_AUTHORED_FILES: &str = "arch.list_authored_files";
    pub const READ_AUTHORED_FILE: &str = "arch.read_authored_file";
    pub const ASSEMBLE_PRELUDE: &str = "arch.assemble_prelude";
    pub const FILE_READ: &str = "file.read";
    pub const FILE_WRITE: &str = "file.write";
    pub const DIR_LIST: &str = "dir.list";
    pub const FILE_WATCH: &str = "file.watch";
    pub const DIR_WATCH: &str = "dir.watch";
    pub const FILE_UNWATCH: &str = "file.unwatch";
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
    #[serde(rename = "arch.move_ticket")]
    MoveTicket(MoveTicketParams),
    #[serde(rename = "arch.archive_ticket")]
    ArchiveTicket(ArchiveTicketParams),
    #[serde(rename = "arch.list_authored_files")]
    ListAuthoredFiles(ListAuthoredFilesParams),
    #[serde(rename = "arch.read_authored_file")]
    ReadAuthoredFile(ReadAuthoredFileParams),
    #[serde(rename = "arch.assemble_prelude")]
    AssemblePrelude(AssemblePreludeParams),
    #[serde(rename = "file.read")]
    FileRead(FileReadParams),
    #[serde(rename = "file.write")]
    FileWrite(FileWriteParams),
    #[serde(rename = "dir.list")]
    DirList(DirListParams),
    #[serde(rename = "file.watch")]
    FileWatch(FileWatchParams),
    #[serde(rename = "dir.watch")]
    DirWatch(DirWatchParams),
    #[serde(rename = "file.unwatch")]
    FileUnwatch(UnwatchParams),
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
    #[serde(rename = "arch.move_ticket")]
    MoveTicket(MoveTicketResult),
    #[serde(rename = "arch.archive_ticket")]
    ArchiveTicket(ArchiveTicketResult),
    #[serde(rename = "arch.list_authored_files")]
    ListAuthoredFiles(ListAuthoredFilesResult),
    #[serde(rename = "arch.read_authored_file")]
    ReadAuthoredFile(ReadAuthoredFileResult),
    #[serde(rename = "arch.assemble_prelude")]
    AssemblePrelude(AssemblePreludeResult),
    #[serde(rename = "file.read")]
    FileRead(FileReadResult),
    #[serde(rename = "file.write")]
    FileWrite(FileWriteResult),
    #[serde(rename = "dir.list")]
    DirList(DirListResult),
    #[serde(rename = "file.watch")]
    FileWatch(WatchResult),
    #[serde(rename = "dir.watch")]
    DirWatch(WatchResult),
    #[serde(rename = "file.unwatch")]
    FileUnwatch(UnwatchResult),
}
