//! @arch:layer(kg_store)
//! @arch:role(bridge)
//! @arch:see(.yah/arch/authored/yah-runner.md)
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//! @arch:see(.yah/arch/authored/yah-roadmap-2026Q2.md)
//!
//! `yah-runner` — provider-agnostic agent runtime contract.
//!
//! One of three cells in the runtime matrix
//! (`.yah/arch/authored/yah-agent-runtime.md`'s "Composable runtime matrix"):
//! this crate is the **HTTP + OpenAI-compat** cell, serving OpenAI,
//! Together, vLLM, and Ollama (local + cloud). The sibling cells are
//! **HTTP + Anthropic-native** in `app/tauri/src/agent.rs` (hand-rolled
//! `/v1/messages` so we can hold the native `tool_use` / `tool_result`
//! protocol and `cache_control` markers without lossy abstraction) and
//! **Process + MCP** wrapping the `claude` CLI as a subprocess (R028 P3,
//! policy-durable against Anthropic's 2026-04-04 ban on Pro/Max OAuth in
//! third-party tools). All three runners share a `Prelude` consumer and
//! `AgentEvent` emitter so the renderer stays wire-agnostic.
//!
//! ## Contract
//!
//! Both runners consume a fully-assembled [`kg::prelude::Prelude`] and
//! emit the same [`kg::agent::AgentEvent`] stream. That keeps the
//! renderer (and any other event subscriber) runner-agnostic — only the
//! dispatch decision changes between backends, not the surface they emit.
//!
//! - [`Runner::start`] opens a session for one ticket. Returns a stable
//!   [`SessionId`]; the implementation also emits an [`AgentEvent::SessionStarted`]
//!   on its first turn (or when [`Runner::send`] is first invoked) — the
//!   Claude reference impl emits at start, which is the canonical shape.
//! - [`Runner::send`] appends a user turn and returns a [`EventStream`]
//!   that yields the assistant's reply as deltas, a final
//!   [`AgentEvent::TurnEnded`], and any inline errors. The stream is
//!   per-turn — closing it does not close the session.
//! - [`Runner::stop`] aborts any in-flight turn and tears down the
//!   session. Idempotent — returns `false` when the session id was
//!   already gone.
//!
//! ## What lives outside the trait
//!
//! - **Prelude assembly** — owned by `yah-kg-daemon::KgService::assemble_prelude`.
//!   The trait takes a fully-built `Prelude` so backends don't depend on
//!   the daemon. Callers compose: assemble → start → send.
//! - **Rig identity** — runtime types stay runner-agnostic. The Tauri
//!   host wraps emits in `RigAgentEvent { rigId, ...event }` (mirrors
//!   the `RigEvent` envelope around `ArchEvent`); other transports (CLI,
//!   JSON-RPC over SSH) follow the same pattern.
//! - **Persistence** — opt-in via [`SessionStore`]. Backends that want
//!   on-disk replay history wrap their stream with [`tap_stream`]; one-
//!   shot or test-only backends can skip it entirely.
//!
//! ## First backend
//!
//! Deferred until a concrete need lands — likely OpenAI-compat
//! (Qwen, Ollama) since the API shape is well-understood and gives users
//! a working alternative the day the trait ships. Implementations live
//! in their own modules under this crate (e.g. `openai_compat::OpenAiRunner`).
//!
//! @yah:relay(R032, "Remote-resident agent: yah-agentd + headless runner + on-disk credential vault")
//! @yah:status(open)
//! @yah:assignee(agent:claude)
//! @yah:handoff("Architecture committed: agent loop runs in a yah-agentd long-running daemon on the remote (systemd user unit, Unix socket), yah serve --stdio becomes a thin proxy, Tauri unchanged at the wire level. Credentials encrypted at rest (AES-256-GCM, per-host machine.key in ~/.config/yah/) — defends against dragnet exfil, not against same-user processes. Trust posture: cloud VM is paid-for + trusted, so keys-on-host is acceptable.")
//! @yah:next("T1 lift yah-runner agent loop into a headless library surface (decouple from Tauri AppHandle/Emitter)")
//! @yah:next("T2 yah keys CLI + ~/.config/yah/{machine.key,credentials.enc} AES-GCM storage")
//! @yah:next("T3 yah-agentd binary + Unix-socket JSON-RPC server + systemd user unit + loginctl enable-linger in deploy-remote.sh")
//! @yah:next("T4 yah serve --stdio proxy mode: forward agent.* methods to ~/yah-agentd.sock; arch.* stay direct")
//! @yah:next("T5 RigBackend::Remote agent_* dispatch in yah-rpc-ssh::SshRpcClient mirroring local agent commands")
//! @yah:next("T6 Tauri session pickup UX: agent.list_sessions on rig reattach + offer to resume")
//!
//! @yah:ticket(R032-T2, "Lift yah-runner agent loop into headless library surface (decouple from Tauri AppHandle)")
//! @yah:status(handoff)
//! @yah:assignee(agent:claude)
//! @yah:parent(R032)
//! @yah:handoff("Sink trait at the right layer: runner::SessionEventSink (sink.rs) + Tauri-side TauriRigSink<'a> impl in app/tauri/src/agent.rs. Existing emit_event chokepoint routes through the trait; 40+ callers unchanged. Tests: 2 sink units (yah-runner) + 44 agent (yah-tauri --lib agent) green. Also: 3-line snapshot_wire.rs fix for the R017-T9 missing 'at' field that blocked yah-tauri build per R030 gotcha. Session-state lift (AgentSession/RunnerSession/AgentSessions move to yah-runner) is the bigger follow-up — recommend as a separate sub-ticket; the trait makes it mechanical.")
//! @yah:next("Spawn R032-T7: lift AgentSession + RunnerSession + AgentSessions into yah-runner. Today they live in app/tauri/src/agent.rs (~2000 LOC). With SessionEventSink in place, every (&AppHandle, &RigId) param becomes (&dyn SessionEventSink). Mechanical, ~1-2 days.")
//! @yah:next("T3 unblocked: yah-agentd binary implements SessionEventSink to push JSON-RPC notifications, no Tauri dependency. Trait surface final: emit(&AgentEvent), no rig_id (one rig per socket), no Result (host-side concern).")
//!
//! @yah:ticket(R037-F15, "Connection health: per-runner error classifier + auto-retry probe (Retry-After) + status broadcast")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R037)

mod openai_compat;
mod runner;
mod session;
mod session_id;
mod sink;
mod tool;

pub use openai_compat::{
    list_openai_compat_models, OpenAiCompatConfig, OpenAiCompatRunner,
};
pub use runner::{EventStream, Runner, RunnerError};
pub use session::{tap_stream, SessionStore, SessionStoreError};
pub use session_id::mint_session_id;
pub use sink::{FnSink, NullSink, SessionEventSink};
pub use tool::{ToolOutcome, ToolRegistry, ToolSchema};

// Re-export the contract types so backends only depend on this crate.
pub use kg::agent::{AgentEvent, Message, Role, SessionId};
pub use kg::prelude::Prelude;
