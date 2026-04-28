//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Wire types for the `arch.validate` RPC: [`Scope`], [`Severity`], and
//! [`Violation`]. The runtime evaluator lives in `yah-kg-validator`; this
//! module is just the contract so the daemon, the validator, and the
//! Tauri/JSON-RPC clients all agree on what the validation surface looks
//! like.
//!
//! `Scope` uses tagged-struct variants so the JSON shape is self-describing
//! (`{"scope":"all"}`, `{"scope":"subtree","root":...}`,
//! `{"scope":"file","path":"..."}`) — convenient for hand-written clients
//! and stable across additions.

use crate::ids::NodeId;
use serde::{Deserialize, Serialize};

/// What slice of the graph to validate. `All` walks every authored rule
/// in the index; `Subtree` restricts to anchors reachable from `root`
/// via outgoing `Contains` edges; `File` restricts to anchors authored
/// in one rig-relative file.
///
/// Defaults to [`Scope::All`] so a `validate` call with no args validates
/// everything — the common interactive case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum Scope {
    All,
    Subtree { root: NodeId },
    File { path: String },
}

impl Default for Scope {
    fn default() -> Self {
        Self::All
    }
}

/// Violation severity. The default is [`Severity::Error`] so an unrated
/// rule fails CI by default — authors who genuinely want a soft warning
/// must opt in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

/// One rule failure. The shape is intentionally flat so the UI can render
/// it in a list without recursing into rule-kind-specific payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// `rule_kind` from the offending `@yah:rule(...)` directive
    /// (`no-import-of`, `must-tag`, …). Unknown rule kinds parsed from
    /// source still emit a violation under their original kind string so
    /// the author can spot the typo.
    pub rule_kind: String,
    /// The structural node carrying the rule annotation.
    pub anchor: NodeId,
    /// Rig-relative path to the source line where the rule is authored.
    pub anchor_file: String,
    /// 1-based source line the rule annotation lives on.
    pub anchor_line: u32,
    /// The structural node that triggered the violation, when applicable.
    /// `None` for parse / vocabulary errors that aren't node-specific.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offending: Option<NodeId>,
    /// Convenience copy of the offending node's file (rig-relative). Lets
    /// the UI link the violation to a source location without an extra
    /// `arch.node` round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offending_file: Option<String>,
    /// 1-based start line of the offending node's source span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offending_line: Option<u32>,
    /// Human-readable message. Always present; the daemon formats it for
    /// CLI output and the UI surfaces it as a hover tooltip.
    pub message: String,
    pub severity: Severity,
}
