//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! `KgService` — the daemon's primary handle.
//!
//! Construction is synchronous (no I/O, no tasks). The service starts cold
//! and is brought online by [`KgService::boot`], which walks the rig and
//! populates the store. Watching is opt-in via
//! [`KgService::start_watching`].
//!
//! Method shapes mirror the `arch.*` RPC surface in `yah_kg::rpc`.
//!
//! @yah:relay(R019, "Remote rigs SSH-RPC v2: extract yah-rpc + yah-rpc-ssh transport")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R013)
//! @arch:see(architecture/yah-roadmap-2026Q2.md)
//!
//! @yah:ticket(R017-F2, "arch.validate(scope) RPC + Tauri command surface")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Tauri command surfaces violations to the UI for inline display")
//! @yah:handoff("Backend surface for arch.validate landed. yah-kg now owns the wire types in a new src/validate.rs (Scope, Severity, Violation — Scope uses tagged-struct variants {scope:'all'|'subtree'|'file'} so the JSON shape is self-describing); yah-kg-validator re-exports them via its existing names so engine/tests stayed compatible (engine destructures the new struct variants; tests/engine.rs::scope_file_filters_anchors updated to Scope::File { path: ... }). yah-kg::rpc gained ValidateParams { #[serde(default)] scope: Scope } + ValidateResult { violations: Vec<Violation> } + method::VALIDATE constant + RpcRequest/RpcResponse variants. yah-kg-daemon depends on yah-kg-validator and exposes KgService::validate(params) -> ValidateResult that takes read locks on store+annotations and runs the validator. app/tauri/src/commands.rs adds arch_validate(rig_id, params: Option<ValidateParams>) -> Result<ValidateResult, String> (Option mirrors arch_list_tickets/arch_list_relays so the renderer can call invoke('arch_validate', { rig_id }) and get a Scope::All run by default). Registered in app/tauri/src/lib.rs invoke_handler. cargo build -p yah-tauri green; cargo test -p yah-kg -p yah-kg-validator -p yah-kg-anno -p yah-kg-daemon 11/11 + 19/19 + e2e all green. Pre-existing yah test arch::mcp::tests::test_tool_definitions (asserts len==7 against a list of 6) is unrelated breakage from prior commits.")
//! @yah:next("yah-ui: add rpc.validate(rigId, scope?) -> ValidateResult to env/index.ts + tauri.ts (invoke('arch_validate', { rigId, params: scope ? { scope } : undefined })); browser.ts can return { violations: [] } as a stub")
//! @yah:next("ArchView: call rpc.validate(rigId) on mount + after every IndexFinished event; surface violations as inline pills/badges on the offending nodes (use Violation.offending → graph node)")
//! @yah:next("TicketCard / Board: also fetch + show violations whose anchor is in the current rig — the rule lives on a structural node but reads as a status line on the relay/ticket that owns it")
//! @yah:next("Add 'arch.validate' to yah-ui/src/types.ts wire mirrors (WireViolation, WireSeverity, WireScope) so the env adapter is fully typed")
//!
//! @yah:ticket(R017-F3, "KgService::save(path) + ::load(path) snapshots; mtime-diff on boot")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Use postcard for size; serde_json acceptable as a v1")
//! @yah:next("Write snapshot on IndexFinished, replay on next boot, then quick mtime-diff to bring stale files current")
//! @yah:next("Old rs-hack-arch had source_hash caching worth lifting")
//! @yah:verify("Cold boot on a workspace with a saved snapshot is order-of-magnitude faster than full reindex")
//! @yah:handoff("KG snapshot persistence landed. New surface: KgService::save(path) / load(path) / boot_with_snapshot(rig_root, snapshot_path) / save_default() — backed by KgSnapshot { rig_root, fingerprints, store, annotations } at .yah/cache/snapshot.msgpack (v1, atomic temp+rename, JSON for v1 — postcard slot reserved by SNAPSHOT_VERSION). yah-kg-store now exposes Store::to_snapshot/restore + StoreSnapshot/SnapshotError; yah-kg-anno exposes AnnotationIndex::to_snapshot/restore + AnnotationIndexSnapshot. yah-kg-daemon got snapshot.rs (FileFingerprint mtime+size, fingerprint_rig walker mirroring walk_and_index skip rules, diff_fingerprints → ReconcilePlan). boot() now refreshes fingerprints; boot_with_snapshot loads + walks + reconciles via reindex_path on each changed/removed file (falls back to full boot when snapshot is missing or rig_root mismatches). Tests: 5 new e2e (round-trip, fallback, mtime-diff skip-vs-reindex, deletion reconcile, rig_root mismatch); 21/21 daemon e2e green; yah-kg / yah-kg-store / yah-kg-anno / yah-kg-validator unit tests all green; yah-tauri builds. Tauri startup wiring is now its own sub-ticket R017-T7. Source-hash caching from old rs-hack-arch is a future optimization (currently mtime+size only).")
//! @yah:next("R017-T7 owns the Tauri host wiring + the verify-gate measurement; this ticket is the backend surface only")
//! @yah:next("Optional follow-up: lift source_hash caching from old rs-hack-arch so 'touched but unchanged' files (git checkout, formatter passes) skip reindex via content hash, not just mtime+size")
//!
//! @yah:ticket(R019-T1, "Extract yah-rpc: transport-agnostic crate from existing serde shapes")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R019)
//! @yah:next("Trivially derivable from yah-kg/src/rpc.rs — split request/response types into yah-rpc")
//! @yah:next("yah-kg-daemon and the Tauri command layer both consume it")
//!
//! @yah:ticket(R019-F2, "yah-rpc-ssh: run daemon on remote host, pipe JSON-RPC over SSH stdio")
//! @yah:assignee(agent:claude)
//! @yah:status(handoff)
//! @yah:phase(P3)
//! @yah:parent(R019)
//! @yah:next("Use openssh-rs or shell out to ssh; framing is line-delimited JSON-RPC")
//! @yah:next("Health/keepalive: re-establish on connection drop with exponential backoff")
//! @yah:handoff("yah-rpc-ssh crate landed. New workspace member at yah-rpc-ssh/ split into two layers so each is independently testable: session.rs owns a transport-agnostic JsonRpcSession over any AsyncRead+AsyncWrite (line-delimited JSON-RPC 2.0, pending-id multiplex, broadcast fan-out for arch:event notifications, marks-closed-and-drains-pending on EOF) — tested with in-memory tokio::io::duplex pipes against a fake echo server. client.rs owns SshRpcClient: lazy first-call connect, shells out to system 'ssh' binary (-T BatchMode=yes ServerAliveInterval=30 ServerAliveCountMax=3, kill_on_drop) launching 'yah serve --stdio --rig <workspace>' on the remote, exponential-backoff reconnect inside .call() on TransportClosed (initial 250ms, max 8s, 5 attempts default — ReconnectPolicy is overrideable). Public surface mirrors KgService: subgraph/lookup/node/neighbors/roots/stats/languages/list_tickets/list_relays/get_ticket/validate/ticket_prompt/move_ticket/list_authored_files/read_authored_file/reindex_path/touch + open_rig/close_rig + subscribe_events. SshRpcConfig fields line up with Rig.{host,user,port,key_path,path} so R019-F3 can hand the existing renderer-collected spec straight in. cargo test -p yah-rpc-ssh 8/8 green; cargo build --workspace + cargo build -p yah-tauri both green. Crate depends only on yah-kg + tokio + serde — no SSH library dep. Choice rationale: shelling out reuses the user's existing ~/.ssh/config + agent without translation; if/when russh is needed (channel multiplexing, sftp, programmatic auth retry) only spawn_ssh internal helper changes. R019-T1 (yah-rpc extraction) can lift the rpc::* re-exports out of yah-kg without touching this crate's call sites — the client only depends on the typed shapes.")
//! @yah:next("R019-F3 (RigBackend enum): add yah-rpc-ssh = { path = \"../../yah-rpc-ssh\" } to app/tauri/Cargo.toml; in app/tauri/src/state.rs replace RigEntry.svc: Arc<KgService> with backend: RigBackend (enum Local(Arc<KgService>) / Remote(SshRpcClient)); RigBackend exposes the same async surface as KgService — every arch_* in commands.rs dispatches via match { Local(svc) => svc.method(...), Remote(c) => c.method(...).map_err(|e| e.to_string()) }")
//! @yah:next("R019-T5 (Connect-remote-rig modal): once F3 lands, replace the early-return in app/tauri/src/commands.rs:130 (the 'Remote rig activation isn't wired yet' branch) with: build SshRpcConfig from rig.{host,user,port,key_path,path} -> SshRpcClient::new(cfg), call client.open_rig().await for the boot summary, store it on the RigEntry. The placeholder Arc<KgService> in attach_remote_rig becomes irrelevant — drop it as part of F3.")
//! @yah:next("Optional polish (R019-T5 follow-up): SshRpcClient::ensure_connected() exists for the renderer's 'Test connection' button — a Test affordance in ConnectRemoteRigModal can call rpc.rigTestRemote(spec) -> a new Tauri command that constructs an ephemeral SshRpcClient, calls ensure_connected().await, and reports back true/error string. Skip until users ask.")
//! @yah:next("Optional: when R019-T1 lifts yah_kg::rpc::* into a yah-rpc crate, bump yah-rpc-ssh's dep from yah-kg -> yah-rpc; the import lines in client.rs are the only churn — test infra in session.rs uses serde_json::Value + ArchEvent (which stays in yah-kg::event) so it's unaffected.")
//! @yah:next("Sharp edge: SshRpcClient::subscribe_events binds to whichever JsonRpcSession is live at subscribe time. After a reconnect the old stream silently goes inert; subscribers must resubscribe (same contract KgService::subscribe has after a daemon restart). If F3 wants smoother UX it can wrap the subscribe call in a re-resubscribe loop tied to a heartbeat task.")
//! @yah:verify("cargo test -p yah-rpc-ssh")
//! @yah:verify("cargo build -p yah-tauri")
//!
//! @yah:ticket(R017-T8, "Lift relay-status + lastModifiedTs derivation into the daemon")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R017)
//! @yah:next("yah-ui already derives relay status from children (active>handoff>review+open=handoff>open>review) and rolls up lastModifiedTs as max over descendants — see yah-ui/src/lib/relay-status.ts")
//! @yah:next("Lift the same logic into yah-kg-daemon's list_relays so CLI (yah board), MCP tools, and any non-UI consumer see the same derived view as the desktop app")
//! @yah:next("Source @yah:status on relays becomes display-only once children exist; consider whether yah board open --kind relay should stop writing it for new relays")
//! @yah:next("Reject yah board move + arch_move_ticket on relays-with-children with a Conflict — relay status is no longer user-input")
//! @yah:next("Verify: cargo test -p yah-kg-daemon (existing list_relays e2e tests should still pass)")
//!
//! @yah:relay(R033, "Files tab: Monaco viewer/editor + server file surface + LSP")
//! @yah:status(open)
//! @arch:see(architecture/yah-files-tab.md)
//! @arch:see(architecture/rig-backend-dispatch.md)
//!
//! @yah:ticket(R033-T1, "file.read RPC: range support + 5MB soft cap")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:verify("cargo test -p yah-kg-daemon --test e2e file_read")
//! @yah:verify("cargo build -p yah-tauri")
//!
//! @yah:ticket(R033-T2, "dir.list RPC: one-shot directory listing")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T3, "file.watch + dir.watch RPC: notify-backed subscriptions")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T4, "file.write RPC: optimistic concurrency via mtime check")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:verify("cargo test -p yah-kg-daemon --test e2e file_write")
//! @yah:verify("cargo build -p yah-tauri")
//!
//! @yah:ticket(R033-F11, "yah-lsp crate: child process pool + language detection + framing")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P4)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:handoff("yah-lsp crate landed at yah-lsp/. Public surface: framing (Content-Length read/write, 32MB cap, tolerates bare-LF + extra headers), language (LanguageId/ServerKind + ext detect: rs->rust-analyzer, ts/tsx/js/mjs/cjs/jsx->typescript-language-server), server (LanguageServer w/ Arc<Inner> JSON-RPC multiplex over LSP framing — request/notify/forward_request/shutdown — fans server-pushed notifications via broadcast; build_initialize_params helper), pool (LspPool keyed by (rig_root, ServerKind), lazy spawn, double-checked under-lock to avoid double-spawn race, runs initialize+initialized handshake on first touch, shutdown_rig + shutdown_all for per-rig + global teardown, CommandOverrides slot for the v1.5 ~/.yah/config.toml follow-up). 25/25 unit tests green: framing round-trip + EOF + bare-LF + missing-Content-Length + size-cap, language detection, multiplex round-trip + concurrent + transport-closed + server-error + forward_request id preservation, pool unknown-extension/unknown-path/spawn-failure-propagates/shutdown-rig-idempotent. Workspace Cargo.toml updated (members + default-members). cargo build --workspace green; cargo test -p yah-lsp -p yah-kg-daemon green.")
//! @yah:next("R033-T12 picks this up: add lsp.request/lsp.notification methods on yah serve --stdio (yah/src/serve.rs) — single LspPool per process, route { server, method, params } via LspPool::for_language(rig_root, LanguageId::parse(server)) then forward_request, surface server→client notifications by spawning a per-server task subscribed via subscribe_notifications and emitting them as 'lsp:notification' frames mirroring how arch:event is emitted today.")
//! @yah:next("Server→client requests (workspace/configuration, window/workDoneProgress/create) are deliberately ignored in v1 — server.rs logs them at debug level. If R033-T13's vscode-languageclient ends up requiring them, add a callback hook on LanguageServer::spawn that the multiplex layer wires to round-trip the request through the renderer.")
//! @yah:next("RigEntry::Drop in app/tauri/src/state.rs needs to call LspPool::shutdown_rig(rig_root).await once R033-T12 lands and the pool is wired into the daemon's KgService — flagged in yah-files-tab.md 'LSP per-rig lifecycle'.")
//! @yah:verify("cargo test -p yah-lsp")
//! @yah:verify("cargo build --workspace")
//!
//! @yah:ticket(R033-T12, "lsp.request + lsp.notification: multiplex on yah serve --stdio")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)

