//! @arch:layer(rpc)
//! @arch:role(transport)
//! @arch:thread(async_io)
//!
//! Transport-agnostic JSON-RPC 2.0 client session.
//!
//! [`JsonRpcSession::spawn`] takes any `AsyncRead + AsyncWrite` pair —
//! the SSH channel stdout/stdin in production, an in-memory pipe in
//! tests — and runs the request/response multiplex on top. The session
//! is `Clone` (cheaply: `Arc` of internals) so multiple concurrent
//! callers can `call(...)` without serializing, while a single reader
//! task drains the framed response stream and dispatches each frame to
//! the right pending future.
//!
//! Framing: line-delimited JSON. Every outbound write emits one frame
//! ending in `\n`. Inbound frames are parsed line-by-line. This matches
//! `yah serve --stdio` (see `yah/src/serve.rs`) byte-for-byte.
//!
//! Notifications (`arch:event`) fan out via a [`tokio::sync::broadcast`]
//! channel so subscribers can be added/removed without the session
//! caring. Lagged subscribers see [`broadcast::error::RecvError::Lagged`]
//! and can resubscribe — same contract `KgService::subscribe` exposes.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, oneshot, Mutex};
use kg::event::ArchEvent;

/// Default broadcast channel capacity for `arch:event` notifications.
/// Sized so a brief renderer hiccup (window backgrounded, devtools
/// attached) doesn't drop frames; lagged subscribers still see
/// [`broadcast::error::RecvError::Lagged`] eventually.
const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Error)]
pub enum RpcError {
    /// Transport went away mid-call: ssh died, the kernel sent SIGPIPE,
    /// the remote daemon exited, the read-task drained EOF. The pending
    /// future for an in-flight call resolves with this.
    #[error("transport closed")]
    TransportClosed,
    /// Daemon answered with a JSON-RPC error frame. `code` mirrors the
    /// constants in `yah/src/serve.rs::RpcError` (-32601 method-not-found,
    /// -32602 invalid-params, -32603 internal, -32700 parse-error).
    #[error("daemon error {code}: {message}")]
    Daemon { code: i32, message: String },
    /// Could not encode the outbound params (rare — only triggers if a
    /// caller hands us a value that fails `serde_json::to_value`).
    #[error("encode error: {0}")]
    Encode(serde_json::Error),
    /// Could not decode the inbound result into the caller's expected
    /// type. Usually means the daemon is on a newer wire version.
    #[error("decode error: {0}")]
    Decode(serde_json::Error),
    /// Best-effort I/O error context bubbling up from the writer half.
    /// Reads close the session via [`RpcError::TransportClosed`] instead
    /// of surfacing here.
    #[error("io error: {0}")]
    Io(String),
}

/// Subscribe to `arch:event` notifications. Returned by
/// [`JsonRpcSession::subscribe_events`]; thin wrapper around a
/// [`broadcast::Receiver`] so callers don't have to depend on tokio's
/// re-export shape directly.
pub struct ArchEventStream {
    rx: broadcast::Receiver<ArchEvent>,
}

