//! @arch:layer(kg_store)
//! @arch:role(graph)
//! @arch:see(.yah/arch/authored/yah-managed-rigs-topology.mmd)
//!
//! `yah-kg-daemon` — runtime composition of the knowledge graph.
//!
//! Wraps a [`kg_store::Store`] in a `tokio::sync::RwLock`, owns an
//! [`kg_store::IndexerRegistry`], runs a `notify` file watcher, and
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
//! @yah:relay(R018, "yah-runner: provider trait + first non-Claude backend (Claude SDK lives in R028)")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R013)
//! @yah:gotcha("Pivoted 2026-04-28 from pi-mono integration. Reason: yah's value prop (per-ticket prelude, cache analytics, @yah:think mapping per provider, structured-event-shape stability) needs control over runner internals. Pi-mono is a reference implementation, not a dependency. Anthropic flows through R028's hand-rolled /v1/messages so we can hold the native tool_use protocol and cache_control markers without lossy abstraction; the cost-controlling Pro/Max OAuth path was originally why we owned the wire, but Anthropic's 2026-04-04 TOS change banned consumer OAuth in third-party tools (incl. their own Agent SDK by name) — so Claude-via-subscription now lives in the Process+MCP cell wrapping the `claude` CLI (R028 P3). The HTTP+Anthropic cell stays for the API-key path.")
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//! @arch:see(.yah/arch/authored/yah-roadmap-2026Q2.md)
//!
//! @yah:ticket(R018-T1, "Design doc: yah-runner trait + AgentEvent vocabulary + session JSONL format")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:gotcha("AgentEvent vocabulary is no longer greenfield — the Claude path (R028-F3) shipped concrete types in yah-kg/src/agent.rs (SessionId, Role, Message, AgentEvent { SessionStarted, TurnStarted, MessageDelta, TurnEnded, SessionEnded, Error }). The design doc should describe what exists, not invent from scratch. Extension slots (ToolCall/ToolResult, per-turn metadata cumulative_tokens/ring_depth/cache_hit_ratio) are non-breaking — add to the enum as runners grow needs.")
//! @yah:next("Extend .yah/arch/authored/yah-agent-runtime.md with a 'yah-runner' section OR write .yah/arch/authored/yah-runner.md")
//! @yah:next("Lift the existing kg::agent vocabulary (SessionId, Role, Message, AgentEvent) into the doc. Cover: Runner trait (consume Prelude → emit AgentEvent stream), backend impl pattern (HTTP-streaming + tool-use loop), first backend choice (defer until concrete need), session JSONL format (compatible with R028's prelude assembler output)")
//! @yah:next("Specify the rig-tagging seam: runtime AgentEvent stays runner-agnostic; the Tauri host wraps with RigAgentEvent { rigId, ...event } at emit time (mirrors RigEvent for ArchEvent). Other transports (CLI, JSON-RPC) follow the same pattern.")
//!
//! @yah:ticket(R018-F2, "Server-side Runner trait + first backend + session JSONL storage")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:gotcha("Vocabulary already exists in yah-kg/src/agent.rs (extracted from R028-F3 on 2026-04-28). Runner trait should have signature: start(prelude: Prelude, ticket_id: &str) -> SessionId + send(session_id, msg) -> Stream<AgentEvent> + stop(session_id). The Claude reference impl (app/tauri/src/agent.rs) is the source for what events to emit when — don't drift.")
//! @yah:next("Define Runner trait in yah-kg-daemon (or new yah-runner crate): start(prelude, ticket_id) -> SessionId; send(session_id, msg) -> Stream<AgentEvent>; stop(session_id). Use kg::agent::{SessionId, AgentEvent} as the contract.")
//! @yah:next("First backend deferred until concrete need — likely OpenAI or OpenAI-compat (Qwen, Ollama)")
//! @yah:next("Persist events to .yah/sessions/<session_id>.jsonl; arch_touch already resolves path:line from tool results — wire into AgentEvent::ToolResult once that variant lands (currently MessageDelta/TurnEnded only)")
//! @yah:handoff("trait + storage landed; backing off review to wire OpenAI-compat backend per user feedback (don't lock in the trait shape without a real backend exercising it)")
//! @yah:next("Add OpenAI-compat backend in yah-runner crate (covers openai, ollama, qwen all via Chat Completions + SSE)")
//!
//! @yah:ticket(R037-F17, "Subclass fallback rule evaluator: declarative ProbeCondition + metric collection (session usage, weekly tokens, connection health) + ConfigSwitch{FallbackTriggered/Recovered} broadcast")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R037)

pub mod path;
pub mod service;
pub mod snapshot;
mod snapshot_wire;
pub mod watcher;

pub use service::{DaemonError, KgService, ServiceConfig};
pub use snapshot::{
    default_snapshot_path, diff_fingerprints, fingerprint_rig, read_snapshot, write_snapshot,
    FileFingerprint, KgSnapshot, ReconcilePlan, SnapshotError, SNAPSHOT_VERSION,
};
pub use watcher::{WatcherHandle, WatcherKind};