use crate::path::{canonicalize_root, is_eligible, relativize};
use crate::snapshot::{
    default_snapshot_path, diff_fingerprints, fingerprint_rig, read_snapshot, write_snapshot,
    FileFingerprint, KgSnapshot, ReconcilePlan, SnapshotError, SNAPSHOT_VERSION,
};
use crate::watcher::{spawn_watcher, WatcherHandle, WatcherKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use yah_kg::anno::AnnotationKind;
use yah_kg::edge::EdgeKind;
use yah_kg::event::{ArchEvent, FileEvent, FileEventKind, IndexReason, IndexScope};
use yah_kg::ids::{NodeFull, NodeId};
use yah_kg::indexer::IndexError;
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg::board_mutate::{
    allowed_transitions, bucket_to_status, locate_ticket_block, rewrite_status_in_source,
    status_to_bucket, touch_in_source, MutateError,
};
use yah_kg::rpc::{
    ArchiveTicketParams, ArchiveTicketResult, AssemblePreludeParams, AssemblePreludeResult,
    AuthoredFile, DirEntry, DirEntryKind, DirListParams, DirListResult, DirWatchParams, Direction,
    FileEncoding, FileReadParams, FileReadResult, FileWatchParams, FileWriteParams,
    FileWriteResult, GetTicketParams,
    GetTicketResult, ListAuthoredFilesParams, ListAuthoredFilesResult, ListRelaysParams,
    ListRelaysResult, ListTicketsParams, ListTicketsResult, LookupParams, LookupResult,
    MoveTicketParams, MoveTicketResult, NeighborsParams, NeighborsResult, ReadAuthoredFileParams,
    ReadAuthoredFileResult, RootsParams, RootsResult, StatsResult, Subgraph, SubgraphParams,
    TicketPromptParams, TicketPromptResult, UnwatchParams, UnwatchResult, ValidateParams,
    ValidateResult, WatchResult, WorkItem, WorkItemAnchor,
};
use yah_kg_anno::{apply_pass, AnnotationIndex, TouchedWorkItem, WorkItemType};
use yah_kg_store::{reindex_file, walk_and_index, IndexerRegistry, Store, WalkSummary};
use yah_kg_validator::validate as run_validator;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("daemon not booted — call boot(rig_root) first")]
    NotBooted,
    #[error("indexer error: {0}")]
    Index(#[from] IndexError),
    #[error("watcher error: {0}")]
    Watcher(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
    /// Domain-level rejection: the requested mutation is structurally
    /// valid but disallowed by the board's rules (epic status is
    /// derived, transition not on the allowed matrix, ticket not on the
    /// board, …). Analogous to HTTP 409 from the board server.
    #[error("conflict: {0}")]
    Conflict(String),
}

/// Tunable knobs. Defaults are sensible for an interactive Tauri host.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Capacity of the broadcast channel. Slow subscribers that fall behind
    /// will see `RecvError::Lagged` rather than back-pressuring publishers.
    pub event_capacity: usize,
    /// Default `node_limit` for `subgraph` queries when the caller doesn't
    /// supply one.
    pub default_subgraph_limit: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            event_capacity: 1024,
            default_subgraph_limit: 2_000,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Booted {
    rig_root: PathBuf,
    last_index_ms: Option<u64>,
}

/// One active `file.watch` / `dir.watch` registration. `root` is a
/// canonicalized absolute path under the rig; `recursive` is true for
/// `dir.watch`. The daemon iterates the registry on every debounced
/// notify batch and emits a `FileEvent` for each handle whose root
/// covers the changed path.
#[derive(Debug, Clone)]
struct WatchEntry {
    root: PathBuf,
    recursive: bool,
}

#[derive(Debug, Default)]
struct WatchRegistry {
    next_id: u64,
    entries: HashMap<u64, WatchEntry>,
}

impl WatchRegistry {
    fn insert(&mut self, entry: WatchEntry) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        self.entries.insert(id, entry);
        id
    }

    fn remove(&mut self, id: u64) -> bool {
        self.entries.remove(&id).is_some()
    }
}

pub struct KgService {
    store: Arc<RwLock<Store>>,
    annotations: Arc<RwLock<AnnotationIndex>>,
    /// Per-file fingerprints captured the last time the rig was walked.
    /// Snapshot save/load round-trips this; boot_with_snapshot uses it
    /// to compute which files to reindex.
    fingerprints: Arc<RwLock<HashMap<String, FileFingerprint>>>,
    registry: Arc<IndexerRegistry>,
    events: broadcast::Sender<ArchEvent>,
    /// Filesystem-change channel for `file.watch` / `dir.watch`
    /// subscribers. Distinct from the structural `events` stream so
    /// renderers that only want raw FS notifications (e.g. a file tree
    /// component) don't have to filter the much busier ArchEvent stream.
    file_events: broadcast::Sender<FileEvent>,
    /// Active watch handles keyed by id. Cleared on `boot()` (each rig
    /// open starts with no watches; renderers re-register).
    watches: Arc<RwLock<WatchRegistry>>,
    booted: Arc<RwLock<Option<Booted>>>,
    watcher: Arc<RwLock<Option<WatcherHandle>>>,
    config: ServiceConfig,
}

impl KgService {
    pub fn new(registry: IndexerRegistry) -> Self {
        Self::with_config(registry, ServiceConfig::default())
    }

