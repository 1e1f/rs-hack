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

use crate::path::{canonicalize_root, is_eligible, relativize};
use crate::watcher::{spawn_watcher, WatcherHandle, WatcherKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use yah_kg::event::{ArchEvent, IndexReason, IndexScope};
use yah_kg::ids::{NodeFull, NodeId};
use yah_kg::indexer::IndexError;
use yah_kg::kind::Lang;
use yah_kg::rpc::{
    LookupParams, LookupResult, NeighborsParams, NeighborsResult, RootsParams, RootsResult,
    StatsResult, Subgraph, SubgraphParams,
};
use yah_kg_anno::{apply_pass, AnnotationIndex};
use yah_kg_store::{reindex_file, walk_and_index, IndexerRegistry, Store, WalkSummary};

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
        let summary = {
            let mut store = self.store.write().await;
            let mut anno = self.annotations.write().await;
            // Wipe existing nodes before booting against a new rig.
            *store = Store::new();
            *anno = AnnotationIndex::new();
            let s = walk_and_index(&canon, &mut store, &self.registry)
                .map_err(DaemonError::Index)?;
            // Pass 4: annotations.
            apply_pass(&mut store, &mut anno, None);
            s
        };
        let elapsed = start.elapsed().as_millis() as u64;

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

        let delta = {
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
            apply_pass(&mut store, &mut anno, Some(&scope));
            d
        };

        emit_delta(&self.events, &delta);

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
                apply_pass(&mut s, &mut a, Some(&scope));
                drop(a);
                drop(s);
                totals.0 += delta.nodes_added.len() as u32;
                totals.1 += delta.nodes_changed.len() as u32;
                totals.2 += delta.nodes_removed.len() as u32;
                totals.3 += delta.edges_added.len() as u32;
                totals.4 += delta.edges_removed.len() as u32;
                emit_delta(events, &delta);
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

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

