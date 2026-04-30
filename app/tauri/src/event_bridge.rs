//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Forwards each rig's `ArchEvent` stream to Tauri's window event bus,
//! stamped with the originating [`RigId`]. Source of the stream depends
//! on the rig backend:
//!
//! - **Local rigs**: subscribe to the in-process `KgService` broadcast.
//! - **Remote rigs**: subscribe to the `SshRpcClient` event stream, which
//!   forwards `arch:event` JSON-RPC notifications from the remote daemon.
//!   When the underlying SSH session drops + reconnects, the prior
//!   subscription goes inert; the bridge polls for fresh subscriptions
//!   on a short interval so events resume after reconnect without the
//!   renderer needing to retry.
//!
//! Frontend listener:
//! ```ts
//! import { listen } from '@tauri-apps/api/event';
//! const unlisten = await listen<RigEvent>('arch:event',
//!   (e) => routeByRig(e.payload.rig_id, e.payload.event));
//! ```
//!
//! Each attach spawns one bridge task. Detaching the rig aborts the
//! task via the `JoinHandle` held in `RigEntry`.

use crate::backend::RigBackend;
use crate::state::RigId;
use serde::Serialize;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::error::RecvError;
use yah_kg::event::{ArchEvent, FileEvent};

const EVENT_NAME: &str = "arch:event";
const FILE_EVENT_NAME: &str = "file:event";

/// Wire payload: rig origin + the daemon's own event.
#[derive(Debug, Clone, Serialize)]
pub struct RigEvent<'a> {
    pub rig_id: &'a RigId,
    #[serde(flatten)]
    pub event: &'a ArchEvent,
}

/// Wire payload for the file-watch stream — same rig-stamp pattern as
/// [`RigEvent`] but for the `file.event` channel.
#[derive(Debug, Clone, Serialize)]
pub struct RigFileEvent<'a> {
    pub rig_id: &'a RigId,
    #[serde(flatten)]
    pub event: &'a FileEvent,
}

pub fn spawn_for(rig_id: RigId, backend: RigBackend, handle: AppHandle) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        match backend {
            RigBackend::Local(svc) => {
                // Parallel forward of the file-watch broadcast; cheap to run
                // unconditionally because the channel is silent until the
                // renderer arms a watch via file.watch / dir.watch.
                let svc_for_files = std::sync::Arc::clone(&svc);
                let rig_id_for_files = rig_id.clone();
                let handle_for_files = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = svc_for_files.subscribe_file_events();
                    loop {
                        match rx.recv().await {
                            Ok(event) => emit_file(&handle_for_files, &rig_id_for_files, &event),
                            Err(RecvError::Lagged(n)) => {
                                tracing::warn!(skipped = n, rig = %rig_id_for_files.as_str(), "frontend lagged on file.event");
                            }
                            Err(RecvError::Closed) => break,
                        }
                    }
                });

                let mut rx = svc.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(event) => emit(&handle, &rig_id, &event),
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, rig = %rig_id.as_str(), "frontend lagged on arch:event");
                        }
                        Err(RecvError::Closed) => break,
                    }
                }
            }
            RigBackend::Remote(client) => {
                // SshRpcClient::subscribe_events binds to the live session.
                // After a transport drop the session reopens and the prior
                // ArchEventStream goes inert; resubscribe on the next iter.
                // Initial subscribe also opens the session if it isn't yet,
                // matching the renderer's "lazy on activation" model.
                'outer: loop {
                    let mut stream = match client.subscribe_events().await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, rig = %rig_id.as_str(), "remote event subscribe failed; retrying");
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                    };
                    loop {
                        match stream.recv().await {
                            Ok(event) => emit(&handle, &rig_id, &event),
                            Err(RecvError::Lagged(n)) => {
                                tracing::warn!(skipped = n, rig = %rig_id.as_str(), "frontend lagged on arch:event (remote)");
                            }
                            Err(RecvError::Closed) => {
                                // Session closed — likely transport drop.
                                // Loop back to resubscribe; SshRpcClient
                                // reopens on next call per its reconnect
                                // policy.
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                continue 'outer;
                            }
                        }
                    }
                }
            }
        }
    })
}

fn emit(handle: &AppHandle, rig_id: &RigId, event: &ArchEvent) {
    let payload = RigEvent { rig_id, event };
    if let Err(e) = handle.emit(EVENT_NAME, &payload) {
        tracing::warn!(error = %e, rig = %rig_id.as_str(), "failed to emit arch event");
    }
}

fn emit_file(handle: &AppHandle, rig_id: &RigId, event: &FileEvent) {
    let payload = RigFileEvent { rig_id, event };
    if let Err(e) = handle.emit(FILE_EVENT_NAME, &payload) {
        tracing::warn!(error = %e, rig = %rig_id.as_str(), "failed to emit file event");
    }
}
