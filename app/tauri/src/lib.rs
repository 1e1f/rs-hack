//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! `yah-tauri` — Tauri host binary for the yah desktop app.
//!
//! Owns one [`KgService`] in Tauri-managed state, exposes the `arch.*`
//! surface as `#[tauri::command]` async functions, and bridges the
//! daemon's `ArchEvent` broadcast channel to Tauri's window event bus.
//!
//! Boot flow:
//! 1. App `setup()` constructs `KgService` with the standard indexer
//!    registry (Rust + TS for now).
//! 2. Spawns the event-bridge task forwarding `ArchEvent`s to the main
//!    window as `arch:event` payloads.
//! 3. If `YAH_RIG_ROOT` is set, calls `boot()` + `start_watching()`
//!    immediately. Otherwise the frontend calls `arch_open_rig(path)`
//!    once the user picks a folder.
//!
//! What's intentionally _not_ here:
//! * No persistence layer (KV cache, SSH key store) — those will be
//!   added when remote-rig support lands.
//! * No HTTP / SSH-RPC transports — same; v1 is local rigs only.
//! * No "split between renderer and host" — Tauri commands are direct
//!   pass-throughs to the daemon.
//!
//! @arch:see(architecture/yah-roadmap-2026Q2.md)
//!
//!
//!
//! @yah:ticket(R017-T7, "Wire boot_with_snapshot into Tauri host startup")
//! @yah:status(open)
//! @yah:parent(R017)
//! @yah:next("Replace KgService::boot() in app/tauri/src/lib.rs with boot_with_snapshot(rig_root, default_snapshot_path(rig_root)) — falls back to full boot if snapshot missing/mismatched")
//! @yah:next("After IndexFinished in the Tauri command layer (or on app suspend), call svc.save_default() so subsequent cold-starts pay near-zero cost")
//! @yah:next("Verify the order-of-magnitude speedup gate from R017-F3 against a real-sized rig (rs-hack itself is fine)")

pub mod commands;
pub mod event_bridge;
pub mod state;

use crate::state::{load_rigs_file, save_rigs_file, AppState};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Tauri entry point. Public so `main.rs` and any future mobile entry
/// can call it.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::empty();
            app.manage(state.clone());
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(boot_registry(state, app_handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::arch_open_rig,
            commands::arch_close_rig,
            commands::arch_subgraph,
            commands::arch_lookup,
            commands::arch_node,
            commands::arch_neighbors,
            commands::arch_roots,
            commands::arch_stats,
            commands::arch_languages,
            commands::arch_reindex_path,
            commands::arch_touch,
            commands::arch_list_tickets,
            commands::arch_list_relays,
            commands::arch_get_ticket,
            commands::arch_validate,
            commands::arch_ticket_prompt,
            commands::rig_list,
            commands::rig_attach,
            commands::rig_detach,
            commands::rig_set_active,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Reattach every rig from `~/.yah/rigs.json`, restore the previous
/// active rig (if it's still in the registry), then honour
/// `YAH_RIG_ROOT` for dev convenience: attach the path, set it active,
/// and auto-boot+watch.
///
/// Daemons for non-active rigs are constructed but **not** booted —
/// indexing takes seconds and the user pays for it lazily by clicking
/// the rig in the selector. This keeps cold-start cheap even with a
/// dozen rigs persisted.
async fn boot_registry(state: AppState, app_handle: AppHandle) {
    let file = load_rigs_file();
    for rig in &file.rigs {
        let _ = state
            .attach_rig(rig.path.clone(), rig.name.clone(), app_handle.clone())
            .await;
    }
    if let Some(last) = file.last_active {
        if !state.set_active(last).await {
            tracing::warn!("lastActive rig from rigs.json is not in the registry");
        }
    }

    if let Ok(p) = std::env::var("YAH_RIG_ROOT") {
        let path = PathBuf::from(&p);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.clone());
        let id = state
            .attach_rig(path.clone(), name, app_handle.clone())
            .await;
        let _ = state.set_active(id).await;
        // Persist so the dev YAH_RIG_ROOT survives a restart even
        // without the user ever opening the rig selector.
        let snapshot = state.snapshot_to_file().await;
        if let Err(e) = save_rigs_file(&snapshot) {
            tracing::warn!(error = %e, "failed to persist rigs.json after YAH_RIG_ROOT attach");
        }
        if let Some(svc) = state.active_svc().await {
            tauri::async_runtime::spawn(async move {
                if let Err(e) = svc.boot(path).await {
                    tracing::error!(error = %e, "auto-boot failed");
                    return;
                }
                if let Err(e) = svc.start_watching().await {
                    tracing::error!(error = %e, "watcher start failed");
                }
            });
        }
    }
}
