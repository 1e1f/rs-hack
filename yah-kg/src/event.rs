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