impl ArchEventStream {
    pub async fn recv(&mut self) -> Result<ArchEvent, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

/// One in-flight call's slot. The reader task drops the sender when it
/// resolves; the caller awaits the receiver. If the session closes the
/// task takes the whole map and drops every sender — every pending
/// caller observes [`RpcError::TransportClosed`].
type Pending = oneshot::Sender<Result<Value, RpcError>>;

struct Inner {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    /// Held by senders writing outbound frames. Wrapped in a mutex so
    /// concurrent `call(...)` invocations don't interleave bytes inside
    /// one frame; the mutex guards the writer half end-to-end.
    writer: Mutex<Box<dyn AsyncWrite + Send + Unpin>>,
    /// `false` once the reader task hits EOF / a fatal error. New
    /// `call(...)` invocations short-circuit to TransportClosed.
    open: std::sync::atomic::AtomicBool,
    events: broadcast::Sender<ArchEvent>,
}

/// JSON-RPC session over a generic `AsyncRead + AsyncWrite` pair.
///
/// Cheap to clone (`Arc` of internals). Drop the last clone to close
/// the session — the reader task auto-exits when its read half
/// terminates, and the writer half is dropped along with the inner
/// state.
#[derive(Clone)]
pub struct JsonRpcSession {
    inner: Arc<Inner>,
}

impl JsonRpcSession {
    /// Wire the session over the given duplex. Spawns a tokio task that
    /// reads frames until the read half hits EOF; the task terminates
    /// the session by marking it closed and dropping every pending
    /// sender so in-flight calls resolve with TransportClosed.
    pub fn spawn<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let (events_tx, _events_rx) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            writer: Mutex::new(Box::new(write)),
            open: std::sync::atomic::AtomicBool::new(true),
            events: events_tx,
        });
        spawn_reader(Arc::clone(&inner), read);
        Self { inner }
    }

    /// Subscribe to `arch:event` notifications. Each subscriber gets its
    /// own buffered receiver — late joiners only see frames published
    /// after they subscribed.
    pub fn subscribe_events(&self) -> ArchEventStream {
        ArchEventStream {
            rx: self.inner.events.subscribe(),
        }
    }

    /// Whether the session is still believed open. Becomes `false` the
    /// instant the reader task observes EOF or a fatal frame error.
    pub fn is_open(&self) -> bool {
        self.inner.open.load(Ordering::Acquire)
    }

    /// Issue one request and await its response. Generic over the params
    /// (any `Serialize`) and the result (any `DeserializeOwned`); the
    /// typed methods on [`crate::client::SshRpcClient`] are thin
    /// wrappers around this.
    pub async fn call<P, R>(&self, method: &str, params: &P) -> Result<R, RpcError>
    where
        P: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        if !self.is_open() {
            return Err(RpcError::TransportClosed);
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let params_value = serde_json::to_value(params).map_err(RpcError::Encode)?;
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params_value,
        });
        let bytes = serde_json::to_vec(&frame).map_err(RpcError::Encode)?;

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.inner.pending.lock().await;
            pending.insert(id, tx);
        }

        // Hold the writer lock for the whole frame so concurrent calls
        // can't interleave their bytes mid-line.
        {
            let mut w = self.inner.writer.lock().await;
            if let Err(e) = w.write_all(&bytes).await {
                self.fail_pending(id).await;
                return Err(RpcError::Io(e.to_string()));
            }
            if let Err(e) = w.write_all(b"\n").await {
                self.fail_pending(id).await;
                return Err(RpcError::Io(e.to_string()));
            }
            if let Err(e) = w.flush().await {
                self.fail_pending(id).await;
                return Err(RpcError::Io(e.to_string()));
            }
        }

        match rx.await {
            Ok(Ok(value)) => serde_json::from_value(value).map_err(RpcError::Decode),
            Ok(Err(e)) => Err(e),
            // Sender dropped without sending — reader task closed the
            // session and drained every pending future.
            Err(_) => Err(RpcError::TransportClosed),
        }
    }

    async fn fail_pending(&self, id: u64) {
        let mut pending = self.inner.pending.lock().await;
        pending.remove(&id);
    }
}

fn spawn_reader<R>(inner: Arc<Inner>, read: R)
where
    R: AsyncRead + Send + Unpin + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(read).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    handle_frame(&inner, &line).await;
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(error = %e, "json-rpc session read error; closing");
                    break;
                }
            }
        }
        // Mark closed first so new calls short-circuit, then drain
        // pending senders so in-flight calls resolve with
        // TransportClosed instead of hanging.
        inner.open.store(false, Ordering::Release);
        let pending: HashMap<u64, Pending> = {
            let mut guard = inner.pending.lock().await;
            std::mem::take(&mut *guard)
        };
        for (_id, tx) in pending {
            let _ = tx.send(Err(RpcError::TransportClosed));
        }
    });
}

#[derive(Deserialize)]
struct Frame {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<FrameError>,
}

#[derive(Deserialize)]
struct FrameError {
    code: i32,
    message: String,
}

