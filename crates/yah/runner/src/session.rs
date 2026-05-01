//! On-disk session log: `<rig_root>/.yah/sessions/<session_id>.jsonl`.
//!
//! Each line is one [`AgentEvent`] serialized as JSON. The format is
//! append-only and self-describing — `kind` tags every record (see
//! `kg::agent::AgentEvent`'s `#[serde(tag = "kind")]`), so
//! consumers can replay a session without external metadata.
//!
//! ## Why JSONL
//!
//! - **Append-only** matches how events arrive: SSE deltas land one at
//!   a time, and the natural write is "open, append, close" (or "keep
//!   the handle around for the session"). No re-encoding cost on each
//!   write.
//! - **Line-delimited** means partial writes don't corrupt earlier
//!   records — a crashed turn loses at most the trailing line.
//! - **Self-describing** via the `kind` tag, so dropping a session log
//!   on someone with no schema knowledge still parses cleanly.
//!
//! ## What's intentionally not here
//!
//! - **Compaction / retention** — the Claude pane and yah-runner both
//!   accumulate session logs per-ticket. A future janitor (R028 follow-
//!   up or a board archive hook) can prune logs older than N days; the
//!   runner shouldn't make that policy call.
//! - **Cross-session search** — that's a `yah board show <ticket>
//!   --history` job, not a runner concern.
//! - **Streaming reads** — replay returns the whole session in one
//!   shot. Any UI that wants progressive rehydration can build a
//!   streaming adapter on top; in practice session logs are KB-sized,
//!   not MB-sized.

use futures::stream::{Stream, StreamExt};
use serde_json;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use kg::agent::{AgentEvent, SessionId};

use crate::runner::EventStream;

/// Errors specific to [`SessionStore`] I/O. Most are surfaced to callers
/// as warnings — the runner shouldn't abort a turn just because the log
/// disk is full — but having distinct variants makes the failure mode
/// readable in tracing output.
#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("io error on session log: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error on session log: {0}")]
    Json(#[from] serde_json::Error),
    #[error("session id contains an unsafe path component: {0}")]
    UnsafeId(String),
}

/// Persists [`AgentEvent`]s to per-session JSONL files under a rig root.
///
/// Cheap to clone (an `Arc` internally) so backends can stash one in
/// their session record and keep wrapping streams.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// `<rig_root>/.yah/sessions`.
    base: PathBuf,
    /// One write-mutex per process keeps concurrent appends from
    /// interleaving lines on overlapping flushes. Per-session locks
    /// would scale better, but in practice each session writes from one
    /// task, so a single mutex is the simpler shape.
    write_lock: Mutex<()>,
}

impl SessionStore {
    /// Construct a store rooted at `<rig_root>/.yah/sessions`. Does not
    /// create the directory eagerly — that happens lazily on the first
    /// [`Self::append`] so a read-only rig (CLI inspection, snapshot
    /// tests) can still construct a store without side effects.
    pub fn new(rig_root: &Path) -> Self {
        let base = rig_root.join(".yah").join("sessions");
        Self {
            inner: Arc::new(Inner {
                base,
                write_lock: Mutex::new(()),
            }),
        }
    }

    /// `<rig_root>/.yah/sessions/<session_id>.jsonl`. Surfacing the
    /// path is useful for tests and for the eventual UI affordance that
    /// reveals where a session's log lives.
    pub fn path_for(&self, session_id: &SessionId) -> Result<PathBuf, SessionStoreError> {
        let safe = sanitize_id(session_id.as_str())?;
        Ok(self.inner.base.join(format!("{safe}.jsonl")))
    }

    /// Append one event. Creates the parent directory + log file on
    /// first write. Holds an internal mutex so concurrent appends from
    /// two tasks can't interleave bytes on the same flush.
    pub async fn append(
        &self,
        session_id: &SessionId,
        event: &AgentEvent,
    ) -> Result<(), SessionStoreError> {
        let path = self.path_for(session_id)?;
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');

        let _guard = self.inner.write_lock.lock().await;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(&line).await?;
        file.flush().await?;
        Ok(())
    }

