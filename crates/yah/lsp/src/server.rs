//! @arch:layer(lsp)
//! @arch:role(transport)
//! @arch:thread(async_io)
//!
//! [`LanguageServer`] — one running language-server child process plus
//! the JSON-RPC bookkeeping to talk to it.
//!
//! Shape mirrors `rpc_ssh::session::JsonRpcSession`: a single reader
//! task drains framed responses, a `Mutex<HashMap<id, oneshot>>` routes
//! each result back to its waiter, notifications fan out via a
//! `broadcast` channel. The structural difference is the framing —
//! LSP uses `Content-Length` headers, not line-delimited JSON — and the
//! direction notifications travel: the *server* publishes (diagnostics,
//! progress) and the multiplex layer above us forwards them out.
//!
//! `LanguageServer` does **not** know about rigs. The pool
//! ([`crate::pool::LspPool`]) keys on `(rig_root, ServerKind)` and
//! constructs one of these per cell.

use crate::framing::{read_message, write_message, FramingError};
use crate::language::ServerCommand;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time::timeout;

/// Default broadcast capacity for server→client notifications
/// (`textDocument/publishDiagnostics`, `window/showMessage`, …). Sized
/// so a brief multiplex hiccup doesn't drop frames.
const NOTIFICATION_CHANNEL_CAPACITY: usize = 256;