    pub fn with_config(registry: IndexerRegistry, config: ServiceConfig) -> Self {
        let (events, _) = broadcast::channel(config.event_capacity);
        let (file_events, _) = broadcast::channel(config.event_capacity);
        Self {
            store: Arc::new(RwLock::new(Store::new())),
            annotations: Arc::new(RwLock::new(AnnotationIndex::new())),
            fingerprints: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(registry),
            events,
            file_events,
            watches: Arc::new(RwLock::new(WatchRegistry::default())),
            booted: Arc::new(RwLock::new(None)),
            watcher: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Languages for which the daemon has a registered indexer. Mirrors
    /// the `arch.languages` RPC.
    pub fn languages(&self) -> Vec<Lang> {
        self.registry.languages()
    }

    /// Subscribe to the `ArchEvent` stream. Each subscriber gets its own
    /// receiver; events fan out. Drop the receiver to unsubscribe.
    pub fn subscribe(&self) -> broadcast::Receiver<ArchEvent> {
        self.events.subscribe()
    }

    /// Subscribe to the raw filesystem `FileEvent` stream. Distinct from
    /// [`Self::subscribe`] — emits one `FileEvent` per active watch
    /// handle that covers a debounced notify batch path. Drop the
    /// receiver to unsubscribe.
    pub fn subscribe_file_events(&self) -> broadcast::Receiver<FileEvent> {
        self.file_events.subscribe()
    }

    /// Register a `file.watch` handle. Mirrors `file.watch`. The path is
    /// canonicalized and rejected if it escapes the rig root or doesn't
    /// resolve to a regular file.
    pub async fn watch_file(
        &self,
        params: FileWatchParams,
    ) -> Result<WatchResult, DaemonError> {
        let abs = self.resolve_under_rig(&params.path).await?;
        let meta = std::fs::metadata(&abs).map_err(|e| DaemonError::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(DaemonError::Conflict(format!(
                "{} is not a regular file",
                params.path
            )));
        }
        let mut reg = self.watches.write().await;
        let id = reg.insert(WatchEntry {
            root: abs,
            recursive: false,
        });
        Ok(WatchResult { id })
    }

    /// Register a `dir.watch` handle. Mirrors `dir.watch`. Empty `path`
    /// watches the rig root.
    pub async fn watch_dir(
        &self,
        params: DirWatchParams,
    ) -> Result<WatchResult, DaemonError> {
        let abs = self.resolve_under_rig(&params.path).await?;
        let meta = std::fs::metadata(&abs).map_err(|e| DaemonError::Io(e.to_string()))?;
        if !meta.is_dir() {
            return Err(DaemonError::Conflict(format!(
                "{} is not a directory",
                params.path
            )));
        }
        let mut reg = self.watches.write().await;
        let id = reg.insert(WatchEntry {
            root: abs,
            recursive: true,
        });
        Ok(WatchResult { id })
    }

    /// Drop a watch handle. Idempotent — `unwatch` of an unknown id is a
    /// no-op (the wire protocol doesn't surface a Conflict here because
    /// the renderer's mental model is "I don't care if it's already
    /// gone"; e.g. a session disconnected and the daemon GC'd the handle
    /// before the explicit unwatch arrived).
    pub async fn unwatch(
        &self,
        params: UnwatchParams,
    ) -> Result<UnwatchResult, DaemonError> {
        let mut reg = self.watches.write().await;
        let _ = reg.remove(params.id);
        Ok(UnwatchResult::default())
    }

    async fn resolve_under_rig(&self, rel: &str) -> Result<PathBuf, DaemonError> {
        let rig_root = self.rig_root().await?;
        let rig_root_canon = rig_root.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve rig root {}: {}",
                rig_root.display(),
                e
            ))
        })?;
        let trimmed = rel.trim_matches('/');
        let candidate = if trimmed.is_empty() || trimmed == "." {
            rig_root.clone()
        } else {
            rig_root.join(trimmed)
        };
        let candidate_canon = candidate
            .canonicalize()
            .map_err(|e| DaemonError::Conflict(format!("cannot resolve {}: {}", rel, e)))?;
        if !candidate_canon.starts_with(&rig_root_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is outside the rig root",
                rel
            )));
        }
        Ok(candidate_canon)
    }

    /// Cold-start: walk `rig_root` and populate the store. Idempotent —
    /// calling it again rebinds to the new root and emits an
    /// `IndexFinished` event.
    pub async fn boot(&self, rig_root: PathBuf) -> Result<WalkSummary, DaemonError> {
        let canon = canonicalize_root(&rig_root);
        self.send(ArchEvent::IndexStarted {
            reason: IndexReason::Boot,
            scope: IndexScope::All,
        });

        let start = Instant::now();
        let (summary, touched_work_items) = {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            // Wipe existing nodes before booting against a new rig.
            *store = Store::new();
            *anno = AnnotationIndex::new();
            let s = walk_and_index(&canon, &mut store, &self.registry)
                .map_err(DaemonError::Index)?;
            // Pass 4: annotations.
            let apply = apply_pass(&mut store, &mut anno, None);
            (s, apply.touched_work_items)
        };
        let elapsed = start.elapsed().as_millis() as u64;
        emit_work_items(&self.events, &touched_work_items);

        // Capture the fingerprints we just walked over so a follow-up
        // `save()` can reconcile them on the next boot.
        {
            let mut fps = self.fingerprints.write().await;
            *fps = fingerprint_rig(&canon, &self.registry);
        }
        {
            let mut booted = self.booted.write().await;
            *booted = Some(Booted {
                rig_root: canon,
                last_index_ms: Some(elapsed),
            });
        }

        self.send(ArchEvent::IndexFinished {
            duration_ms: elapsed,
            nodes_added: summary.files_indexed, // placeholder counts; full diff comes from reindex_path
            nodes_changed: 0,
            nodes_removed: 0,
            edges_added: 0,
            edges_removed: 0,
        });
        Ok(summary)
    }

    /// Snapshot-aware boot. If `snapshot_path` exists and was produced
    /// against the same `rig_root`, restore in-memory state from it and
    /// then walk the rig once to reindex any file whose mtime + size
    /// have changed (or that disappeared) since the snapshot was
    /// written. Falls back to a full [`Self::boot`] when no usable
    /// snapshot is found.
    ///
    /// Returns a `WalkSummary` whose `files_indexed` count reflects the
    /// number of files the reconcile actually touched — for a clean
    /// snapshot this is `0`, which is the order-of-magnitude speedup
    /// over a fresh boot (R017-F3 verify gate).
    pub async fn boot_with_snapshot(
        &self,
        rig_root: PathBuf,
        snapshot_path: &Path,
    ) -> Result<WalkSummary, DaemonError> {
        let debug_phases = std::env::var("YAH_SNAPSHOT_DEBUG").is_ok();
        let canon = canonicalize_root(&rig_root);
        let phase_read = Instant::now();
        let snap = match read_snapshot(snapshot_path) {
            Ok(s) if s.rig_root == canon => Some(s),
            Ok(_) => None,
            Err(e) => {
                if debug_phases {
                    eprintln!("snapshot rejected: {}", e);
                }
                None
            }
        };
        let Some(snap) = snap else {
            // No usable snapshot: fall back to a full boot. The caller
            // will typically want to `save()` afterwards so the next
            // cold-start has something to replay.
            return self.boot(rig_root).await;
        };
        if debug_phases {
            eprintln!(
                "snapshot phase: read+parse {}ms",
                phase_read.elapsed().as_millis()
            );
        }

        self.send(ArchEvent::IndexStarted {
            reason: IndexReason::Boot,
            scope: IndexScope::All,
        });
        let start = Instant::now();

        // Restore the saved store + annotations.
        let phase_restore = Instant::now();
        {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            *store = Store::new();
            store.restore(snap.store).map_err(SnapshotError::from)?;
            *anno = AnnotationIndex::new();
            anno.restore(snap.annotations);
        }
        if debug_phases {
            eprintln!(
                "snapshot phase: store+anno restore {}ms",
                phase_restore.elapsed().as_millis()
            );
        }
        {
            let mut booted = self.booted.write().await;
            *booted = Some(Booted {
                rig_root: canon.clone(),
                last_index_ms: None,
            });
        }
        // Seed fingerprints from the snapshot before reconcile so
        // `save()` mid-reconcile would still write a coherent file.
        {
            let mut fps = self.fingerprints.write().await;
            *fps = snap.fingerprints.clone();
        }

        // Reconcile against the live filesystem.
        let phase_reconcile = Instant::now();
        let plan = self.reconcile(&canon, &snap.fingerprints).await?;
        if debug_phases {
            eprintln!(
                "snapshot phase: fingerprint+reconcile {}ms",
                phase_reconcile.elapsed().as_millis()
            );
        }
        let touched_files = plan.changed.len() + plan.removed.len();

        let elapsed = start.elapsed().as_millis() as u64;
        if let Some(b) = self.booted.write().await.as_mut() {
            b.last_index_ms = Some(elapsed);
        }
        self.send(ArchEvent::IndexFinished {
            duration_ms: elapsed,
            nodes_added: 0,
            nodes_changed: 0,
            nodes_removed: 0,
            edges_added: 0,
            edges_removed: 0,
        });

        Ok(WalkSummary {
            files_seen: (plan.unchanged.len() + plan.changed.len()) as u32,
            files_indexed: touched_files as u32,
            files_skipped: plan.unchanged.len() as u32,
            parse_errors: 0,
        })
    }

    /// Walk the rig, fingerprint every indexable file, and reindex any
    /// file whose fingerprint differs from `prev_fps` (or that vanished).
    /// Updates `self.fingerprints` to the fresh set on success.
    async fn reconcile(
        &self,
        rig_root: &Path,
        prev_fps: &HashMap<String, FileFingerprint>,
    ) -> Result<ReconcilePlan, DaemonError> {
        let fresh = fingerprint_rig(rig_root, &self.registry);
        let plan = diff_fingerprints(prev_fps, &fresh);

        for rel in &plan.changed {
            let abs = rig_root.join(rel);
            self.reindex_path(&abs, IndexReason::Boot).await?;
        }
        for rel in &plan.removed {
            let abs = rig_root.join(rel);
            // reindex_path on a missing file wipes its nodes.
            self.reindex_path(&abs, IndexReason::Boot).await?;
        }

        // Record the fresh fingerprints regardless of whether anything
        // changed — `unchanged` files still need to round-trip into the
        // next snapshot.
        {
            let mut fps = self.fingerprints.write().await;
            *fps = fresh;
        }
        Ok(plan)
    }

    /// Capture the daemon's current state as a [`KgSnapshot`] and write
    /// it to `path`. Atomic on success (write-temp + rename).
    pub async fn save(&self, path: &Path) -> Result<(), DaemonError> {
        let rig_root = self.rig_root().await?;
        let store_snap = self.store.read().await.to_snapshot();
        let anno_snap = self.annotations.read().await.to_snapshot();
        let fingerprints = self.fingerprints.read().await.clone();
        let snap = KgSnapshot {
            version: SNAPSHOT_VERSION,
            rig_root,
            fingerprints,
            store: store_snap,
            annotations: anno_snap,
        };
        write_snapshot(path, snap).map_err(DaemonError::from)
    }

    /// Convenience: write to the conventional
    /// `<rig_root>/.yah/cache/snapshot.bin`.
    pub async fn save_default(&self) -> Result<(), DaemonError> {
        let rig_root = self.rig_root().await?;
        let path = default_snapshot_path(&rig_root);
        self.save(&path).await
    }

    /// Replace the daemon's in-memory state with the contents of a
    /// snapshot file. Does **not** reconcile against the filesystem —
    /// callers that want mtime-diff should use [`Self::boot_with_snapshot`]
    /// instead. Useful in tests and for tooling that wants to inspect
    /// a saved graph without booting against a live rig.
    pub async fn load(&self, path: &Path) -> Result<(), DaemonError> {
        let snap = read_snapshot(path).map_err(DaemonError::from)?;
        let canon = snap.rig_root.clone();
        {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            *store = Store::new();
            store.restore(snap.store).map_err(SnapshotError::from)?;
            *anno = AnnotationIndex::new();
            anno.restore(snap.annotations);
        }
        {
            let mut fps = self.fingerprints.write().await;
            *fps = snap.fingerprints;
        }
        {
            let mut booted = self.booted.write().await;
            *booted = Some(Booted {
                rig_root: canon,
                last_index_ms: None,
            });
        }
        Ok(())
    }

    /// Start a `notify` watcher rooted at the booted rig. No-op if a
    /// watcher is already running.
    pub async fn start_watching(&self) -> Result<(), DaemonError> {
        let rig_root = self.rig_root().await?;

        let mut slot = self.watcher.write().await;
        if slot.is_some() {
            return Ok(());
        }

        let store = Arc::clone(&self.store);
        let annotations = Arc::clone(&self.annotations);
        let registry = Arc::clone(&self.registry);
        let events = self.events.clone();
        let file_events = self.file_events.clone();
        let watches = Arc::clone(&self.watches);
        let root_for_task = rig_root.clone();

        let handle = spawn_watcher(
            WatcherKind::Recursive,
            rig_root.clone(),
            move |abs_paths| {
                let store = Arc::clone(&store);
                let annotations = Arc::clone(&annotations);
                let registry = Arc::clone(&registry);
                let events = events.clone();
                let file_events = file_events.clone();
                let watches = Arc::clone(&watches);
                let root = root_for_task.clone();
                Box::pin(async move {
                    fan_file_events(&abs_paths, &root, &watches, &file_events).await;
                    apply_paths(abs_paths, &root, &store, &annotations, &registry, &events).await;
                })
            },
        )
        .await
        .map_err(|e| DaemonError::Watcher(e.to_string()))?;

        *slot = Some(handle);
        Ok(())
    }

    /// Stop the watcher if running. Idempotent.
    pub async fn stop_watching(&self) {
        let mut slot = self.watcher.write().await;
        if let Some(handle) = slot.take() {
            handle.stop().await;
        }
    }

    /// Manual reindex of one absolute path. Most callers use the watcher;
    /// this is the entry point pi-mono will call after an agent edit so
    /// the resulting `ArchEvent`s carry `IndexReason::AgentEdit`.
    pub async fn reindex_path(
        &self,
        abs_path: &Path,
        reason: IndexReason,
    ) -> Result<(), DaemonError> {
        let rig_root = self.rig_root().await?;
        let Some(rel) = relativize(abs_path, &rig_root) else {
            return Ok(());
        };
        if !is_eligible(Path::new(&rel)) {
            return Ok(());
        }

        self.send(ArchEvent::IndexStarted {
            reason,
            scope: IndexScope::Files {
                paths: vec![rel.clone()],
            },
        });
        let start = Instant::now();

        let (delta, touched_work_items) = {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            let d = reindex_file(&rig_root, &rel, &mut store, &self.registry)
                .map_err(DaemonError::Index)?;
            // Drop annotations for any nodes that vanished.
            for id in &d.nodes_removed {
                anno.remove(*id);
            }
            // Re-run Pass 4 over the file's surviving + new nodes only.
            let scope: std::collections::HashSet<_> = store
                .lookup(&rel, None)
                .into_iter()
                .collect();
            let apply = apply_pass(&mut store, &mut anno, Some(&scope));
            (d, apply.touched_work_items)
        };

        emit_delta(&self.events, &delta);
        emit_work_items(&self.events, &touched_work_items);

        self.send(ArchEvent::IndexFinished {
            duration_ms: start.elapsed().as_millis() as u64,
            nodes_added: delta.nodes_added.len() as u32,
            nodes_changed: delta.nodes_changed.len() as u32,
            nodes_removed: delta.nodes_removed.len() as u32,
            edges_added: delta.edges_added.len() as u32,
            edges_removed: delta.edges_removed.len() as u32,
        });

        if let Some(booted) = self.booted.write().await.as_mut() {
            booted.last_index_ms = Some(start.elapsed().as_millis() as u64);
        }
        Ok(())
    }

    /// Emit an `AgentTouch` event: pi-mono ran a tool over `paths` (with
    /// optional `:line` suffixes) and we resolve those to NodeIds via the
    /// store's lookup index. Subscribers (the UI) candle-pulse the touched
    /// nodes.
    pub async fn touch(&self, paths: &[String], tool: &str, relay: &str) {
        let store = self.store.read().await;
        let mut ids: Vec<NodeId> = Vec::new();
        for p in paths {
            let (file, line) = parse_path_line(p);
            for id in store.lookup(file, line) {
                ids.push(id);
            }
        }
        drop(store);
        if ids.is_empty() {
            return;
        }
        self.send(ArchEvent::AgentTouch {
            ids,
            tool: tool.to_string(),
            relay: relay.to_string(),
            ts: now_ms(),
        });
    }

    // ---------- Read queries (mirror arch.* RPC) ----------

    pub async fn subgraph(&self, params: SubgraphParams) -> Subgraph {
        let store = self.store.read().await;
        store.subgraph(
            params.root,
            params.depth,
            params.edges.as_deref(),
            params.kinds.as_deref(),
            params.langs.as_deref(),
            Some(
                params
                    .node_limit
                    .unwrap_or(self.config.default_subgraph_limit),
            ),
        )
    }

    pub async fn lookup(&self, params: LookupParams) -> LookupResult {
        let store = self.store.read().await;
        LookupResult {
            ids: store.lookup(&params.file, params.line),
        }
    }

    pub async fn node(&self, id: NodeId) -> Option<NodeFull> {
        let store = self.store.read().await;
        let mut full = store.node_full(id)?;
        // Splice in the annotation overlay so a single arch.node call gives
        // the UI the full picture.
        let anno = self.annotations.read().await;
        full.annotations = anno.get(id).to_vec();
        Some(full)
    }

    pub async fn neighbors(&self, params: NeighborsParams) -> NeighborsResult {
        let store = self.store.read().await;
        NeighborsResult {
            edges: store.neighbors(params.id, params.dir, params.edges.as_deref()),
        }
    }

    pub async fn roots(&self, params: RootsParams) -> RootsResult {
        let store = self.store.read().await;
        let langs = params.lang.as_ref().map(|l| vec![*l]);
        let kinds = params.kind.clone().map(|k| vec![k]);
        RootsResult {
            roots: store.roots(langs.as_deref(), kinds.as_deref()),
        }
    }

    /// Enumerate every synthetic Ticket node currently in the graph,
    /// along with its source anchors and parsed payload. Stub-only nodes
    /// (referenced as a parent but never authored) are filtered out —
    /// they have no anchors and no payload, so they'd just be noise.
    pub async fn list_tickets(&self, _params: ListTicketsParams) -> ListTicketsResult {
        let rig_root = self.rig_root().await.ok();
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let tickets =
            collect_work_items(&store, &anno, WorkItemType::Ticket, rig_root.as_deref());
        ListTicketsResult { tickets }
    }

    /// Same shape as `list_tickets`, scoped to synthetic Relay nodes.
    ///
    /// Relay `status` and `last_modified_ts` are *derived* from the relay's
    /// children (and their descendants) before the result goes out — once a
    /// relay has children the source-authored `@yah:status(...)` is
    /// display-only. Mirrors `yah-ui/src/lib/relay-status.ts` so the CLI,
    /// MCP tools, and any non-UI consumer see the same view as the desktop
    /// app. See [`yah_kg::board::apply_derived_relay_fields`] for the rules.
    pub async fn list_relays(&self, _params: ListRelaysParams) -> ListRelaysResult {
        let rig_root = self.rig_root().await.ok();
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let mut relays =
            collect_work_items(&store, &anno, WorkItemType::Relay, rig_root.as_deref());
        let tickets =
            collect_work_items(&store, &anno, WorkItemType::Ticket, rig_root.as_deref());
        yah_kg::board::apply_derived_relay_fields(&mut relays, &tickets);
        ListRelaysResult { relays }
    }

    /// Look up one synthetic Ticket node by its bare ID (e.g. `R042-T1`,
    /// not `ticket:R042-T1`). Returns `None` if no Ticket node bears that
    /// id — including the case where a stub-only node exists for a
    /// referenced-but-unauthored ticket id.
    pub async fn get_ticket(&self, params: GetTicketParams) -> GetTicketResult {
        let rig_root = self.rig_root().await.ok();
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let qualified = format!("ticket:{}", params.id);
        let ticket = store
            .all_node_refs()
            .find(|n| {
                matches!(n.kind, NodeKind::Common(CommonKind::Ticket)) && n.qualified == qualified
            })
            .and_then(|n| {
                build_work_item(&store, &anno, n, WorkItemType::Ticket, rig_root.as_deref())
            });
        GetTicketResult { ticket }
    }

    /// Run the rule validator across the requested scope. Mirrors the
    /// `arch.validate` RPC. Each `@yah:rule(...)` annotation in source
    /// becomes zero or more [`yah_kg::validate::Violation`]s — parse and
    /// vocabulary errors surface as violations rather than aborting the
    /// run, so a single bad rule never hides the rest of the report.
    pub async fn validate(&self, params: ValidateParams) -> ValidateResult {
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let violations = run_validator(&store, &anno, params.scope);
        ValidateResult { violations }
    }

    /// Render the canonical pickup or review markdown for one work-item id.
    /// Mirrors the `arch.ticket_prompt` RPC. Returns `markdown: None` when
    /// no work-item bears the id (callers surface that as a transient miss
    /// rather than throwing).
    ///
    /// Builds a [`yah_kg::board::Board`] from the daemon's current relay /
    /// ticket state and dispatches to [`yah_kg::prompt::render`] — the
    /// same renderer the CLI's `yah board show <id> --prompt` flows
    /// through, so the two cannot drift on prompt shape.
    pub async fn ticket_prompt(&self, params: TicketPromptParams) -> TicketPromptResult {
        let rig_root = self.rig_root().await.ok();
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let relays = collect_work_items(&store, &anno, WorkItemType::Relay, rig_root.as_deref());
        let tickets = collect_work_items(&store, &anno, WorkItemType::Ticket, rig_root.as_deref());
        let board = yah_kg::board::Board::from_work_items(relays, tickets);
        let markdown = yah_kg::prompt::render(&board, &params.id, params.mode);
        TicketPromptResult { markdown }
    }

    /// Build the per-ticket cached prefix the agent runtime injects into
    /// the system prompt every turn (R028). Combines the daemon's three
    /// inputs to [`yah_kg::prelude::assemble`]:
    ///
    /// 1. The board (recomputed from current relay/ticket nodes — same
    ///    shape `ticket_prompt` uses).
    /// 2. A KG slice rooted at the ticket's primary structural anchor,
    ///    bounded by `params.kg_depth` (default `2`) so the prelude
    ///    doesn't grow without limit on a deep walk.
    /// 3. Bodies of any `@arch:see` doc paths small enough to inline —
    ///    read from the rig root with a 4×`arch_inline_max_bytes` read
    ///    cap so a runaway file can't blow the daemon's working set.
    ///
    /// `result.prelude` is `None` when no work-item bears `params.id`
    /// (mirrors `arch.get_ticket` / `arch.ticket_prompt`'s null-when-
    /// missing convention).
    pub async fn assemble_prelude(&self, params: AssemblePreludeParams) -> AssemblePreludeResult {
        let rig_root = self.rig_root().await.ok();

        let (board, workspace_policy) = {
            let store = self.store.read().await;
            let anno = self.annotations.read().await;
            let relays =
                collect_work_items(&store, &anno, WorkItemType::Relay, rig_root.as_deref());
            let tickets =
                collect_work_items(&store, &anno, WorkItemType::Ticket, rig_root.as_deref());
            let board = yah_kg::board::Board::from_work_items(relays, tickets);
            let workspace_policy = collect_workspace_agent_policy(&anno);
            (board, workspace_policy)
        };

        let Some(item) = board.get(&params.id) else {
            return AssemblePreludeResult { prelude: None };
        };

        let mut options = yah_kg::prelude::PreludeOptions::default();
        if let Some(t) = params.max_tokens {
            options.max_tokens = t;
        }
        if let Some(n) = params.kg_node_limit {
            options.kg_node_limit = n;
        }
        if let Some(b) = params.arch_inline_max_bytes {
            options.arch_inline_max_bytes = b as usize;
        }
        let depth = params.kg_depth.unwrap_or(2);

        let kg_slice = if let Some(anchor) = item.item.anchors.first() {
            let store = self.store.read().await;
            Some(store.subgraph(
                anchor.node,
                depth,
                None,
                None,
                None,
                Some(options.kg_node_limit),
            ))
        } else {
            None
        };

        let arch_docs = load_arch_docs(
            rig_root.as_deref(),
            &item.item.anno.see_also,
            options.arch_inline_max_bytes,
        );

        let inputs = yah_kg::prelude::PreludeInputs {
            ticket_id: &params.id,
            board: &board,
            kg_slice: kg_slice.as_ref(),
            arch_docs: &arch_docs,
            // R028-F5 will populate this from column/tag → skill rules.
            skills: &[],
            // Free-floating `@yah:rule(agent-...)` annotations collected
            // from the AnnotationIndex — see [`collect_workspace_agent_policy`].
            workspace_policy: &workspace_policy,
            options,
        };
        AssemblePreludeResult {
            prelude: yah_kg::prelude::assemble(&inputs),
        }
    }

    /// Rewrite a ticket's `@yah:status(...)` line in source to mirror a
    /// column drag-and-drop in the UI. Validates the transition matrix
    /// (open→active, active→{open,handoff,review}, handoff→{active,
    /// review}, review→handoff), refuses epic mutation (epic status is
    /// computed from children), then writes the source file at the
    /// lex-first anchor and triggers an `IndexReason::AgentEdit` reindex
    /// so subscribers see the resulting `WorkItemChanged` event with the
    /// right cause.
    ///
    /// `to_bucket` is the column the renderer dropped onto: `open`,
    /// `active`, `handoff`, or `review` — the bucket→status mapping
    /// lives on the daemon (see [`yah_kg::board_mutate::bucket_to_status`])
    /// so the UI doesn't have to know about `in-progress` vs `claimed`
    /// vs `done`.
    pub async fn move_ticket(
        &self,
        params: MoveTicketParams,
    ) -> Result<MoveTicketResult, DaemonError> {
        let rig_root = self.rig_root().await?;

        let to_bucket = params.to_bucket.to_lowercase();
        let new_status = bucket_to_status(&to_bucket).ok_or_else(|| {
            DaemonError::Conflict(format!(
                "unknown target column '{}' (expected: open | active | handoff | review)",
                params.to_bucket
            ))
        })?;

        // Find the ticket and its lex-first anchor while holding read locks.
        let (anchor_file_rel, anchor_line, current_status_str, is_epic, is_relay_with_children) = {
            let store = self.store.read().await;
            let anno = self.annotations.read().await;
            let relays = collect_work_items(
                &store,
                &anno,
                WorkItemType::Relay,
                Some(rig_root.as_path()),
            );
            let tickets = collect_work_items(
                &store,
                &anno,
                WorkItemType::Ticket,
                Some(rig_root.as_path()),
            );
            let board = yah_kg::board::Board::from_work_items(relays, tickets);
            let item = board.get(&params.id).ok_or_else(|| {
                DaemonError::Conflict(format!(
                    "ticket '{}' is not on the board",
                    params.id
                ))
            })?;
            let anchor = item.item.anchors.first().ok_or_else(|| {
                DaemonError::Conflict(format!(
                    "ticket '{}' has no source anchor",
                    params.id
                ))
            })?;
            let current = anchor
                .anno
                .status
                .as_ref()
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| "open".to_string());
            // A relay with any children — bare-R epic children OR
            // compound sub-tickets — has its status derived (see
            // `apply_derived_relay_fields`). Direct mutation would
            // silently revert on the next list_relays.
            let has_children = matches!(item.item.item_type, WorkItemType::Relay)
                && board.children_of(&params.id).next().is_some();
            (
                anchor.file.clone(),
                anchor.line,
                current,
                item.is_epic,
                has_children,
            )
        };

        if is_epic {
            return Err(DaemonError::Conflict(format!(
                "'{}' is an epic; status is derived from children and cannot be set directly",
                params.id
            )));
        }
        if is_relay_with_children {
            return Err(DaemonError::Conflict(format!(
                "'{}' is a relay with children; status is derived from sub-tickets and cannot be set directly. Move the children to drive the relay's column.",
                params.id
            )));
        }

        let from_bucket = status_to_bucket(&current_status_str);
        if from_bucket.is_empty() {
            return Err(DaemonError::Conflict(format!(
                "ticket '{}' has unrecognized status '{}'",
                params.id, current_status_str
            )));
        }

        if from_bucket != to_bucket {
            let allowed = allowed_transitions(from_bucket);
            if !allowed.contains(&to_bucket.as_str()) {
                return Err(DaemonError::Conflict(format!(
                    "transition {} → {} is not allowed (allowed targets: {:?})",
                    from_bucket, to_bucket, allowed
                )));
            }
        }

        let abs_path = rig_root.join(&anchor_file_rel);
        let original = std::fs::read_to_string(&abs_path).map_err(|e| {
            DaemonError::Io(format!("read {}: {}", abs_path.display(), e))
        })?;

        let after_status = match rewrite_status_in_source(&original, &params.id, new_status) {
            Ok(s) => s,
            Err(MutateError::NotFound { id }) => {
                return Err(DaemonError::Conflict(format!(
                    "ticket '{}' anchor at {} no longer carries an @yah:ticket/@yah:relay declaration — file may have been edited mid-flight",
                    id, anchor_file_rel
                )));
            }
        };

        // Stamp `@yah:at(now)` only when the move actually changed
        // status — dragging a card to its current column is a no-op
        // and shouldn't bump the per-ticket timestamp.
        let new_content = if after_status != original {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let now_rfc = yah_kg::timefmt::format_rfc3339(now_secs);
            touch_in_source(&after_status, &params.id, &now_rfc).unwrap_or(after_status)
        } else {
            after_status
        };

        if new_content != original {
            std::fs::write(&abs_path, &new_content).map_err(|e| {
                DaemonError::Io(format!("write {}: {}", abs_path.display(), e))
            })?;
            self.reindex_path(&abs_path, IndexReason::AgentEdit).await?;
        }

        Ok(MoveTicketResult {
            id: params.id,
            from_status: current_status_str,
            to_status: new_status.to_string(),
            file: anchor_file_rel,
            line: anchor_line,
        })
    }

    /// Strip a ticket's `@yah:*` annotation lines from source and append
    /// an `archived` event to its per-relay shard at
    /// `.yah/events/<shard>.jsonl`. The event carries a full
    /// [`WorkItem`] snapshot plus the stripped source lines so the
    /// audit log alone is enough to rehydrate the ticket later.
    ///
    /// Sub-tickets share their parent's shard (`R007-T1` → `R007.jsonl`).
    /// Non-`@yah:` lines in the same doc-comment block (notably
    /// `@arch:`) are preserved.
    ///
    /// Validation mirrors `yah board archive`:
    /// - `claimed` / `in-progress` rejected (move to review/handoff first)
    /// - epics with live children rejected
    pub async fn archive_ticket(
        &self,
        params: ArchiveTicketParams,
    ) -> Result<ArchiveTicketResult, DaemonError> {
        let rig_root = self.rig_root().await?;

        // Look up the ticket and pull the snapshot we'll embed in the
        // event. Read locks released before any disk I/O.
        let (snapshot, anchor_file_rel, anchor_line, current_status, is_epic, has_children) = {
            let store = self.store.read().await;
            let anno = self.annotations.read().await;
            let relays = collect_work_items(
                &store,
                &anno,
                WorkItemType::Relay,
                Some(rig_root.as_path()),
            );
            let tickets = collect_work_items(
                &store,
                &anno,
                WorkItemType::Ticket,
                Some(rig_root.as_path()),
            );
            let board = yah_kg::board::Board::from_work_items(relays, tickets);
            let item = board.get(&params.id).ok_or_else(|| {
                DaemonError::Conflict(format!("ticket '{}' is not on the board", params.id))
            })?;
            let anchor = item.item.anchors.first().ok_or_else(|| {
                DaemonError::Conflict(format!("ticket '{}' has no source anchor", params.id))
            })?;
            let current = anchor
                .anno
                .status
                .as_ref()
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| "open".to_string());
            let has_children = matches!(item.item.item_type, WorkItemType::Relay)
                && board.children_of(&params.id).next().is_some();
            (
                item.item.clone(),
                anchor.file.clone(),
                anchor.line,
                current,
                item.is_epic,
                has_children,
            )
        };

        // Active-state guard: claimed/in-progress must move to a
        // terminal-ish bucket (review/handoff) before archiving.
        if current_status == "claimed" || current_status == "in-progress" {
            return Err(DaemonError::Conflict(format!(
                "cannot archive '{}' — ticket is {}. Move to review or handoff first.",
                params.id, current_status
            )));
        }

        // Epic guard: refuse to archive an epic while children remain.
        if is_epic && has_children {
            return Err(DaemonError::Conflict(format!(
                "epic '{}' has live child relays — archive each child first",
                params.id
            )));
        }

        let abs_path = rig_root.join(&anchor_file_rel);
        let original = std::fs::read_to_string(&abs_path).map_err(|e| {
            DaemonError::Io(format!("read {}: {}", abs_path.display(), e))
        })?;

        let block = locate_ticket_block(&original, &params.id).ok_or_else(|| {
            DaemonError::Conflict(format!(
                "ticket '{}' anchor at {} no longer carries an @yah:ticket/@yah:relay declaration — file may have been edited mid-flight",
                params.id, anchor_file_rel
            ))
        })?;

        // Strip `@yah:` lines within the block. Leaves `@arch:` lines
        // and surrounding non-hack doc text untouched — same shape as
        // `yah board archive` and the legacy hack-board server's
        // stripHackAnnotations.
        let is_hack_line = |l: &str| -> bool {
            let t = l.trim_start();
            let body = if let Some(rest) = t.strip_prefix("//!") {
                rest
            } else if let Some(rest) = t.strip_prefix("///") {
                rest
            } else if let Some(rest) = t.strip_prefix('#') {
                rest
            } else {
                return false;
            };
            body.trim_start().starts_with("@yah:")
        };
        let mut removed: Vec<String> = Vec::new();
        let mut kept: Vec<String> = Vec::new();
        for (i, line) in original.split('\n').enumerate() {
            let line_num = i + 1;
            if line_num >= block.start_line && line_num <= block.end_line && is_hack_line(line) {
                removed.push(line.to_string());
            } else {
                kept.push(line.to_string());
            }
        }
        if removed.is_empty() {
            return Err(DaemonError::Conflict(format!(
                "no @yah: annotations found at {}:{}",
                anchor_file_rel, block.decl_line
            )));
        }
        let new_content = kept.join("\n");

        // Write archive event before mutating source so a crash
        // mid-archive leaves the audit log intact (the source still
        // owns the ticket; a fresh replay will see no archived event
        // mismatch). Ordering follows yah/src/main.rs::handle_archive.
        let shard = shard_for(&params.id);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let event = serde_json::json!({
            "t": now,
            "type": "archived",
            "id": &params.id,
            "ticket": &snapshot,
            "sourceLines": &removed,
            "file": &anchor_file_rel,
            "line": block.decl_line,
        });
        append_archive_event(&rig_root, shard, &event).map_err(|e| {
            DaemonError::Io(format!(
                "append .yah/events/{}.jsonl: {}",
                shard, e
            ))
        })?;

        std::fs::write(&abs_path, &new_content).map_err(|e| {
            DaemonError::Io(format!("write {}: {}", abs_path.display(), e))
        })?;
        self.reindex_path(&abs_path, IndexReason::AgentEdit).await?;

        Ok(ArchiveTicketResult {
            id: params.id,
            file: anchor_file_rel,
            line: anchor_line,
            removed_lines: removed.len() as u32,
        })
    }

    /// Enumerate `.mmd` and `.md` files under `<rig_root>/.yah/arch/authored/`.
    /// Mirrors `arch.list_authored_files`. Missing directory → empty
    /// list (a fresh rig without any authored diagrams is a normal
    /// state, not an error). Walks recursively; nested subfolders
    /// surface as `name = "subdir/foo"`.
    pub async fn list_authored_files(
        &self,
        _params: ListAuthoredFilesParams,
    ) -> Result<ListAuthoredFilesResult, DaemonError> {
        let rig_root = self.rig_root().await?;
        let sandbox = rig_root.join(".yah").join("arch").join("authored");
        if !sandbox.exists() {
            return Ok(ListAuthoredFilesResult { files: vec![] });
        }
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&sandbox)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_authored_extension(path) {
                continue;
            }
            let Ok(rel_to_sandbox) = path.strip_prefix(&sandbox) else {
                continue;
            };
            let Ok(rel_to_rig) = path.strip_prefix(&rig_root) else {
                continue;
            };
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push(AuthoredFile {
                rel_path: rel_to_rig.to_string_lossy().replace('\\', "/"),
                name: rel_to_sandbox
                    .with_extension("")
                    .to_string_lossy()
                    .replace('\\', "/"),
                bytes,
            });
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ListAuthoredFilesResult { files })
    }

    /// Read one file under `<rig_root>/.yah/arch/authored/`. Mirrors
    /// `arch.read_authored_file`. The dominant safety property is that
    /// `params.rel_path` is canonicalized and verified to live inside
    /// the sandbox before any I/O — paths that escape (via `..`,
    /// absolute prefix, or symlink to outside) are rejected as
    /// `Conflict` rather than read.
    pub async fn read_authored_file(
        &self,
        params: ReadAuthoredFileParams,
    ) -> Result<ReadAuthoredFileResult, DaemonError> {
        let rig_root = self.rig_root().await?;
        let sandbox = rig_root.join(".yah").join("arch").join("authored");
        let sandbox_canon = sandbox.canonicalize().map_err(|_| {
            DaemonError::Conflict(format!(
                "authored arch directory does not exist: {}",
                sandbox.display()
            ))
        })?;
        let candidate = rig_root.join(&params.rel_path);
        let candidate_canon = candidate.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve {}: {}",
                params.rel_path, e
            ))
        })?;
        if !candidate_canon.starts_with(&sandbox_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is outside the authored arch sandbox",
                params.rel_path
            )));
        }
        if !is_authored_extension(&candidate_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is not a .mmd or .md file",
                params.rel_path
            )));
        }
        let content = std::fs::read_to_string(&candidate_canon)
            .map_err(|e| DaemonError::Io(e.to_string()))?;
        let bytes = content.len() as u64;
        Ok(ReadAuthoredFileResult {
            rel_path: params.rel_path,
            content,
            bytes,
        })
    }

    /// Read bytes from a rig-relative path. Mirrors `file.read`.
    ///
    /// Safety: `params.path` is canonicalized and rejected if it escapes
    /// the rig root (via `..`, absolute prefix, or symlink target outside
    /// the tree). With no `range`, the response is clipped at
    /// [`FILE_READ_SOFT_CAP_BYTES`] and `truncated` is set so the renderer
    /// knows to page; with `range`, the cap does not apply (the caller is
    /// already paging).
    pub async fn file_read(
        &self,
        params: FileReadParams,
    ) -> Result<FileReadResult, DaemonError> {
        let rig_root = self.rig_root().await?;
        let rig_root_canon = rig_root.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve rig root {}: {}",
                rig_root.display(),
                e
            ))
        })?;
        let candidate = rig_root.join(&params.path);
        let candidate_canon = candidate.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!("cannot resolve {}: {}", params.path, e))
        })?;
        if !candidate_canon.starts_with(&rig_root_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is outside the rig root",
                params.path
            )));
        }
        let meta = std::fs::metadata(&candidate_canon)
            .map_err(|e| DaemonError::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(DaemonError::Conflict(format!(
                "{} is not a regular file",
                params.path
            )));
        }
        let total_bytes = meta.len();

        let (offset, requested_len, range_supplied) = match params.range {
            Some(r) => (r.offset, r.len as u64, true),
            None => (0, FILE_READ_SOFT_CAP_BYTES as u64, false),
        };

        if offset > total_bytes {
            return Err(DaemonError::Conflict(format!(
                "offset {} past end of {} ({} bytes)",
                offset, params.path, total_bytes
            )));
        }

        let available = total_bytes - offset;
        let to_read = requested_len.min(available);
        let truncated = !range_supplied && available > requested_len;
        let eof = offset + to_read >= total_bytes;

        let bytes = read_file_slice(&candidate_canon, offset, to_read)
            .map_err(|e| DaemonError::Io(e.to_string()))?;

        let (content, encoding) = match std::str::from_utf8(&bytes) {
            Ok(s) => (s.to_string(), FileEncoding::Utf8),
            Err(_) => {
                use base64::Engine;
                (
                    base64::engine::general_purpose::STANDARD.encode(&bytes),
                    FileEncoding::Base64,
                )
            }
        };

        Ok(FileReadResult {
            path: params.path,
            content,
            encoding,
            bytes: bytes.len() as u32,
            total_bytes,
            offset,
            eof,
            truncated,
        })
    }

    /// Write `params.content` to a rig-relative path with optimistic
    /// concurrency. Mirrors `file.write`.
    ///
    /// Two operations selected by `params.expected_mtime_ms`:
    ///
    /// - `None` → create-new. Resolves the parent directory under the
    ///   rig root, then verifies the target does not yet exist. The
    ///   parent must already be a directory (no implicit `mkdir -p`).
    /// - `Some(t)` → update-existing. Verifies the target's current
    ///   mtime equals `t`; otherwise returns
    ///   `DaemonError::Conflict("mtime mismatch: expected … actual …")`
    ///   so the renderer can show the "file changed on disk" prompt.
    ///
    /// The write itself is atomic: bytes go to a sibling temp file
    /// (`.<name>.tmp`) and are renamed into place. Watchers see one
    /// `modified` event for the rename rather than a partial buffer.
    pub async fn file_write(
        &self,
        params: FileWriteParams,
    ) -> Result<FileWriteResult, DaemonError> {
        let rig_root = self.rig_root().await?;
        let rig_root_canon = rig_root.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve rig root {}: {}",
                rig_root.display(),
                e
            ))
        })?;

        let rel = params.path.trim_matches('/');
        if rel.is_empty() || rel == "." {
            return Err(DaemonError::Conflict(
                "path must address a file, not the rig root".into(),
            ));
        }
        let candidate = rig_root.join(rel);

        // Resolve parent + filename separately so create-new works for a
        // path that doesn't exist yet. The parent must already exist —
        // we don't `mkdir -p` (a renderer that needs that should call a
        // future `dir.create`).
        let parent = candidate.parent().ok_or_else(|| {
            DaemonError::Conflict(format!("{} has no parent directory", params.path))
        })?;
        let file_name = candidate.file_name().ok_or_else(|| {
            DaemonError::Conflict(format!("{} has no file name", params.path))
        })?;
        let parent_canon = parent.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve parent of {}: {}",
                params.path, e
            ))
        })?;
        if !parent_canon.starts_with(&rig_root_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is outside the rig root",
                params.path
            )));
        }
        if !parent_canon.is_dir() {
            return Err(DaemonError::Conflict(format!(
                "parent of {} is not a directory",
                params.path
            )));
        }
        let target = parent_canon.join(file_name);

        // Decode content per encoding before any disk action so a bad
        // payload can't leave a temp file behind.
        let bytes = match params.encoding {
            FileEncoding::Utf8 => params.content.into_bytes(),
            FileEncoding::Base64 => {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(params.content.as_bytes())
                    .map_err(|e| {
                        DaemonError::Conflict(format!("invalid base64: {}", e))
                    })?
            }
        };

        let existing_meta = std::fs::metadata(&target).ok();
        let created = match (params.expected_mtime_ms, existing_meta.as_ref()) {
            (None, Some(_)) => {
                return Err(DaemonError::Conflict(format!(
                    "{} already exists; pass expected_mtime_ms to overwrite",
                    params.path
                )));
            }
            (None, None) => true,
            (Some(_), None) => {
                return Err(DaemonError::Conflict(format!(
                    "{} does not exist; omit expected_mtime_ms to create",
                    params.path
                )));
            }
            (Some(expected), Some(meta)) => {
                if !meta.is_file() {
                    return Err(DaemonError::Conflict(format!(
                        "{} is not a regular file",
                        params.path
                    )));
                }
                let actual = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);
                let actual_value = actual.ok_or_else(|| {
                    DaemonError::Conflict(format!(
                        "platform does not report mtime for {}",
                        params.path
                    ))
                })?;
                if actual_value != expected {
                    return Err(DaemonError::Conflict(format!(
                        "mtime mismatch: expected {} actual {}",
                        expected, actual_value
                    )));
                }
                false
            }
        };

        // Atomic write: bytes -> sibling temp -> rename. Mirrors the
        // pattern used by `snapshot::write_snapshot`.
        let tmp_name = format!(".{}.tmp", file_name.to_string_lossy());
        let tmp_path = parent_canon.join(tmp_name);
        std::fs::write(&tmp_path, &bytes).map_err(|e| {
            DaemonError::Io(format!("write {}: {}", tmp_path.display(), e))
        })?;
        if let Err(e) = std::fs::rename(&tmp_path, &target) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(DaemonError::Io(format!(
                "rename {} -> {}: {}",
                tmp_path.display(),
                target.display(),
                e
            )));
        }

        let mtime_ms = std::fs::metadata(&target)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);

        Ok(FileWriteResult {
            path: params.path,
            mtime_ms,
            bytes: bytes.len() as u64,
            created,
        })
    }

    /// One-shot listing of `params.path` under the rig root. Mirrors
    /// `dir.list`. The path is canonicalized and rejected if it escapes
    /// the rig root or doesn't resolve to a directory. Empty/`.` lists
    /// the rig root itself.
    pub async fn dir_list(
        &self,
        params: DirListParams,
    ) -> Result<DirListResult, DaemonError> {
        let rig_root = self.rig_root().await?;
        let rig_root_canon = rig_root.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!(
                "cannot resolve rig root {}: {}",
                rig_root.display(),
                e
            ))
        })?;
        let rel = params.path.trim_matches('/');
        let candidate = if rel.is_empty() || rel == "." {
            rig_root.clone()
        } else {
            rig_root.join(rel)
        };
        let candidate_canon = candidate.canonicalize().map_err(|e| {
            DaemonError::Conflict(format!("cannot resolve {}: {}", params.path, e))
        })?;
        if !candidate_canon.starts_with(&rig_root_canon) {
            return Err(DaemonError::Conflict(format!(
                "{} is outside the rig root",
                params.path
            )));
        }
        let meta = std::fs::metadata(&candidate_canon)
            .map_err(|e| DaemonError::Io(e.to_string()))?;
        if !meta.is_dir() {
            return Err(DaemonError::Conflict(format!(
                "{} is not a directory",
                params.path
            )));
        }

        let mut entries: Vec<DirEntry> = Vec::new();
        let read = std::fs::read_dir(&candidate_canon)
            .map_err(|e| DaemonError::Io(e.to_string()))?;
        for ent in read {
            let ent = match ent {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = match ent.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let symlink_meta = ent.metadata();
            let is_symlink = ent
                .file_type()
                .map(|t| t.is_symlink())
                .unwrap_or(false);
            let (kind, size, mtime_ms) = match symlink_meta {
                Ok(m) => {
                    let kind = if m.is_dir() {
                        DirEntryKind::Dir
                    } else if m.is_file() {
                        DirEntryKind::File
                    } else {
                        DirEntryKind::Other
                    };
                    let size = if m.is_file() { m.len() } else { 0 };
                    let mtime_ms = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64);
                    (kind, size, mtime_ms)
                }
                Err(_) => (DirEntryKind::Other, 0, None),
            };
            entries.push(DirEntry {
                name,
                kind,
                size,
                mtime_ms,
                is_symlink,
            });
        }

        entries.sort_by(|a, b| {
            let a_dir = matches!(a.kind, DirEntryKind::Dir);
            let b_dir = matches!(b.kind, DirEntryKind::Dir);
            match (a_dir, b_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        let echo_path = if rel.is_empty() || rel == "." {
            String::new()
        } else {
            rel.replace('\\', "/")
        };
        Ok(DirListResult {
            path: echo_path,
            entries,
        })
    }

    pub async fn stats(&self) -> StatsResult {
        let store = self.store.read().await;
        let s = store.stats();
        let last_index_ms = self
            .booted
            .read()
            .await
            .as_ref()
            .and_then(|b| b.last_index_ms);
        StatsResult {
            node_count: s.node_count,
            edge_count: s.edge_count,
            by_lang: s.by_lang,
            by_kind: s.by_kind,
            last_index_ms,
        }
    }

    // ---------- helpers ----------

    async fn rig_root(&self) -> Result<PathBuf, DaemonError> {
        self.booted
            .read()
            .await
            .as_ref()
            .map(|b| b.rig_root.clone())
            .ok_or(DaemonError::NotBooted)
    }

    fn send(&self, event: ArchEvent) {
        // Ignore the count — broadcast::send only fails if there are zero
        // subscribers, which is fine in tests and in cold-start.
        let _ = self.events.send(event);
    }
}

/// Soft cap for unbounded `file.read` requests. Monaco starts to flounder
/// past a few MB and our wire envelope is JSON, so reads without an
/// explicit `range` clip here and the renderer pages from there.
pub const FILE_READ_SOFT_CAP_BYTES: u32 = 5 * 1024 * 1024;

/// Read `len` bytes from `path` starting at `offset`. Uses
/// Authored arch files surface in two flavors: raw `.mmd` (rendered as a
/// mermaid canvas) and `.md` (rendered as markdown, with `~~~mermaid`
/// fences specialized into diagrams). Both share the same sandbox; the
/// renderer picks the right pane by extension.
fn is_authored_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("mmd") | Some("md")
    )
}

