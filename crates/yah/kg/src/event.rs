//! @arch:layer(kg)
//! @arch:role(protocol)
//!
//! `ArchEvent` — the push channel from the daemon to subscribers.
//!
//! Two consumers care about this stream: the Tauri shell (which uses
//! events to incrementally update ArchView, the Board, and Agent
//! status overlays) and any pi-mono agent loop wanting structural
//! awareness of its own edits.
//!
//! `AgentTouch` is the integration point that lets the UI candle-pulse
//! the nodes a running agent has touched. The daemon resolves the
//! agent's tool-call paths against its `(file, line) → NodeId` index
//! server-side so the browser never does path resolution.

use crate::edge::{EdgeId, EdgeOut};
use crate::ids::{NodeId, NodeRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ArchEvent {
    IndexStarted {
        reason: IndexReason,
        scope: IndexScope,
    },
    IndexFinished {
        duration_ms: u64,
        nodes_added: u32,
        nodes_changed: u32,
        nodes_removed: u32,
        edges_added: u32,
        edges_removed: u32,
    },
    NodeAdded {
        node: NodeRef,
    },
    NodeChanged {
        id: NodeId,
        fields: Vec<ChangedField>,
    },
    NodeRemoved {
        id: NodeId,
    },
    EdgeAdded {
        edge: EdgeOut,
    },
    EdgeRemoved {
        id: EdgeId,
    },
    /// Pi-mono agent touched these nodes via a tool call. The daemon
    /// is responsible for resolving the agent's `path:line` payloads
    /// to node ids; subscribers just react.
    AgentTouch {
        ids: Vec<NodeId>,
        tool: String,
        relay: String,
        ts: u64,
    },
    /// A `@yah:relay(...)` block was upserted by the annotation pass.
    /// Fires per relay touched on each reindex (boot or per-file). The
    /// Board UI subscribes so it can refresh just the relay row without
    /// re-rendering the whole graph. `node` is the synthetic
    /// `CommonKind::Relay` node id; `work_item_id` is the source-level
    /// ID like `R042`.
    RelayChanged {
        node: NodeId,
        work_item_id: String,
    },
    /// A `@yah:ticket(...)` block was upserted by the annotation pass.
    /// Counterpart to `RelayChanged` — see that variant for semantics.
    TicketChanged {
        node: NodeId,
        work_item_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexReason {
    /// Daemon cold-start (re-hydrating from snapshot or full reindex).
    Boot,
    /// File-watcher noticed a change on disk.
    FileWatch,
    /// Operator explicitly requested via `arch.reindex`.
    Manual,
    /// Pi-mono agent edited a file mid-session.
    AgentEdit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum IndexScope {
    All,
    Files { paths: Vec<String> },
    Subtree { root: String },
}

/// Which fields of a node changed in a `NodeChanged` event. Lets the UI
/// avoid full re-renders when only metadata moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangedField {
    Span,
    Label,
    Qualified,
    File,
    Doc,
    Properties,
    Annotations,
}

/// Filesystem change emitted as a `file.event` JSON-RPC notification by
/// the daemon. Sent for every active watch handle whose root covers the
/// changed path; consumers route by `watch_id`. Path is rig-relative
/// with POSIX separators.
///
/// `kind` is best-effort — `notify` events are debounced and reclassified
/// from disk state at emit time, so a quick rename-then-write may surface
/// as a single `Modified`. Renderers shouldn't rely on tight kind fidelity
/// for correctness; treat any event as "this path may have changed".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    /// Watch handle id this event belongs to (from
    /// `rpc::WatchResult::id` in the sibling `rpc` crate).
    pub watch_id: u64,
    pub kind: FileEventKind,
    /// Rig-relative path to the affected file or directory.
    pub path: String,
    /// File mtime in milliseconds since the Unix epoch when known. `None`
    /// for `Removed` events and on platforms that can't report it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEventKind {
    Created,
    Modified,
    Removed,
}