    /// Read the full session log. Records that fail to parse (corrupt
    /// trailing line from an aborted write, schema drift in an old
    /// log) are skipped with a `tracing::warn` rather than aborting —
    /// a partly-replayable log is more useful than no log at all.
    pub async fn replay(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<AgentEvent>, SessionStoreError> {
        let path = self.path_for(session_id)?;
        let raw = match fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for (idx, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<AgentEvent>(trimmed) {
                Ok(ev) => out.push(ev),
                Err(e) => tracing::warn!(
                    session_id = %session_id.as_str(),
                    line = idx + 1,
                    error = %e,
                    "skipping unparseable session log line",
                ),
            }
        }
        Ok(out)
    }
}

/// Reject session ids that would escape the sessions directory or
/// expand to surprising filenames. The Claude mint scheme produces
/// `session:<hex>` which becomes `session:<hex>.jsonl` on disk — the
/// colon is fine on every host filesystem we target. We only reject
/// path separators and parent traversal.
fn sanitize_id(id: &str) -> Result<&str, SessionStoreError> {
    if id.is_empty() || id == "." || id == ".." {
        return Err(SessionStoreError::UnsafeId(id.to_string()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(SessionStoreError::UnsafeId(id.to_string()));
    }
    Ok(id)
}

/// Wrap an [`EventStream`] so each *durable* yielded event is appended
/// to the session log before being forwarded to the consumer. Persistence
/// failures are logged at `warn` and swallowed — a runner shouldn't
/// drop user-visible deltas because the disk is full.
///
/// `MessageDelta` events are intentionally **not** persisted:
/// [`AgentEvent::TurnEnded`] / [`AgentEvent::TurnFailed`] each carry the
/// full accumulated assistant text, so persisting per-token deltas
/// would just duplicate the same bytes line-by-line. Skipping them
/// shrinks a typical 5K-token reply's JSONL log from ~25 KB across
/// hundreds of lines to a single ~3 KB terminal-event line. The
/// consumer (renderer) still receives every delta in real time — only
/// the on-disk log shrinks.
///
/// Backends call this once per [`Runner::send`](crate::Runner::send)
/// to keep their own emit-loop free of persistence concerns.
pub fn tap_stream(
    store: SessionStore,
    session_id: SessionId,
    inner: EventStream,
) -> EventStream {
    let stream = inner.then(move |event| {
        let store = store.clone();
        let session_id = session_id.clone();
        async move {
            if !matches!(event, AgentEvent::MessageDelta { .. }) {
                if let Err(e) = store.append(&session_id, &event).await {
                    tracing::warn!(
                        session_id = %session_id.as_str(),
                        error = %e,
                        "session log append failed; dropping persistence for this event",
                    );
                }
            }
            event
        }
    });
    Box::pin(stream) as Pin<Box<dyn Stream<Item = AgentEvent> + Send>>
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use tempfile::TempDir;
    use kg::agent::SessionId;

    fn delta(id: &SessionId, text: &str) -> AgentEvent {
        AgentEvent::MessageDelta {
            session_id: id.clone(),
            text: text.to_string(),
        }
    }

    #[tokio::test]
    async fn append_then_replay_round_trips_event_order() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        let id = SessionId::new("session:abc12345");

        store
            .append(
                &id,
                &AgentEvent::TurnStarted {
                    session_id: id.clone(),
                },
            )
            .await
            .unwrap();
        store.append(&id, &delta(&id, "hello ")).await.unwrap();
        store.append(&id, &delta(&id, "world")).await.unwrap();
        store
            .append(
                &id,
                &AgentEvent::TurnEnded {
                    session_id: id.clone(),
                    text: "hello world".into(),
                    stop_reason: Some("end_turn".into()),
                    usage: None,
                },
            )
            .await
            .unwrap();

        let replay = store.replay(&id).await.unwrap();
        assert_eq!(replay.len(), 4);
        assert!(matches!(replay[0], AgentEvent::TurnStarted { .. }));
        assert!(matches!(replay[3], AgentEvent::TurnEnded { .. }));
    }

    #[tokio::test]
    async fn replay_returns_empty_when_log_does_not_exist() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        let id = SessionId::new("session:00000000");
        let out = store.replay(&id).await.unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn replay_skips_unparseable_lines_without_aborting() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        let id = SessionId::new("session:11111111");
        // Append one good event, then poke a corrupt line in by hand.
        store
            .append(
                &id,
                &AgentEvent::TurnStarted {
                    session_id: id.clone(),
                },
            )
            .await
            .unwrap();
        let path = store.path_for(&id).unwrap();
        let mut existing = std::fs::read_to_string(&path).unwrap();
        existing.push_str("not json at all\n");
        existing.push_str(
            &serde_json::to_string(&AgentEvent::SessionEnded {
                session_id: id.clone(),
            })
            .unwrap(),
        );
        existing.push('\n');
        std::fs::write(&path, existing).unwrap();

        let replay = store.replay(&id).await.unwrap();
        assert_eq!(replay.len(), 2);
        assert!(matches!(replay[0], AgentEvent::TurnStarted { .. }));
        assert!(matches!(replay[1], AgentEvent::SessionEnded { .. }));
    }

    #[tokio::test]
    async fn unsafe_session_ids_refuse_path_resolution() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        for bad in ["..", "../escape", "a/b", "a\\b", "with\0null", ""] {
            let id = SessionId::new(bad);
            assert!(matches!(
                store.path_for(&id),
                Err(SessionStoreError::UnsafeId(_))
            ));
        }
    }