/// [`std::os::unix::fs::FileExt::read_at`]/`seek_read` on Windows so
/// concurrent reads don't fight the file cursor.
fn read_file_slice(
    path: &Path,
    offset: u64,
    len: u64,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// Read each `@arch:see` doc body the prelude assembler may want to inline.
/// Files larger than `4 * inline_max_bytes` are skipped — the assembler
/// would render them as reference-only anyway, and reading 100MB to
/// throw it away is wasted I/O. Missing files surface as missing keys
/// (not errors): the assembler falls back to a reference-only line.
fn load_arch_docs(
    rig_root: Option<&Path>,
    paths: &[String],
    inline_max_bytes: usize,
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let Some(root) = rig_root else {
        return out;
    };
    let read_cap = inline_max_bytes.saturating_mul(4);
    for rel in paths {
        let abs = root.join(rel);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if (meta.len() as usize) > read_cap {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&abs) else {
            continue;
        };
        out.insert(rel.clone(), body);
    }
    out
}

fn emit_delta(events: &broadcast::Sender<ArchEvent>, delta: &yah_kg_store::FileDelta) {
    for node in &delta.nodes_added {
        let _ = events.send(ArchEvent::NodeAdded { node: node.clone() });
    }
    for (id, fields) in &delta.nodes_changed {
        let _ = events.send(ArchEvent::NodeChanged {
            id: *id,
            fields: fields.clone(),
        });
    }
    for id in &delta.nodes_removed {
        let _ = events.send(ArchEvent::NodeRemoved { id: *id });
    }
    for edge in &delta.edges_added {
        let _ = events.send(ArchEvent::EdgeAdded { edge: edge.clone() });
    }
    for id in &delta.edges_removed {
        let _ = events.send(ArchEvent::EdgeRemoved { id: *id });
    }
}

fn emit_work_items(
    events: &broadcast::Sender<ArchEvent>,
    touched: &[TouchedWorkItem],
) {
    for t in touched {
        let event = match t.item_type {
            WorkItemType::Relay => ArchEvent::RelayChanged {
                node: t.node,
                work_item_id: t.work_item_id.clone(),
            },
            WorkItemType::Ticket => ArchEvent::TicketChanged {
                node: t.node,
                work_item_id: t.work_item_id.clone(),
            },
        };
        let _ = events.send(event);
    }
}

/// Fan one [`FileEvent`] per (changed-path × matching-watch-handle)
/// pair into the broadcast channel. Path classification is post-hoc:
/// missing paths surface as `Removed`; present paths surface as
/// `Modified`. We don't try to distinguish `Created` from `Modified`
/// because the debounce window collapses create-then-write into a single
/// notify batch and the difference doesn't matter to the renderer
/// (whichever it is, the file's contents need a re-read).
async fn fan_file_events(
    abs_paths: &[PathBuf],
    rig_root: &Path,
    watches: &Arc<RwLock<WatchRegistry>>,
    file_events: &broadcast::Sender<FileEvent>,
) {
    if abs_paths.is_empty() {
        return;
    }
    // Snapshot the registry under a short-lived read lock so the actual
    // metadata I/O happens outside the critical section.
    let entries: Vec<(u64, WatchEntry)> = {
        let reg = watches.read().await;
        reg.entries
            .iter()
            .map(|(id, e)| (*id, e.clone()))
            .collect()
    };
    if entries.is_empty() {
        return;
    }
    for abs in abs_paths {
        let mut matching: Vec<u64> = Vec::new();
        for (id, e) in &entries {
            let hit = if e.recursive {
                abs.starts_with(&e.root)
            } else {
                abs == &e.root
            };
            if hit {
                matching.push(*id);
            }
        }
        if matching.is_empty() {
            continue;
        }
        let (kind, mtime_ms) = match std::fs::metadata(abs) {
            Ok(m) => {
                let mt = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);
                (FileEventKind::Modified, mt)
            }
            Err(_) => (FileEventKind::Removed, None),
        };
        let rel = relativize(abs, rig_root).unwrap_or_else(|| abs.display().to_string());
        for id in matching {
            let _ = file_events.send(FileEvent {
                watch_id: id,
                kind,
                path: rel.clone(),
                mtime_ms,
            });
        }
    }
}