/// Cap on how long [`LanguageServer::shutdown`] waits for the LSP
/// `shutdown` reply before forcing the child down with SIGKILL.
const SHUTDOWN_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum LspError {
    #[error("language server transport closed")]
    TransportClosed,
    #[error("language server returned error {code}: {message}")]
    Server { code: i32, message: String },
    #[error("encode error: {0}")]
    Encode(serde_json::Error),
    #[error("decode error: {0}")]
    Decode(serde_json::Error),
    #[error("framing error: {0}")]
    Framing(#[from] FramingError),
    #[error("io error: {0}")]
    Io(String),
    #[error("failed to spawn `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// Server-pushed notification frame (no `id`). Surfaced verbatim so the
/// multiplex layer can wrap it as an `lsp.notification` to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerNotification {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Subscribe to server-pushed notifications. Thin wrapper around a
/// `broadcast::Receiver` so callers don't depend on tokio's re-exports.
pub struct NotificationStream {
    rx: broadcast::Receiver<ServerNotification>,
}

impl NotificationStream {
    pub async fn recv(
        &mut self,
    ) -> Result<ServerNotification, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

type Pending = oneshot::Sender<Result<Value, LspError>>;

struct Inner {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    /// Writes the LSP-framed bytes to the child's stdin. Wrapped in a
    /// mutex so concurrent senders can't interleave bytes mid-frame.
    writer: Mutex<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>,
    open: AtomicBool,
    notifications: broadcast::Sender<ServerNotification>,
    /// Held to keep the child alive for the lifetime of the session;
    /// `Drop` on `Child` SIGKILLs (we set `kill_on_drop` on spawn).
    child: Mutex<Option<Child>>,
    /// Echoed back from the spawn site for diagnostics.
    program: String,
}

/// One running language-server child plus its JSON-RPC bookkeeping.
///
/// Cheap to clone (`Arc` of internals). Drop the last clone (or call
/// [`LanguageServer::shutdown`]) to terminate the child.
#[derive(Clone)]
pub struct LanguageServer {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for LanguageServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageServer")
            .field("program", &self.inner.program)
            .field("open", &self.is_open())
            .finish()
    }
}

impl LanguageServer {
    /// Spawn a language-server child according to `command` and wire its
    /// stdio into a JSON-RPC session. Sets `cwd` to `workspace_root` so
    /// the server picks up the right project on `initialize`.
    pub async fn spawn(
        command: &ServerCommand,
        workspace_root: &std::path::Path,
    ) -> Result<Self, LspError> {
        let mut cmd = Command::new(&command.program);
        cmd.args(&command.args);
        cmd.current_dir(workspace_root);
        for (k, v) in &command.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| LspError::Spawn {
            program: command.program.clone(),
            source: e,
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Io("child missing stdin handle".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Io("child missing stdout handle".into()))?;
        // stderr stays piped on the child — the pool drains it through
        // a separate task so the child doesn't block on a full stderr
        // pipe. We hand the stderr handle back via the child for that.
        let stderr = child.stderr.take();

        let (tx, _rx) = broadcast::channel(NOTIFICATION_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            writer: Mutex::new(Box::new(stdin)),
            open: AtomicBool::new(true),
            notifications: tx,
            child: Mutex::new(Some(child)),
            program: command.program.clone(),
        });
        spawn_reader(Arc::clone(&inner), stdout);
        if let Some(stderr) = stderr {
            spawn_stderr_drain(stderr, command.program.clone());
        }
        Ok(Self { inner })
    }

    /// Echo back the program name we spawned (for log lines, error
    /// messages — pool uses this when reporting "spawn failed").
    pub fn program(&self) -> &str {
        &self.inner.program
    }

    pub fn is_open(&self) -> bool {
        self.inner.open.load(Ordering::Acquire)
    }

    /// Subscribe to server-pushed notifications. Each subscriber gets
    /// its own buffered receiver — late joiners only see frames after
    /// the subscribe call.
    pub fn subscribe_notifications(&self) -> NotificationStream {
        NotificationStream {
            rx: self.inner.notifications.subscribe(),
        }
    }

    /// Issue an LSP request and await the typed response.
    pub async fn request<P, R>(&self, method: &str, params: &P) -> Result<R, LspError>
    where
        P: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let value = self.request_raw(method, params).await?;
        serde_json::from_value(value).map_err(LspError::Decode)
    }

    /// Issue an LSP request, return the raw JSON result. Useful when
    /// the multiplex layer is forwarding a body verbatim (R033-T12).
    pub async fn request_raw<P>(&self, method: &str, params: &P) -> Result<Value, LspError>
    where
        P: Serialize + ?Sized,
    {
        if !self.is_open() {
            return Err(LspError::TransportClosed);
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let params_value = serde_json::to_value(params).map_err(LspError::Encode)?;
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params_value,
        });
        let bytes = serde_json::to_vec(&frame).map_err(LspError::Encode)?;

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.inner.pending.lock().await;
            pending.insert(id, tx);
        }
        if let Err(e) = self.write_frame(&bytes).await {
            // Take the slot back if the write failed before reader can
            // route a response into it.
            let mut pending = self.inner.pending.lock().await;
            pending.remove(&id);
            return Err(e);
        }

        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(LspError::TransportClosed),
        }
    }

    /// Fire-and-forget LSP notification (no `id`).
    pub async fn notify<P>(&self, method: &str, params: &P) -> Result<(), LspError>
    where
        P: Serialize + ?Sized,
    {
        if !self.is_open() {
            return Err(LspError::TransportClosed);
        }
        let params_value = serde_json::to_value(params).map_err(LspError::Encode)?;
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params_value,
        });
        let bytes = serde_json::to_vec(&frame).map_err(LspError::Encode)?;
        self.write_frame(&bytes).await
    }

    /// Forward a pre-serialized request body verbatim. Used by the
    /// multiplex layer when the client → server JSON envelope already
    /// arrived intact and we just need to remap the `id`.
    ///
    /// The `id` in `body` is rewritten to a freshly-allocated server-side
    /// id so concurrent multiplex callers don't collide; the original
    /// id is preserved in the returned [`ForwardedResponse`] so the
    /// caller can rewrite it back on the way out.
    pub async fn forward_request(
        &self,
        body: Value,
    ) -> Result<ForwardedResponse, LspError> {
        let original_id = body.get("id").cloned().unwrap_or(Value::Null);
        let method = body
            .get("method")
            .and_then(|m| m.as_str())
            .ok_or_else(|| LspError::Io("forwarded body missing `method`".into()))?
            .to_string();
        let params = body.get("params").cloned().unwrap_or(Value::Null);
        let result = self.request_raw(&method, &params).await?;
        Ok(ForwardedResponse {
            original_id,
            result,
        })
    }

    /// LSP shutdown handshake: send `shutdown` (request, expects ack),
    /// then `exit` (notification, no ack), then drop the child. Bounded
    /// timeout — if the server is wedged we kill it.
    pub async fn shutdown(&self) -> Result<(), LspError> {
        if !self.is_open() {
            return Ok(());
        }
        let _ = timeout(
            SHUTDOWN_REQUEST_TIMEOUT,
            self.request_raw("shutdown", &Value::Null),
        )
        .await;
        // `exit` is a notification — best-effort.
        let _ = self.notify("exit", &Value::Null).await;
        self.inner.open.store(false, Ordering::Release);
        // Drain pending so any racing callers resolve fast.
        let drained: HashMap<u64, Pending> = {
            let mut guard = self.inner.pending.lock().await;
            std::mem::take(&mut *guard)
        };
        for (_id, tx) in drained {
            let _ = tx.send(Err(LspError::TransportClosed));
        }
        let mut guard = self.inner.child.lock().await;
        if let Some(mut c) = guard.take() {
            // Best-effort wait so we don't leak a zombie; if the child
            // is wedged, kill_on_drop handles it.
            let _ = timeout(Duration::from_millis(500), c.wait()).await;
            let _ = c.start_kill();
        }
        Ok(())
    }

    async fn write_frame(&self, bytes: &[u8]) -> Result<(), LspError> {
        let mut w = self.inner.writer.lock().await;
        write_message(&mut *w, bytes)
            .await
            .map_err(LspError::Framing)?;
        use tokio::io::AsyncWriteExt;
        w.flush().await.map_err(|e| LspError::Io(e.to_string()))?;
        Ok(())
    }
}