    #[tokio::test]
    async fn tap_stream_forwards_all_events_but_skips_delta_persistence() {
        // Consumer (renderer) sees every event in order, including the
        // delta. Persistence skips the delta — TurnEnded already carries
        // the full text, so duplicating per-token bytes in the log is
        // just waste.
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        let id = SessionId::new("session:deadbeef");
        let events = vec![
            AgentEvent::TurnStarted {
                session_id: id.clone(),
            },
            delta(&id, "hi"),
            AgentEvent::TurnEnded {
                session_id: id.clone(),
                text: "hi".into(),
                stop_reason: None,
                usage: None,
            },
        ];
        let stream: EventStream = Box::pin(stream::iter(events));
        let mut tapped = tap_stream(store.clone(), id.clone(), stream);
        let mut consumed = Vec::new();
        while let Some(ev) = tapped.next().await {
            consumed.push(ev);
        }
        // Live forwarding: all three events reach the consumer.
        assert_eq!(consumed.len(), 3);
        assert!(matches!(consumed[1], AgentEvent::MessageDelta { .. }));

        // Persistence: delta is filtered out, only the durable terminal
        // events land on disk.
        let replay = store.replay(&id).await.unwrap();
        assert_eq!(replay.len(), 2);
        assert!(matches!(replay[0], AgentEvent::TurnStarted { .. }));
        assert!(matches!(replay[1], AgentEvent::TurnEnded { .. }));
    }

    #[tokio::test]
    async fn tap_stream_persists_turn_failed_with_partial_text() {
        // TurnFailed is symmetric to TurnEnded — the runner accumulated
        // some text before failure, the persisted event carries it.
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::new(tmp.path());
        let id = SessionId::new("session:cafef00d");
        let events = vec![
            AgentEvent::TurnStarted {
                session_id: id.clone(),
            },
            delta(&id, "partial "),
            delta(&id, "reply"),
            AgentEvent::TurnFailed {
                session_id: id.clone(),
                text: "partial reply".into(),
                message: "transport: connection reset".into(),
            },
        ];
        let stream: EventStream = Box::pin(stream::iter(events));
        let mut tapped = tap_stream(store.clone(), id.clone(), stream);
        while tapped.next().await.is_some() {}

        let replay = store.replay(&id).await.unwrap();
        assert_eq!(replay.len(), 2);
        match &replay[1] {
            AgentEvent::TurnFailed { text, message, .. } => {
                assert_eq!(text, "partial reply");
                assert!(message.contains("transport"));
            }
            other => panic!("expected TurnFailed, got {other:?}"),
        }
    }
}