async fn apply_paths(
    abs_paths: Vec<PathBuf>,
    rig_root: &Path,
    store: &Arc<RwLock<Store>>,
    annotations: &Arc<RwLock<AnnotationIndex>>,
    registry: &Arc<IndexerRegistry>,
    events: &broadcast::Sender<ArchEvent>,
) {
    if abs_paths.is_empty() {
        return;
    }
    // Resolve to rig-relative paths up front so we hold the lock briefly.
    let mut rels: Vec<String> = Vec::new();
    for p in abs_paths {
        let Some(rel) = relativize(&p, rig_root) else {
            continue;
        };
        if !is_eligible(Path::new(&rel)) {
            continue;
        }
        rels.push(rel);
    }
    rels.sort();
    rels.dedup();
    if rels.is_empty() {
        return;
    }

    let _ = events.send(ArchEvent::IndexStarted {
        reason: IndexReason::FileWatch,
        scope: IndexScope::Files {
            paths: rels.clone(),
        },
    });

    let start = Instant::now();
    let mut totals = (0u32, 0u32, 0u32, 0u32, 0u32);
    for rel in &rels {
        let mut s = store.write().await;
        let mut a = annotations.write().await;
        match reindex_file(rig_root, rel, &mut s, registry) {
            Ok(delta) => {
                for id in &delta.nodes_removed {
                    a.remove(*id);
                }
                let scope: std::collections::HashSet<_> =
                    s.lookup(rel, None).into_iter().collect();
                let apply = apply_pass(&mut s, &mut a, Some(&scope));
                drop(a);
                drop(s);
                totals.0 += delta.nodes_added.len() as u32;
                totals.1 += delta.nodes_changed.len() as u32;
                totals.2 += delta.nodes_removed.len() as u32;
                totals.3 += delta.edges_added.len() as u32;
                totals.4 += delta.edges_removed.len() as u32;
                emit_delta(events, &delta);
                emit_work_items(events, &apply.touched_work_items);
            }
            Err(e) => {
                tracing::warn!(?rel, error = %e, "reindex failed");
            }
        }
    }
    let _ = events.send(ArchEvent::IndexFinished {
        duration_ms: start.elapsed().as_millis() as u64,
        nodes_added: totals.0,
        nodes_changed: totals.1,
        nodes_removed: totals.2,
        edges_added: totals.3,
        edges_removed: totals.4,
    });
}

