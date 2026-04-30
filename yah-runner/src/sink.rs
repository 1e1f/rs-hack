//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! [`SessionEventSink`] — receives [`AgentEvent`]s from a running session.
//!
//! Different hosts implement different sinks: the Tauri host wraps each
//! event in `RigAgentEvent { rig_id, ... }` and emits to the renderer
//! via `AppHandle::emit("agent:event", _)`; `yah-agentd` (R032-T3)
//! pushes JSON-RPC notifications to the SSH-channel client; CLI hosts
//! might just println. Sessions don't care which.
//!
//! The trait is deliberately tiny — `&AgentEvent` in, `()` out, no
//! result. Sink errors (renderer disconnected, socket closed) are
//! host-side concerns to log + recover from; the agent loop should
//! keep grinding regardless. If a sink needs to backpressure, that
//! shape lands in a separate `BackpressuredSink` trait when we have a
//! concrete need.
//!
//! Rig identity intentionally lives at the host layer, not the trait:
//! a single Tauri host serves N rigs and stamps each emit with the
//! right `rig_id`, while `yah-agentd` serves exactly one rig per
//! socket so the `rig_id` wrapper is unnecessary. Sink impls do
//! whatever wrapping their host needs.

use yah_kg::agent::AgentEvent;

/// Receives `AgentEvent`s emitted by an agent session.
pub trait SessionEventSink: Send + Sync {
    fn emit(&self, event: &AgentEvent);
}

/// No-op sink for tests, dry-runs, and one-shot programmatic invocations
/// that don't care about the event stream. Intentionally not the
/// default for real runners — silent event drops on a real session
/// would be a hard-to-spot bug.
pub struct NullSink;

impl SessionEventSink for NullSink {
    fn emit(&self, _event: &AgentEvent) {}
}

/// Adapter that fans every event into a closure. Useful for tests
/// (`Arc::new(FnSink::new(move |e| tx.send(e.clone())))`) and for the
/// CLI host where "print to stdout" is a one-liner.
pub struct FnSink<F: Fn(&AgentEvent) + Send + Sync>(pub F);

impl<F: Fn(&AgentEvent) + Send + Sync> FnSink<F> {
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F: Fn(&AgentEvent) + Send + Sync> SessionEventSink for FnSink<F> {
    fn emit(&self, event: &AgentEvent) {
        (self.0)(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use yah_kg::agent::SessionId;

    fn fixture_session_started(id: &str) -> AgentEvent {
        AgentEvent::SessionStarted {
            session_id: SessionId(id.to_string()),
            ticket_id: "T".to_string(),
            engine: "test:engine".to_string(),
            cache_key: "k".to_string(),
            estimated_tokens: 0,
            ring_depth: 0.0,
        }
    }

    fn fixture_turn_ended(id: &str) -> AgentEvent {
        AgentEvent::TurnEnded {
            session_id: SessionId(id.to_string()),
            text: String::new(),
            stop_reason: None,
            usage: None,
        }
    }

    #[test]
    fn null_sink_swallows_events() {
        let sink: Box<dyn SessionEventSink> = Box::new(NullSink);
        sink.emit(&fixture_session_started("test"));
    }

    #[test]
    fn fn_sink_forwards_to_closure() {
        let captured: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::clone(&captured);
        let sink = FnSink::new(move |e: &AgentEvent| cap.lock().unwrap().push(e.clone()));
        sink.emit(&fixture_session_started("a"));
        sink.emit(&fixture_turn_ended("a"));
        let log = captured.lock().unwrap();
        assert_eq!(log.len(), 2);
    }
}