/// Result of [`LanguageServer::forward_request`]: the verbatim JSON
/// `result` plus the caller's original `id` so the multiplex layer can
/// rebuild the response envelope before sending it back to the renderer.
#[derive(Debug, Clone)]
pub struct ForwardedResponse {
    pub original_id: Value,
    pub result: Value,
}

fn spawn_reader<R>(inner: Arc<Inner>, read: R)
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    tokio::spawn(async move {
        let mut read = read;
        loop {
            match read_message(&mut read).await {
                Ok(Some(body)) => handle_frame(&inner, &body).await,
                Ok(None) => break, // clean EOF
                Err(e) => {
                    tracing::warn!(error = %e, program = %inner.program, "lsp framing error; closing");
                    break;
                }
            }
        }
        inner.open.store(false, Ordering::Release);
        let drained: HashMap<u64, Pending> = {
            let mut guard = inner.pending.lock().await;
            std::mem::take(&mut *guard)
        };
        for (_id, tx) in drained {
            let _ = tx.send(Err(LspError::TransportClosed));
        }
    });
}

fn spawn_stderr_drain<R>(read: R, program: String)
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    use tokio::io::{AsyncBufReadExt, BufReader};
    tokio::spawn(async move {
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "lsp::stderr", program = %program, "{}", line);
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

async fn handle_frame(inner: &Arc<Inner>, body: &[u8]) {
    let frame: Frame = match serde_json::from_slice(body) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "malformed lsp frame; dropping");
            return;
        }
    };

    // Notification: no id, has method.
    if frame.id.is_none() {
        if let Some(method) = frame.method {
            let note = ServerNotification {
                method,
                params: frame.params,
            };
            let _ = inner.notifications.send(note);
        }
        return;
    }

    // Some servers send server→client *requests* (window/workDoneProgress/create,
    // workspace/configuration). v1 doesn't honour those — ignore the id
    // so the server doesn't expect a reply, but record a debug log so
    // we can quantify how often it happens. The multiplex layer can
    // grow proper bidirectional support in a follow-up.
    if frame.method.is_some() && frame.id.is_some() && frame.result.is_none() && frame.error.is_none() {
        let method = frame.method.as_deref().unwrap_or("");
        tracing::debug!(method, "ignoring server→client request (v1 limitation)");
        return;
    }

    let id = match frame.id.as_ref().and_then(extract_u64_id) {
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
        tracing::debug!(id, "lsp response for unknown id");
        return;
    };
    if let Some(err) = frame.error {
        let _ = tx.send(Err(LspError::Server {
            code: err.code,
            message: err.message,
        }));
    } else {
        let _ = tx.send(Ok(frame.result.unwrap_or(Value::Null)));
    }
}

fn extract_u64_id(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
}