async fn handle_frame(inner: &Arc<Inner>, line: &str) {
    let frame: Frame = match serde_json::from_str(line) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, line = %line, "malformed json-rpc frame; dropping");
            return;
        }
    };
    // Notification: no id, has method. Currently only `arch:event`.
    if frame.id.is_none() {
        if let (Some(method), Some(params)) = (frame.method, frame.params) {
            if method == "arch:event" {
                match serde_json::from_value::<ArchEvent>(params) {
                    Ok(event) => {
                        // Receiver count == 0 is fine: nobody subscribed
                        // yet, the event is dropped silently.
                        let _ = inner.events.send(event);
                    }
                    Err(e) => tracing::warn!(error = %e, "arch:event failed to deserialize"),
                }
            } else {
                tracing::debug!(method = %method, "unknown notification method");
            }
        }
        return;
    }
    // Response: has id; result XOR error.
    let id = match frame.id.and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => {
            tracing::warn!("response frame had non-numeric id; dropping");
            return;
        }
    };
    let tx = {
        let mut pending = inner.pending.lock().await;
        pending.remove(&id)
    };
    let Some(tx) = tx else {
        tracing::debug!(id, "response for unknown id; daemon may have echoed a stale frame");
        return;
    };
    if let Some(err) = frame.error {
        let _ = tx.send(Err(RpcError::Daemon {
            code: err.code,
            message: err.message,
        }));
    } else {
        let _ = tx.send(Ok(frame.result.unwrap_or(Value::Null)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::duplex;

    /// Minimal fake server that reads one request, parrots back a
    /// `result` containing `{"echoed": params}`. Useful for round-trip
    /// tests without a real `yah serve --stdio`.
    async fn echo_server<R, W>(read: R, mut write: W)
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let req: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let params = req.get("params").cloned().unwrap_or(Value::Null);
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "echoed": params }
            });
            let mut bytes = serde_json::to_vec(&resp).unwrap();
            bytes.push(b'\n');
            let _ = write.write_all(&bytes).await;
            let _ = write.flush().await;
        }
    }

    #[tokio::test]
    async fn round_trip_one_call() {
        let (client_io, server_io) = duplex(8 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);
        tokio::spawn(echo_server(server_read, server_write));
        let session = JsonRpcSession::spawn(client_read, client_write);

        let resp: serde_json::Value = session
            .call("arch.stats", &serde_json::json!({"hello": "world"}))
            .await
            .expect("echo round-trip");
        assert_eq!(resp["echoed"]["hello"], "world");
    }

    #[tokio::test]
    async fn concurrent_calls_dispatch_to_correct_waiters() {
        let (client_io, server_io) = duplex(8 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);
        tokio::spawn(echo_server(server_read, server_write));
        let session = JsonRpcSession::spawn(client_read, client_write);

        let s1 = session.clone();
        let s2 = session.clone();
        let p1 = serde_json::json!({ "i": 1 });
        let p2 = serde_json::json!({ "i": 2 });
        let (a, b) = tokio::join!(
            s1.call::<_, serde_json::Value>("arch.x", &p1),
            s2.call::<_, serde_json::Value>("arch.y", &p2),
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert_eq!(a["echoed"]["i"], 1);
        assert_eq!(b["echoed"]["i"], 2);
    }

    #[tokio::test]
    async fn pending_calls_resolve_with_transport_closed_on_eof() {
        let (client_io, server_io) = duplex(8 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);
        // Server task: read but never reply, drop both ends after a beat.
        tokio::spawn(async move {
            drop(server_write);
            let mut lines = BufReader::new(server_read).lines();
            // Read once so the client's frame leaves the buffer, then EOF.
            let _ = lines.next_line().await;
        });
        let session = JsonRpcSession::spawn(client_read, client_write);

        let result: Result<serde_json::Value, _> =
            session.call("arch.never", &serde_json::json!({})).await;
        match result {
            Err(RpcError::TransportClosed) => {}
            other => panic!("expected TransportClosed, got {:?}", other),
        }
        assert!(!session.is_open());
    }

    #[tokio::test]
    async fn daemon_error_frame_surfaces_as_rpc_error() {
        let (client_io, server_io) = duplex(8 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);
        // Custom server: emits an error frame instead of a result.
        tokio::spawn(async move {
            let mut lines = BufReader::new(server_read).lines();
            let mut writer = server_write;
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": "Method not found: arch.bogus" }
                });
                let mut bytes = serde_json::to_vec(&resp).unwrap();
                bytes.push(b'\n');
                let _ = writer.write_all(&bytes).await;
                let _ = writer.flush().await;
            }
        });
        let session = JsonRpcSession::spawn(client_read, client_write);

        let result: Result<serde_json::Value, _> =
            session.call("arch.bogus", &serde_json::json!({})).await;
        match result {
            Err(RpcError::Daemon { code, message }) => {
                assert_eq!(code, -32601);
                assert!(message.contains("arch.bogus"));
            }
            other => panic!("expected Daemon error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn arch_event_notifications_fan_out_to_subscribers() {
        let (client_io, server_io) = duplex(8 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        // Server emits one notification then idles.
        tokio::spawn(async move {
            let _server_read = server_read;
            let mut writer = server_write;
            let frame = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "arch:event",
                "params": {
                    "event": "index_started",
                    "reason": "boot",
                    "scope": { "scope": "all" }
                }
            });
            let mut bytes = serde_json::to_vec(&frame).unwrap();
            bytes.push(b'\n');
            let _ = writer.write_all(&bytes).await;
            let _ = writer.flush().await;
            // Hold the connection open so the client's reader keeps running.
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let session = JsonRpcSession::spawn(client_read, client_write);
        let mut events = session.subscribe_events();

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("event arrives in time")
            .expect("not lagged");
        match event {
            ArchEvent::IndexStarted { .. } => {}
            other => panic!("expected IndexStarted, got {:?}", other),
        }
    }
}
