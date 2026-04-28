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
//! @yah:status(handoff)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Use postcard for size; serde_json acceptable as a v1")
//! @yah:next("Write snapshot on IndexFinished, replay on next boot, then quick mtime-diff to bring stale files current")
//! @yah:next("Old rs-hack-arch had source_hash caching worth lifting")
//! @yah:verify("Cold boot on a workspace with a saved snapshot is order-of-magnitude faster than full reindex")
//! @yah:handoff("KG snapshot persistence landed. New surface: KgService::save(path) / load(path) / boot_with_snapshot(rig_root, snapshot_path) / save_default() — backed by KgSnapshot { rig_root, fingerprints, store, annotations } at .yah/cache/snapshot.json (v1, atomic temp+rename, JSON for v1 — postcard slot reserved by SNAPSHOT_VERSION). yah-kg-store now exposes Store::to_snapshot/restore + StoreSnapshot/SnapshotError; yah-kg-anno exposes AnnotationIndex::to_snapshot/restore + AnnotationIndexSnapshot. yah-kg-daemon got snapshot.rs (FileFingerprint mtime+size, fingerprint_rig walker mirroring walk_and_index skip rules, diff_fingerprints → ReconcilePlan). boot() now refreshes fingerprints; boot_with_snapshot loads + walks + reconciles via reindex_path on each changed/removed file (falls back to full boot when snapshot is missing or rig_root mismatches). Tests: 5 new e2e (round-trip, fallback, mtime-diff skip-vs-reindex, deletion reconcile, rig_root mismatch); 21/21 daemon e2e green; yah-kg / yah-kg-store / yah-kg-anno / yah-kg-validator unit tests all green; yah-tauri builds. Tauri startup wiring is now its own sub-ticket R017-T7. Source-hash caching from old rs-hack-arch is a future optimization (currently mtime+size only).")
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
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R019)
//! @yah:next("Use openssh-rs or shell out to ssh; framing is line-delimited JSON-RPC")
//! @yah:next("Health/keepalive: re-establish on connection drop with exponential backoff")

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
use yah_kg::event::{ArchEvent, IndexReason, IndexScope};
use yah_kg::ids::{NodeFull, NodeId};
use yah_kg::indexer::IndexError;
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg::rpc::{
    Direction, GetTicketParams, GetTicketResult, ListRelaysParams, ListRelaysResult,
    ListTicketsParams, ListTicketsResult, LookupParams, LookupResult, NeighborsParams,
    NeighborsResult, RootsParams, RootsResult, StatsResult, Subgraph, SubgraphParams,
    TicketPromptParams, TicketPromptResult, ValidateParams, ValidateResult, WorkItem,
    WorkItemAnchor,
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

pub struct KgService {
    store: Arc<RwLock<Store>>,
    annotations: Arc<RwLock<AnnotationIndex>>,
    /// Per-file fingerprints captured the last time the rig was walked.
    /// Snapshot save/load round-trips this; boot_with_snapshot uses it
    /// to compute which files to reindex.
    fingerprints: Arc<RwLock<HashMap<String, FileFingerprint>>>,
    registry: Arc<IndexerRegistry>,
    events: broadcast::Sender<ArchEvent>,
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
        Self {
            store: Arc::new(RwLock::new(Store::new())),
            annotations: Arc::new(RwLock::new(AnnotationIndex::new())),
            fingerprints: Arc::new(RwLock::new(HashMap::new())),
            registry: Arc::new(registry),
            events,
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
        let canon = canonicalize_root(&rig_root);
        let snap = match read_snapshot(snapshot_path) {
            Ok(s) if s.rig_root == canon => Some(s),
            Ok(_) | Err(_) => None,
        };
        let Some(snap) = snap else {
            // No usable snapshot: fall back to a full boot. The caller
            // will typically want to `save()` afterwards so the next
            // cold-start has something to replay.
            return self.boot(rig_root).await;
        };

        self.send(ArchEvent::IndexStarted {
            reason: IndexReason::Boot,
            scope: IndexScope::All,
        });
        let start = Instant::now();

        // Restore the saved store + annotations.
        {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            *store = Store::new();
            store.restore(snap.store).map_err(SnapshotError::from)?;
            *anno = AnnotationIndex::new();
            anno.restore(snap.annotations);
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
        let plan = self.reconcile(&canon, &snap.fingerprints).await?;
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
        write_snapshot(path, &snap).map_err(DaemonError::from)
    }

    /// Convenience: write to the conventional
    /// `<rig_root>/.yah/cache/snapshot.json`.
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
        let root_for_task = rig_root.clone();

        let handle = spawn_watcher(
            WatcherKind::Recursive,
            rig_root.clone(),
            move |abs_paths| {
                let store = Arc::clone(&store);
                let annotations = Arc::clone(&annotations);
                let registry = Arc::clone(&registry);
                let events = events.clone();
                let root = root_for_task.clone();
                Box::pin(async move {
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
    pub async fn list_relays(&self, _params: ListRelaysParams) -> ListRelaysResult {
        let rig_root = self.rig_root().await.ok();
        let store = self.store.read().await;
        let anno = self.annotations.read().await;
        let relays = collect_work_items(&store, &anno, WorkItemType::Relay, rig_root.as_deref());
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
/// Primary source is the per-relay event shard at
/// `<rig_root>/.yah/events/<shard>.jsonl` — scan every line and keep the
/// largest `t` (unix seconds) on a record whose `id` matches. The shard
/// is small (one line per status move / scan / archive), so reading the
/// whole file is cheaper than parsing-from-the-tail tricks.
///
/// Fallback when no shard exists or it has no record for this id: the
/// mtime of the lex-first anchor's source file. That keeps freshly-
/// claimed tickets sortable before they've accrued any events. Returns
/// `0` only when both fail (e.g. the shard is unreadable and the source
/// file's metadata can't be stat'd).
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

