//! @arch:layer(cli)
//! @arch:role(bridge)
//! @arch:thread(async_io)
//!
//! `yah serve --stdio` — JSON-RPC daemon over stdin/stdout.
//!
//! One [`KgService`] per process, line-delimited JSON-RPC 2.0 frames on
//! stdin/stdout. Designed to be launched on a remote box by
//! `SshRpcClient` (R019-F2) so the renderer can talk to remote rigs over
//! the same shapes Tauri commands speak today (`rpc::*`).
//!
//! Wire shape:
//! - Request:  `{"jsonrpc":"2.0","id":1,"method":"arch.stats","params":{...}}`
//! - Response: `{"jsonrpc":"2.0","id":1,"result":{...}}` or
//!             `{"jsonrpc":"2.0","id":1,"error":{"code":...,"message":"..."}}`
//! - Notification: `{"jsonrpc":"2.0","method":"arch:event","params":<ArchEvent>}`
//!   (no `id`; matches the Tauri renderer's `arch:event` payload, minus
//!   the `rig_id` wrapper — the SSH-side client stamps that locally so
//!   the daemon stays single-rig-aware).
//!
//! `rigId` / `rig_id` in incoming params is stripped before deserializing
//! into the typed param struct: a `yah serve --stdio` instance serves
//! exactly one rig (the path the client opens with `arch.open_rig`), so
//! per-request rig dispatch isn't this layer's job.
//!
//! Logs go to stderr — stdout is reserved for JSON frames.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use kg::event::{ArchEvent, FileEvent, IndexReason};
use kg::ids::NodeId;
use rpc::{
    method, ArchiveTicketParams, DirListParams, DirWatchParams, FileReadParams, FileWatchParams,
    FileWriteParams, GetTicketParams, ListRelaysParams, ListTicketsParams, LookupParams,
    MoveTicketParams, NeighborsParams, RootsParams, SubgraphParams, TicketPromptParams,
    UnwatchParams, ValidateParams,
};
use kg_daemon::KgService;
use kg_rust::RustIndexer;
use kg_store::IndexerRegistry;
use kg_ts::TsIndexer;

/// Entry point invoked from `Commands::Serve { transport: "stdio", .. }`.
/// Builds the runtime, spins the request loop, and blocks until stdin
/// closes (typical termination signal from `SshRpcClient` dropping the
/// channel).
pub fn run_stdio(rig: Option<PathBuf>) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .context("failed to start tokio runtime for `yah serve --stdio`")?;
    rt.block_on(async move {
        // Logs to stderr keep stdout clean for JSON-RPC frames.
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(std::io::stderr)
            .with_target(false)
            .try_init()
            .ok();

        let svc = Arc::new(KgService::new(default_registry()));
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

        // Notification fan-out: subscribe to the daemon's broadcast and
        // emit each `ArchEvent` as a JSON-RPC notification. Wired up
        // *before* pre-boot so IndexStarted/Finished frames during the
        // initial walk make it to the client. Auto-exits when every
        // publisher drops (i.e. svc is gone after we return).
        let stdout_for_events = Arc::clone(&stdout);
        let mut event_rx = svc.subscribe();
        let event_task = tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match event_rx.recv().await {
                    Ok(event) => emit_event(&stdout_for_events, &event).await,
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "stdio client lagged on arch:event");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });

        // Same pattern for the file-watch broadcast → `file.event`
        // notifications. Separate channel so a chatty filesystem
        // doesn't drown the structural ArchEvent stream.
        let stdout_for_file_events = Arc::clone(&stdout);
        let mut file_event_rx = svc.subscribe_file_events();
        let file_event_task = tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match file_event_rx.recv().await {
                    Ok(event) => emit_file_event(&stdout_for_file_events, &event).await,
                    Err(RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "stdio client lagged on file.event");
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });

        let preboot = rig.or_else(|| std::env::var("YAH_RIG_ROOT").ok().map(PathBuf::from));
        if let Some(path) = preboot {
            tracing::info!(path = %path.display(), "pre-booting rig from --rig/YAH_RIG_ROOT");
            match svc.boot(path).await {
                Ok(_) => {
                    if let Err(e) = svc.start_watching().await {
                        tracing::warn!(error = %e, "watcher start failed");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "pre-boot failed; daemon stays cold"),
            }
        }

        // Request loop: read one JSON-RPC frame per line, spawn its
        // dispatch concurrently so a slow query doesn't block stats /
        // notifications. JoinSet drains in-flight tasks at shutdown.
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();
        let mut tasks: JoinSet<()> = JoinSet::new();
        loop {
            tokio::select! {
                line_res = lines.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            if line.trim().is_empty() {
                                continue;
                            }
                            let svc = Arc::clone(&svc);
                            let stdout = Arc::clone(&stdout);
                            tasks.spawn(async move {
                                if let Some(frame) = handle_line(&line, &svc).await {
                                    write_frame(&stdout, &frame).await;
                                }
                            });
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(error = %e, "stdin read error; closing");
                            break;
                        }
                    }
                }
                Some(_) = tasks.join_next() => {}
            }
        }

        // Drain in-flight requests so their responses make it to stdout.
        while tasks.join_next().await.is_some() {}

        // Stop the watcher (if any) before the service drops, so the
        // `notify` thread shuts down cleanly.
        svc.stop_watching().await;

        // Drop the service handle held by the event task by dropping its
        // subscriber implicitly: when `svc` (Arc) drops at end-of-scope
        // there are no more publishers and the broadcast closes.
        drop(svc);
        let _ = event_task.await;
        let _ = file_event_task.await;

        Ok::<(), anyhow::Error>(())
    })
}