fn parse_path_line(s: &str) -> (&str, Option<u32>) {
    if let Some((file, line)) = s.rsplit_once(':') {
        if let Ok(n) = line.parse::<u32>() {
            return (file, Some(n));
        }
    }
    (s, None)
}

/// Collect free-floating `@yah:rule(agent-...)` annotations from the
/// AnnotationIndex into a workspace-level policy list for the prelude
/// assembler (R028-F10).
///
/// Two parsing paths produce policy data:
///
/// 1. The yah-kg-anno parser folds policy rules onto a relay/ticket's
///    `WorkItemAnno::agent_policy` when the directive appears inside a
///    work-item block. Those reach the prelude through the board.
/// 2. Policy rules outside any work-item block are emitted as plain
///    `AnnotationKind::Rule { rule_kind, args }`. This function picks
///    them up and re-parses through `agent_policy::parse_rule`, so they
///    reach the prelude as `workspace_policy` regardless of where in
///    the rig they were authored.
///
/// Malformed agent-policy rules (parse error from `parse_rule`) are
/// silently skipped here — the validator owns vocabulary diagnostics.
fn collect_workspace_agent_policy(
    anno: &AnnotationIndex,
) -> Vec<yah_kg::agent_policy::AgentPolicyRule> {
    use yah_kg::anno::AnnotationKind;
    let mut out = Vec::new();
    for (_node, refs) in anno.iter() {
        for r in refs {
            if let AnnotationKind::Rule { rule_kind, args } = &r.kind {
                if let Some(Ok(rule)) = yah_kg::agent_policy::parse_rule(rule_kind, args) {
                    out.push(rule);
                }
            }
        }
    }
    out
}

