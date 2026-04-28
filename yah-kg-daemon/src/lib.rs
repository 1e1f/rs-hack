//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! `yah-kg-daemon` — runtime composition of the knowledge graph.
//!
//! Wraps a [`yah_kg_store::Store`] in a `tokio::sync::RwLock`, owns an
//! [`yah_kg_store::IndexerRegistry`], runs a `notify` file watcher, and
//! fans `ArchEvent`s out over a `tokio::sync::broadcast` channel.
//!
//! Exposes the `arch.*` RPC surface as in-process async methods so Tauri
//! commands can call them directly. JSON-RPC framing for remote/browser
//! transports lives downstream — this crate is transport-agnostic.
//!
//! Concurrency model:
//! * One `Store` behind `RwLock`. Queries take read locks (cheap, parallel);
//!   reindex paths take write locks for the snapshot/wipe/re-emit cycle.
//! * Watcher events arrive on a `notify` callback running on its own
//!   thread; we forward them through an `mpsc` channel to a tokio task
//!   that does the reindex on the runtime.
//! * `ArchEvent` is a `tokio::sync::broadcast` so any number of subscribers
//!   (Tauri command, JSON-RPC fanout, internal listeners) can tail without
//!   blocking each other.
//!
//! @yah:relay(R018, "Pi-mono agent integration: server-side runner + AgentView wiring")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R013)
//! @arch:see(architecture/yah-roadmap-2026Q2.md)
//!
//! @yah:ticket(R018-T1, "Design doc: pi-mono runner harness, session JSONL, Tauri command surface")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:next("Write architecture/pi-mono-agent.md before any code lands")
//! @yah:next("Cover: process model (one runner per session), JSONL shape (subset of Claude transcript?), command/event surface")
//!
//! @yah:ticket(R018-F2, "Server-side pi-mono runner + session JSONL storage")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:next("Spawn runner subprocess per session; write events to .yah/sessions/<id>.jsonl")
//! @yah:next("arch_touch already resolves path:line from tool results — wire it into the runner's tool output stream")

pub mod path;
pub mod service;
pub mod snapshot;
pub mod watcher;

pub use service::{DaemonError, KgService, ServiceConfig};
pub use snapshot::{
    default_snapshot_path, diff_fingerprints, fingerprint_rig, read_snapshot, write_snapshot,
    FileFingerprint, KgSnapshot, ReconcilePlan, SnapshotError, SNAPSHOT_VERSION,
};
pub use watcher::{WatcherHandle, WatcherKind};