/// Workspace lifecycle bookkeeping. The pool calls these so multiple
/// rigs sharing one server kind (not the v1 case, but the shape is
/// here) don't have to re-derive `initializeParams`.
pub fn build_initialize_params(
    workspace_root: &std::path::Path,
    process_id: Option<u32>,
) -> Value {
    let uri = path_to_file_uri(workspace_root);
    serde_json::json!({
        "processId": process_id,
        "clientInfo": { "name": "yah", "version": env!("CARGO_PKG_VERSION") },
        "rootUri": uri,
        "workspaceFolders": [{
            "uri": uri,
            "name": workspace_root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "workspace".to_string()),
        }],
        "capabilities": {
            "textDocument": {
                "synchronization": { "didSave": true, "willSave": false, "willSaveWaitUntil": false },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "completion": {
                    "completionItem": { "snippetSupport": true, "documentationFormat": ["markdown", "plaintext"] }
                },
                "publishDiagnostics": {}
            },
            "workspace": {
                "workspaceFolders": true,
                "configuration": false
            }
        }
    })
}

fn path_to_file_uri(p: &std::path::Path) -> String {
    // Sufficient for v1: percent-encode spaces only. The renderer-side
    // `vscode-languageclient` does fuller encoding when sending its own
    // URIs, and we never round-trip the rootUri through anything else.
    let canonical = std::fs::canonicalize(p)
        .unwrap_or_else(|_| PathBuf::from(p));
    let mut s = canonical.to_string_lossy().to_string();
    if cfg!(windows) {
        s = s.replace('\\', "/");
        format!("file:///{}", s.replace(' ', "%20"))
    } else {
        format!("file://{}", s.replace(' ', "%20"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::write_message;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    /// Tests bypass the real `Command::spawn` and wire the LanguageServer
    /// internals against an in-memory duplex driving a fake LSP server
    /// task. Lets us assert the framing + multiplex without depending on
    /// rust-analyzer being installed.
    fn server_from_pipes(
        write: tokio::io::DuplexStream,
        read: tokio::io::DuplexStream,
    ) -> LanguageServer {
        let (tx, _rx) = broadcast::channel(NOTIFICATION_CHANNEL_CAPACITY);
        let inner = Arc::new(Inner {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            writer: Mutex::new(Box::new(write)),
            open: AtomicBool::new(true),
            notifications: tx,
            child: Mutex::new(None),
            program: "fake-lsp".to_string(),
        });
        spawn_reader(Arc::clone(&inner), read);
        LanguageServer { inner }
    }

    #[tokio::test]
    async fn request_round_trip() {
        // Two duplexes: client→server and server→client. The server task
        // reads requests from one, replies on the other.
        let (cw, sr) = duplex(64 * 1024);
        let (sw, cr) = duplex(64 * 1024);
        // Server task: read one frame, parrot back result.
        tokio::spawn(async move {
            let mut sr = sr;
            let mut sw = sw;
            let body = read_message(&mut sr).await.unwrap().unwrap();
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "result": { "ok": true, "method": req["method"] }
            });
            let bytes = serde_json::to_vec(&resp).unwrap();
            write_message(&mut sw, &bytes).await.unwrap();
            sw.flush().await.unwrap();
        });
        let ls = server_from_pipes(cw, cr);
        let result: serde_json::Value = ls
            .request("textDocument/hover", &serde_json::json!({"line": 1}))
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["method"], "textDocument/hover");
    }

    #[tokio::test]
    async fn notification_fans_out_to_subscribers() {
        let (cw, sr) = duplex(64 * 1024);
        let (mut sw, cr) = duplex(64 * 1024);
        // Push one diagnostics notification then idle.
        tokio::spawn(async move {
            let _sr = sr;
            let frame = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": "file:///x.rs", "diagnostics": [] }
            });
            let bytes = serde_json::to_vec(&frame).unwrap();
            write_message(&mut sw, &bytes).await.unwrap();
            sw.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
        let ls = server_from_pipes(cw, cr);
        let mut sub = ls.subscribe_notifications();
        let note = timeout(Duration::from_secs(1), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(note.method, "textDocument/publishDiagnostics");
    }

    #[tokio::test]
    async fn pending_calls_resolve_with_transport_closed_on_eof() {
        let (cw, _sr) = duplex(64 * 1024);
        // server-side write half dropped immediately so client read EOF
        let (sw, cr) = duplex(64 * 1024);
        drop(sw);
        let ls = server_from_pipes(cw, cr);
        let result: Result<serde_json::Value, _> =
            ls.request("anything", &serde_json::json!({})).await;
        match result {
            Err(LspError::TransportClosed) => {}
            other => panic!("expected TransportClosed, got {:?}", other),
        }
        assert!(!ls.is_open());
    }

    #[tokio::test]
    async fn server_error_frame_surfaces() {
        let (cw, sr) = duplex(64 * 1024);
        let (sw, cr) = duplex(64 * 1024);
        tokio::spawn(async move {
            let mut sr = sr;
            let mut sw = sw;
            let body = read_message(&mut sr).await.unwrap().unwrap();
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "error": { "code": -32601, "message": "Method not found" }
            });
            let bytes = serde_json::to_vec(&resp).unwrap();
            write_message(&mut sw, &bytes).await.unwrap();
            sw.flush().await.unwrap();
        });
        let ls = server_from_pipes(cw, cr);
        let err = ls
            .request::<_, serde_json::Value>("bogus", &serde_json::json!({}))
            .await
            .unwrap_err();
        match err {
            LspError::Server { code, message } => {
                assert_eq!(code, -32601);
                assert!(message.contains("Method not found"));
            }
            other => panic!("expected Server error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn forward_request_preserves_original_id() {
        let (cw, sr) = duplex(64 * 1024);
        let (sw, cr) = duplex(64 * 1024);
        tokio::spawn(async move {
            let mut sr = sr;
            let mut sw = sw;
            let body = read_message(&mut sr).await.unwrap().unwrap();
            let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": req["id"],
                "result": { "echoed": req["params"] }
            });
            let bytes = serde_json::to_vec(&resp).unwrap();
            write_message(&mut sw, &bytes).await.unwrap();
            sw.flush().await.unwrap();
        });
        let ls = server_from_pipes(cw, cr);
        // Caller's id is "client-77" — a string, not numeric. The pool
        // remaps to a server-side numeric id but we round-trip the
        // caller's id back via ForwardedResponse.
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "client-77",
            "method": "textDocument/definition",
            "params": { "x": 1 }
        });
        let resp = ls.forward_request(body).await.unwrap();
        assert_eq!(resp.original_id, serde_json::json!("client-77"));
        assert_eq!(resp.result["echoed"]["x"], 1);
    }

    #[tokio::test]
    async fn build_initialize_params_includes_root_uri() {
        let tmp = tempfile::tempdir().unwrap();
        let params = build_initialize_params(tmp.path(), Some(99));
        let root_uri = params["rootUri"].as_str().unwrap().to_string();
        assert!(root_uri.starts_with("file://"));
        assert_eq!(params["processId"], 99);
        assert!(params["capabilities"]["textDocument"]["hover"].is_object());
    }

    #[tokio::test]
    async fn spawn_propagates_io_error_for_missing_program() {
        let cmd = ServerCommand {
            program: "definitely-not-a-real-program-name-xyz".to_string(),
            args: vec![],
            env: vec![],
        };
        let tmp = tempfile::tempdir().unwrap();
        let err = LanguageServer::spawn(&cmd, tmp.path()).await.unwrap_err();
        match err {
            LspError::Spawn { program, .. } => {
                assert!(program.contains("definitely-not-a-real"));
            }
            other => panic!("expected Spawn error, got {:?}", other),
        }
    }

    /// Shutdown best-effort: when there's no real child, still flips
    /// `open` to false and drains pending. We don't have a real child
    /// in the pipe-based fixture, but the public effect is the same.
    #[tokio::test]
    async fn shutdown_marks_closed_and_drains_pending() {
        // A server that ignores the inbound shutdown — we want to
        // exercise the timeout path AND the pending-drain side effect.
        let (cw, sr) = duplex(64 * 1024);
        let (sw, cr) = duplex(64 * 1024);
        let _ = sw; // hold open so reader doesn't EOF early
        tokio::spawn(async move {
            // Drain inbound forever, never reply. Holds sw via ownership.
            let mut sr = sr;
            let mut buf = [0u8; 1024];
            while let Ok(n) = sr.read(&mut buf).await {
                if n == 0 {
                    break;
                }
            }
        });
        let ls = server_from_pipes(cw, cr);
        // Make sure shutdown's bounded timeout returns rather than hanging.
        timeout(Duration::from_secs(5), ls.shutdown())
            .await
            .expect("shutdown returned in bounded time")
            .unwrap();
        assert!(!ls.is_open());
    }
}
