//! Provider-agnostic tool registry surface.
//!
//! The runner sees this thin trait — list schemas, execute by name. The
//! concrete [`Tool`] trait (with its `KgService` + `rig_root` access) lives
//! host-side in `app/tauri/src/agent_tools.rs` because it depends on
//! daemon types this crate doesn't see. Hosts wrap their concrete tools
//! in something that implements [`ToolRegistry`] and hand it to
//! [`crate::OpenAiCompatRunner::with_tools`].
//!
//! ## Why split the trait
//!
//! Function-calling protocol (request body's `tools[]`, SSE
//! `tool_calls[]` deltas, dispatch loop) is provider-agnostic and lives
//! in this crate. *What* a tool does — read a rig file, query the KG,
//! mutate Rust source through rs-hack — is host-specific and depends on
//! `KgService`, `RigId`, and the on-disk rig root. Putting all of that
//! behind one trait would either bloat `yah-runner` with daemon deps or
//! force the host to invent runner internals.
//!
//! The split keeps each crate small: `yah-runner` cares only about the
//! string name, the JSON schema (passed to the LLM), and the JSON
//! result (passed back to the LLM). Approval gating (R031-F5) and write
//! semantics (R031-F4) layer on top — the runner doesn't see them.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// One tool's manifest as the LLM will see it. The runner dumps these
/// into the request body's `tools[]` array on every turn.
///
/// `name` must match exactly the string the runner will pass to
/// [`ToolRegistry::execute`] when the LLM emits a `tool_calls[]` entry —
/// it's the identity key for dispatch. `description` is what the LLM
/// reads when deciding which tool to call; keep it short, action-
/// oriented, and surface any sandbox / cost gotchas. `input_schema` is
/// JSON Schema the LLM uses to construct arguments; OpenAI-compat
/// providers wrap it in `{ type: "function", function: { name,
/// description, parameters: input_schema } }` at request time, so
/// implementations should produce a `type: "object"` schema with the
/// argument fields under `properties`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of one tool invocation. `ok = false` is *not* a runner error —
/// it rides back to the LLM as the next-turn input so the model can
/// reason about and retry. Runner-level failures (missing tool name,
/// arg-parse failure before dispatch) are the only thing that surface
/// as `AgentEvent::Error`; everything a tool itself reports is in-band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutcome {
    /// `true` when the tool produced its declared output. `false` when
    /// the tool reached its own error path (file not found, KG node id
    /// missing, sandbox escape, …) — `result` carries a structured
    /// description for the LLM to reason about.
    pub ok: bool,
    /// Tool-specific JSON. Read tools tend to emit
    /// `{ "content": "…", "bytes": N }`; KG tools emit their natural
    /// response shape. Failure path surfaces as e.g.
    /// `{ "error": "…", "kind": "sandbox_escape" }`.
    pub result: serde_json::Value,
}

impl ToolOutcome {
    pub fn ok(result: serde_json::Value) -> Self {
        Self { ok: true, result }
    }

    /// Convenience for the `ok = false` failure path. The LLM sees this
    /// as `{ "error": <message>, ...rest }` so it can correct course.
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: serde_json::json!({ "error": message.into() }),
        }
    }
}

/// Provider-agnostic tool surface the runner depends on. One
/// implementation per (rig, registry) — hosts typically build it from a
/// `ToolContext` that points at the active rig's `KgService` and root
/// path.
///
/// `Send + Sync + 'static` because the runner stores it behind an `Arc`
/// and a session may dispatch tools from spawned tasks.
#[async_trait]
pub trait ToolRegistry: Send + Sync + 'static {
    /// Static manifest the runner injects into every provider request's
    /// `tools[]` array. Must be cheap — called once per turn.
    fn schemas(&self) -> Vec<ToolSchema>;

    /// Invoke a tool by `name` with the LLM-produced JSON `args`. The
    /// runner has no view of approval — write tools must already have
    /// passed the host's gate before this is called (P5 wiring).
    async fn execute(&self, name: &str, args: serde_json::Value) -> ToolOutcome;
}
