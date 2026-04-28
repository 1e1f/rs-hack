//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Forwards each rig's `tokio::sync::broadcast<ArchEvent>` to Tauri's
//! window event bus, stamped with the originating [`RigId`].
//!
//! Multi-rig wrapping (R024-T1): the daemon side is unchanged — every
//! [`KgService`] still emits a plain `ArchEvent` over its own broadcast
//! channel. The bridge wraps each event in [`RigEvent { rig_id, event }`]
//! before emitting to the window, so the renderer can fan events out
//! by rig without needing per-rig subscriptions.
//!
//! Frontend listener:
//! ```ts
//! import { listen } from '@tauri-apps/api/event';
//! const unlisten = await listen<RigEvent>('arch:event',
//!   (e) => routeByRig(e.payload.rig_id, e.payload.event));
//! ```
//!
//! Each attach spawns one bridge task. Detaching the rig aborts the
//! task via the [`tokio::task::JoinHandle`] held in `RigEntry`. A
//! `RecvError::Lagged` from a slow renderer drops a frame but doesn't
//! tear down the bridge.

use crate::state::RigId;
use serde::Serialize;
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::error::RecvError;
use yah_kg::event::ArchEvent;
use yah_kg_daemon::KgService;

const EVENT_NAME: &str = "arch:event";

/// Wire payload: rig origin + the daemon's own event.
#[derive(Debug, Clone, Serialize)]
pub struct RigEvent<'a> {
    pub rig_id: &'a RigId,
    #[serde(flatten)]
    pub event: &'a ArchEvent,
}

pub fn spawn_for(rig_id: RigId, svc: Arc<KgService>, handle: AppHandle) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut rx = svc.subscribe();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let payload = RigEvent {
                        rig_id: &rig_id,
                        event: &event,
                    };
                    if let Err(e) = handle.emit(EVENT_NAME, &payload) {
                        tracing::warn!(error = %e, rig = %rig_id.as_str(), "failed to emit arch event");
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, rig = %rig_id.as_str(), "frontend lagged on arch:event");
                }
                Err(RecvError::Closed) => break,
            }
        }
    })
}
