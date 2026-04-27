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

pub mod path;
pub mod service;
pub mod watcher;

pub use service::{DaemonError, KgService, ServiceConfig};
pub use watcher::{WatcherHandle, WatcherKind};