fn collect_work_items(
    store: &Store,
    anno: &AnnotationIndex,
    item_type: WorkItemType,
    rig_root: Option<&Path>,
) -> Vec<WorkItem> {
    let want_kind = match item_type {
        WorkItemType::Relay => CommonKind::Relay,
        WorkItemType::Ticket => CommonKind::Ticket,
    };
    let mut out: Vec<WorkItem> = store
        .all_node_refs()
        .filter(|n| matches!(&n.kind, NodeKind::Common(k) if *k == want_kind))
        .filter_map(|n| build_work_item(store, anno, n, item_type, rig_root))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Rebuild one `WorkItem` from its synthetic node by walking incoming
/// `Anchors` edges back to the structural sources. Each anchor carries
/// its own parsed `WorkItemAnno` so the board-recompute layer can detect
/// scalar disagreements across anchors; `WorkItem::anno` is the lex-first
/// anchor's payload (deterministic winner). A synthetic node with no
/// anchors is treated as a stub and dropped (returns `None`) — those
/// exist when a parent id is referenced before the parent's own header
/// is scanned.
fn build_work_item(
    store: &Store,
    anno: &AnnotationIndex,
    synthetic: &yah_kg::ids::NodeRef,
    item_type: WorkItemType,
    rig_root: Option<&Path>,
) -> Option<WorkItem> {
    let id = synthetic.label.clone();
    let edges = store.neighbors(synthetic.id, Direction::In, Some(&[EdgeKind::Anchors]));
    if edges.is_empty() {
        return None;
    }
    let mut anchors: Vec<WorkItemAnchor> = Vec::with_capacity(edges.len());
    for edge in &edges {
        let Some(structural) = store.node_ref(edge.from) else {
            continue;
        };
        let mut anchor_payload: Option<yah_kg::anno::WorkItemAnno> = None;
        for ann in anno.get(edge.from) {
            let candidate = match (item_type, &ann.kind) {
                (WorkItemType::Relay, AnnotationKind::Relay(w)) => Some(w),
                (WorkItemType::Ticket, AnnotationKind::Ticket(w)) => Some(w),
                _ => None,
            };
            if let Some(w) = candidate {
                if w.id == id {
                    anchor_payload = Some(w.clone());
                    break;
                }
            }
        }
        let Some(anchor_anno) = anchor_payload else {
            // Anchor edge exists but no matching `@yah:` payload found —
            // skip rather than emit a half-anchor; matches the previous
            // "drop on missing payload" semantics.
            continue;
        };
        anchors.push(WorkItemAnchor {
            node: edge.from,
            file: structural.file.clone(),
            line: structural.span.start_line,
            anno: anchor_anno,
        });
    }
    if anchors.is_empty() {
        return None;
    }
    anchors.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    let payload = anchors[0].anno.clone();
    let last_modified_ts = rig_root
        .map(|root| last_modified_for(root, &id, &anchors))
        .unwrap_or(0);
    Some(WorkItem {
        id,
        node: synthetic.id,
        item_type,
        anno: payload,
        anchors,
        last_modified_ts,
    })
}

/// Per-relay shard name for a work-item id. Sub-tickets live in their
/// parent's shard (`R007-T1` → `R007`); bare ids use their own. Mirrors
/// `yah::arch::archive::shard_for` — the daemon is intentionally not
/// taking a dep on the `yah` crate, so the rule is duplicated here.
fn shard_for(id: &str) -> &str {
    id.split_once('-').map(|(p, _)| p).unwrap_or(id)
}

/// Resolve `last_modified_ts` for a work-item.
///
/// Resolution order (highest precedence first):
/// 1. Per-relay event shard at `<rig_root>/.yah/events/<shard>.jsonl` —
///    scan every line and keep the largest `t` on a record whose `id`
///    matches. Today the shard only carries `archived` events, but a
///    historical scan/move record (R001-R025) still wins if present.
/// 2. The lex-first anchor's `@yah:at(<rfc3339>)` — written by daemon
///    mutations (`move_ticket`). Per-ticket precision so co-resident
///    tickets in the same file don't share a single file mtime.
/// 3. The lex-first anchor's source file mtime — last-resort fallback
///    so freshly-authored tickets still sort sensibly before they've
///    been touched by a daemon mutation.
///
/// Returns `0` only when all three fail.
fn last_modified_for(rig_root: &Path, id: &str, anchors: &[WorkItemAnchor]) -> u64 {
    let shard_path = rig_root
        .join(".yah")
        .join("events")
        .join(format!("{}.jsonl", shard_for(id)));
    if let Ok(raw) = std::fs::read_to_string(&shard_path) {
        let mut latest: u64 = 0;
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if ev.get("id").and_then(|v| v.as_str()) != Some(id) {
                continue;
            }
            if let Some(t) = ev.get("t").and_then(|v| v.as_u64()) {
                if t > latest {
                    latest = t;
                }
            }
        }
        if latest > 0 {
            return latest;
        }
    }
    if let Some(anchor) = anchors.first() {
        if let Some(at) = anchor.anno.at.as_deref() {
            if let Some(secs) = yah_kg::timefmt::parse_rfc3339(at) {
                return secs;
            }
        }
        let abs = rig_root.join(&anchor.file);
        if let Ok(meta) = std::fs::metadata(&abs) {
            if let Ok(modified) = meta.modified() {
                if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                    return d.as_secs();
                }
            }
        }
    }
    0
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Append a single JSONL event line to the per-relay shard. Creates
/// `.yah/events/` if missing.
fn append_archive_event(
    rig_root: &Path,
    shard: &str,
    event: &serde_json::Value,
) -> std::io::Result<()> {
    use std::io::Write;
    let dir = rig_root.join(".yah").join("events");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{shard}.jsonl"));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut line = serde_json::to_string(event).unwrap();
    line.push('\n');
    f.write_all(line.as_bytes())
}

