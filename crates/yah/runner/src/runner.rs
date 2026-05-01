//! The [`Runner`] trait — the surface every non-Claude backend implements.
//!
//! Kept deliberately narrow: three methods, all async. Anything fancier
//! (session listing, prelude metadata) is the host's job (the Tauri
//! command surface in R018-F3) and composes on top of these primitives.

use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use thiserror::Error;
use kg::agent::{AgentEvent, SessionId};
use kg::prelude::Prelude;

/// Boxed `Stream` of [`AgentEvent`]s. One per [`Runner::send`] call.
///
/// `'static` because the stream typically outlives the borrowed `&self`
/// of `send` — backends own the underlying state in `Arc`s and emit
/// from a spawned task.
pub type EventStream = Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>;

/// Errors a [`Runner`] can surface synchronously (i.e. before a turn has
/// been spawned). Mid-turn failures ride [`AgentEvent::TurnFailed`] on
/// the stream — symmetric to [`AgentEvent::TurnEnded`], it carries the
/// accumulated assistant text up to the failure point. The session
/// stays open in either case so the user can retry without re-paying
/// the prelude assembly cost. [`AgentEvent::Error`] is reserved for
/// command-shaped failures with no in-flight turn (e.g. the host's
/// pre-stream `runner.send` itself returning [`RunnerError`]).
#[derive(Debug, Error)]
pub enum RunnerError {
    /// Engine selected by the ticket isn't one this runner serves.
    /// The dispatch matrix in R018-F3 should normally catch this before
    /// the runner is invoked; surfacing it here is defence-in-depth for
    /// callers that mint a Runner directly.
    #[error("engine not supported by this runner: {engine}")]
    UnsupportedEngine { engine: String },
    /// Session lookup failed — usually a stale id from the renderer.
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// Backend-specific configuration is missing (API key, endpoint URL,
    /// model name, …). Carries a user-readable hint pointing at the
    /// settings UI.
    #[error("configuration missing: {0}")]
    Config(String),
    /// I/O / transport failure during setup. Once a turn is in flight,
    /// transport errors should ride [`AgentEvent::TurnFailed`] on the
    /// stream instead — that variant carries the accumulator so partial
    /// replies aren't lost on a mid-stream disconnect.
    #[error("transport error: {0}")]
    Transport(String),
}

/// Provider-agnostic agent runner.
///
/// One implementation per provider family (OpenAI-compat covers OpenAI,
/// Qwen, Ollama, vLLM, …). The Claude path lives in
/// `app/tauri/src/agent.rs` and intentionally does **not** implement
/// this trait — it depends on Tauri primitives (`AppHandle`, the
/// `agent:event` channel) that runtime types here can't see.
/// Dispatch between Claude and yah-runner is the host's job; both sides
/// emit the same [`AgentEvent`] vocabulary so the renderer can't tell
/// them apart.
///
/// Implementations should be `Arc<Self>`-shareable: hosts hold one
/// runner instance for the process lifetime and dispatch concurrent
/// sessions through it.
#[async_trait]
pub trait Runner: Send + Sync {
    /// Open a session for `ticket_id` using the assembled `prelude`.
    /// Returns the freshly-minted [`SessionId`]. The implementation
    /// retains whatever state it needs (history, cached prefix, model)
    /// keyed by the session id.
    ///
    /// `prelude` is consumed because the runner caches the rendered
    /// text plus engine/think metadata internally; callers who want to
    /// inspect the prelude after start should clone before calling.
    async fn start(
        &self,
        prelude: Prelude,
        ticket_id: &str,
    ) -> Result<SessionId, RunnerError>;

    /// Append a user turn and return a stream of [`AgentEvent`]s. The
    /// stream is bounded to one turn — implementations close it after
    /// emitting **exactly one** of [`AgentEvent::TurnEnded`] (clean
    /// completion) or [`AgentEvent::TurnFailed`] (mid-stream failure
    /// carrying the partial accumulator). Stream closure does not close
    /// the session; subsequent `send` calls reuse the same id.
    ///
    /// The first event of a session's first turn is typically
    /// [`AgentEvent::SessionStarted`] — see the Claude reference impl
    /// for the exact emission pattern.
    async fn send(
        &self,
        session_id: &SessionId,
        message: String,
    ) -> Result<EventStream, RunnerError>;

    /// Abort any in-flight turn and tear down the session. Idempotent —
    /// returns `Ok(false)` when no session matched.
    async fn stop(&self, session_id: &SessionId) -> Result<bool, RunnerError>;
}