fn default_registry() -> IndexerRegistry {
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(RustIndexer::new()));
    registry.register(Box::new(TsIndexer::new()));
    // JsonIndexer/YamlIndexer/TomlIndexer (yah-kg-json-yaml) are the
    // missing pair the Tauri host wires up — add them here once that
    // crate's mid-edit WIP merges (see yah/Cargo.toml note).
    registry
}

async fn emit_event(stdout: &Arc<Mutex<tokio::io::Stdout>>, event: &ArchEvent) {
    let frame = json!({
        "jsonrpc": "2.0",
        "method": "arch:event",
        "params": event,
    });
    write_frame(stdout, &frame).await;
}

async fn emit_file_event(stdout: &Arc<Mutex<tokio::io::Stdout>>, event: &FileEvent) {
    let frame = json!({
        "jsonrpc": "2.0",
        "method": "file.event",
        "params": event,
    });
    write_frame(stdout, &frame).await;
}

async fn write_frame(stdout: &Arc<Mutex<tokio::io::Stdout>>, frame: &Value) {
    let Ok(mut bytes) = serde_json::to_vec(frame) else {
        tracing::warn!("failed to serialize JSON-RPC frame");
        return;
    };
    bytes.push(b'\n');
    let mut out = stdout.lock().await;
    let _ = out.write_all(&bytes).await;
    let _ = out.flush().await;
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

async fn handle_line(line: &str, svc: &Arc<KgService>) -> Option<Value> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(error_frame(
                Value::Null,
                -32700,
                format!("Parse error: {}", e),
            ));
        }
    };
    let id = req.id;
    let is_notification = id.is_none();
    match dispatch(&req.method, req.params, svc).await {
        Ok(value) => {
            if is_notification {
                None
            } else {
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": value,
                }))
            }
        }
        Err(e) => {
            if is_notification {
                tracing::warn!(error = %e.message, method = %req.method, "notification failed");
                None
            } else {
                Some(error_frame(id.unwrap_or(Value::Null), e.code, e.message))
            }
        }
    }
}

fn error_frame(id: Value, code: i32, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

struct RpcError {
    code: i32,
    message: String,
}

impl RpcError {
    fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: format!("Invalid params: {}", msg.into()),
        }
    }
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {}", method),
        }
    }
    fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: format!("Internal error: {}", msg.into()),
        }
    }
}

async fn dispatch(method: &str, mut params: Value, svc: &Arc<KgService>) -> Result<Value, RpcError> {
    // Strip rig identifiers — this daemon serves a single rig and the
    // client (SshRpcClient) is responsible for its own routing layer.
    if let Some(obj) = params.as_object_mut() {
        obj.remove("rigId");
        obj.remove("rig_id");
    }

    match method {
        method::ROOTS => to_value(svc.roots(de_or_default::<RootsParams>(params)?).await),
        method::SUBGRAPH => to_value(svc.subgraph(de::<SubgraphParams>(params)?).await),
        method::LOOKUP => to_value(svc.lookup(de::<LookupParams>(params)?).await),
        method::NODE => {
            #[derive(Deserialize)]
            struct P { id: NodeId }
            let P { id } = de(params)?;
            to_value(svc.node(id).await)
        }
        method::NEIGHBORS => to_value(svc.neighbors(de::<NeighborsParams>(params)?).await),
        method::LANGUAGES => Ok(json!({ "langs": svc.languages() })),
        method::STATS => to_value(svc.stats().await),
        method::LIST_TICKETS => {
            to_value(svc.list_tickets(de_or_default::<ListTicketsParams>(params)?).await)
        }
        method::LIST_RELAYS => {
            to_value(svc.list_relays(de_or_default::<ListRelaysParams>(params)?).await)
        }
        method::GET_TICKET => to_value(svc.get_ticket(de::<GetTicketParams>(params)?).await),
        method::VALIDATE => {
            to_value(svc.validate(de_or_default::<ValidateParams>(params)?).await)
        }
        method::TICKET_PROMPT => {
            to_value(svc.ticket_prompt(de::<TicketPromptParams>(params)?).await)
        }
        method::MOVE_TICKET => {
            let p = de::<MoveTicketParams>(params)?;
            let r = svc
                .move_ticket(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::ARCHIVE_TICKET => {
            let p = de::<ArchiveTicketParams>(params)?;
            let r = svc
                .archive_ticket(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::FILE_READ => {
            let p = de::<FileReadParams>(params)?;
            let r = svc
                .file_read(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::FILE_WRITE => {
            let p = de::<FileWriteParams>(params)?;
            let r = svc
                .file_write(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::DIR_LIST => {
            let p = de::<DirListParams>(params)?;
            let r = svc
                .dir_list(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::FILE_WATCH => {
            let p = de::<FileWatchParams>(params)?;
            let r = svc
                .watch_file(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::DIR_WATCH => {
            let p = de::<DirWatchParams>(params)?;
            let r = svc
                .watch_dir(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }
        method::FILE_UNWATCH => {
            let p = de::<UnwatchParams>(params)?;
            let r = svc
                .unwatch(p)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            to_value(r)
        }

        // ----- Tauri-only surface, lifted onto the wire so SshRpcClient
        // can drive a remote daemon end-to-end. Names mirror
        // app/tauri/src/commands.rs so the client side speaks one
        // vocabulary across local/remote (see .yah/arch/authored/rig-backend-dispatch.md).

        "arch.open_rig" => {
            #[derive(Deserialize)]
            struct P { path: String }
            let P { path } = de(params)?;
            let summary = svc
                .boot(PathBuf::from(&path))
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            if let Err(e) = svc.start_watching().await {
                tracing::warn!(error = %e, "watcher start failed");
            }
            Ok(json!({
                "filesSeen": summary.files_seen,
                "filesIndexed": summary.files_indexed,
                "filesSkipped": summary.files_skipped,
                "parseErrors": summary.parse_errors,
            }))
        }
        "arch.close_rig" => {
            svc.stop_watching().await;
            Ok(Value::Null)
        }
        "arch.reindex_path" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct P {
                path: String,
                #[serde(default)]
                reason: Option<String>,
            }
            let P { path, reason } = de(params)?;
            let reason = match reason.as_deref() {
                Some("file_watch") => IndexReason::FileWatch,
                Some("manual") => IndexReason::Manual,
                Some("agent_edit") => IndexReason::AgentEdit,
                _ => IndexReason::Boot,
            };
            svc.reindex_path(Path::new(&path), reason)
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
            Ok(Value::Null)
        }
        "arch.touch" => {
            #[derive(Deserialize)]
            struct P {
                paths: Vec<String>,
                tool: String,
                relay: String,
            }
            let P { paths, tool, relay } = de(params)?;
            svc.touch(&paths, &tool, &relay).await;
            Ok(Value::Null)
        }

        other => Err(RpcError::method_not_found(other)),
    }
}

fn de<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    serde_json::from_value(v).map_err(|e| RpcError::invalid_params(e.to_string()))
}

/// Like [`de`] but treats `null`/missing params as the type's `Default`
/// — for methods like `arch.list_tickets` whose params struct is empty.
fn de_or_default<T: serde::de::DeserializeOwned + Default>(v: Value) -> Result<T, RpcError> {
    if v.is_null() {
        return Ok(T::default());
    }
    if let Some(obj) = v.as_object() {
        if obj.is_empty() {
            return Ok(T::default());
        }
    }
    de(v)
}

fn to_value<T: Serialize>(v: T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| RpcError::internal(e.to_string()))
}
