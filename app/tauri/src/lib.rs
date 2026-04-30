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
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R017)
//! @yah:handoff("Tauri startup wiring landed at lib.rs:203. Postcard v3 wire format at .yah/cache/snapshot.bin. Current speedup: ~7-8x (full ~500-630ms vs snap ~65-75ms). F3's order-of-magnitude (10x) gate not yet met. Phase breakdown under YAH_SNAPSHOT_DEBUG=1: file io 2-4ms (24MB), postcard parse 43-45ms, wire→canonical 1-2ms, store+anno restore 14-20ms, fingerprint+reconcile 3ms. The previous handoff blamed wire→canonical; verified that's wrong (1-2ms, basically free). Real bottleneck is postcard parse on a 24MB file — pushing read+parse below ~25ms needs the snapshot itself to shrink to ~12MB. Concrete sub-ticket spawned: R017-T9 (snapshot string interning).")
//! @yah:gotcha("Postcard rejects internally/adjacently-tagged serde enums (#[serde(tag=...)] or tag+content) with WontImplement at deserialize time. yah_kg::kind has TsKind/DocKind/KodaKind tagged that way; yah_kg::edge has EdgeKind/KodaEdge; yah_kg::anno has AnnotationKind/ThinkBudget. Every one of those needs a wire wrapper or postcard fails at workspace scale — small e2e rigs with one Rust file won't exercise the variants and the bug looks fine until the perf gate runs.")
//! @yah:gotcha("Re-running the perf test back-to-back gets warm-cache numbers. Run from a fresh shell on a real cold-cache rig for true numbers; --release matters (debug is ~5x absolute slower, same ratio). Use YAH_SNAPSHOT_DEBUG=1 with --nocapture to print per-phase ms.")
//! @yah:gotcha("Predicted '~10ms read+parse' from the prior handoff was unrealistic — that math assumed wire→canonical dominated. Postcard at ~556 MB/s on 24MB is the floor; only a smaller snapshot moves the number meaningfully.")
//! @yah:next("Land R017-T9 (string-intern snapshot) — that's the path to the F3 gate. Then archive R017-T7 + R017-T9 + R017-F3 together.")
//! @yah:next("Once F3 gate is met: drop orphan .yah/cache/snapshot.json + v2 snapshot.msgpack from rigs predating the v3 bump (version mismatch already falls through to full boot+save; orphans just waste disk).")
//! @yah:next("Deferred polish: per-rig listener on ArchEvent::IndexFinished that debounce-saves the snapshot on a 5s quiet window. Naive every-event save would write ~24MB on every keystroke.")
//!
//! @yah:ticket(R026-T3, "Register two new commands in invoke_handler!()")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R026)
//! @yah:next("Add commands::arch_list_authored_files and commands::arch_read_authored_file to the tauri::generate_handler! macro list alongside existing arch_* entries.")
//!
//! @yah:relay(R028, "Yah agent runtime: SDK pane + per-ticket context fork + skill hooks")
//! @yah:status(open)
//! @yah:phase(P1)
//! @yah:parent(R013)
//! @yah:gotcha("Two runtimes, not a primary+fallback: R028 owns the Claude track (Anthropic-native via the SDK — subscription OAuth, prompt caching, extended thinking, Anthropic's native tool_use protocol). R031 owns the bespoke harness (Ollama-first OpenAI-compat backend in yah-runner + host-side tool registry in app/tauri/src/agent_tools.rs). Both consume the same Prelude and emit the same AgentEvent stream; what diverges is the wire protocol. Don't paper over the split — each vendor's affordances are why both paths exist. See architecture/yah-agent-runtime.md 'Two first-class runtimes' for the framing.")
//! @yah:gotcha("R028 ↔ R031 file boundary: agent_tools.rs and yah-runner/src/openai_compat.rs are R031-owned; agent.rs's run_anthropic_turn / build_anthropic_body and the Claude session path are R028-owned. start_runner_session already wires R031's KgToolRegistry into the OpenAI-compat runner (agent.rs:488) — don't re-wire it. Anthropic tool-use, when it lands, plumbs into run_anthropic_turn here on the native tool_use/tool_result content-block shape, not via R031's registry. Shared vocabulary lives in yah_kg::agent::{AgentEvent, Message} — coordinate before adding ToolCalled/ToolResult variants so both paths can consume them.")
//! @yah:gotcha("Approval gate placement (load-bearing for both R028 and R031-F5): the gate lives inside KgToolRegistry at the host layer, between the runner-shaped ToolRegistry::execute and Tool::execute — NOT inside yah_runner::ToolRegistry::execute. The yah_runner trait impl is a thin shim that forwards to KgToolRegistry::execute_gated; the Claude path calls execute_gated directly. One gate, two callers. KgToolRegistry should also expose Tool borrow access (e.g. fn tool(name) -> Option<&dyn Tool> or Vec<&dyn Tool>) so the Anthropic loop can iterate schemas + dispatch without round-tripping through the OpenAI-shaped runner trait. See architecture/agent-tool-calls.md 'Approval gate placement (load-bearing)'.")
//! @yah:next("Start P1: write annotation schema (T2), then prelude assembler (T3), then SDK pane (T4)")
//! @yah:verify("yah board inflight | grep R028")
//! @arch:see(architecture/yah-agent-runtime.md)
//! @arch:see(architecture/yah-roadmap-2026Q2.md)
//!
//! @yah:ticket(R017-T9, "Snapshot string interning to hit F3 order-of-magnitude verify gate")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R017)
//! @yah:handoff("Snapshot wire bumped v3→v4 with two interning tables: StringInterner (Vec<String>) and NodeIdInterner (Vec<NodeId>). NodeRef.file, AnnotationRef.source_file, property keys, property values, ticket id/parent/kind/etc, tag namespaces are interned as Sid (u32 varint). NodeRef.id, EdgeOut.from/to, AnnotationRef.anchor, doc anchors, property anchors, AnnotationIndex entry keys are interned as Nid (u32). NodeRef.label/qualified left inline (mostly unique long strings — interning them was a wash on size and a net loss on unpack-clone time). Snapshot size: ~24MB→~13MB (45% smaller). Postcard parse: 43ms→14ms (3× faster). Snap-boot phase breakdown: file_io 1ms, postcard_parse 14ms, unpack 10-13ms, store+anno restore 15-18ms, fingerprint+reconcile 3ms = ~50ms total vs ~75ms pre-T9. Speedup: typically 8-10× (was 7-8×) with peaks >10× when full-boot is on the slow side of its noise band. Also micro-optimized Store::rebuild_from_parts to skip per-node String clone on by_file lookups (entry().or_default() always clones key; replaced with get_mut/insert pattern, ~3ms saved). All daemon e2e tests pass (34/34).")
//! @yah:gotcha("From<X> for XWire was replaced with explicit pack/unpack methods that thread &mut StringInterner / &mut NodeIdInterner through the call tree. Plain From is preserved for value-only types (NodeKindWire/EdgeKindWire/ThinkBudgetWire). KgSnapshotWire::pack(snap)/unpack() are the public entry points snapshot.rs calls. RPC wire untouched — interning is snapshot-only.")
//! @yah:gotcha("v4 wire format is incompatible with v3 — older .yah/cache/snapshot.bin trips a version mismatch and falls through to a full reindex+resave (existing behavior, no migration code needed).")
//! @yah:assumes("F3 'order-of-magnitude' gate accepts the typical-case 8-10× speedup — the test only asserts snap_ms < full_ms, not 10×. If F3 requires a hard 10× floor, follow-up work is needed (see cleanup).")
//! @yah:cleanup("To reliably exceed 10× under load: (a) restore-side optimization — rebuild_from_iter/restore_from_iter to fuse wire→canonical with petgraph insert and skip the intermediate Vec/BTreeMap allocation in unpack; (b) drop EdgeOut.id from the wire (deterministic from from/to/kind via EdgeId::compute, would save ~1.2MB and ~5ms); (c) consider moving by_file index into the snapshot itself rather than rebuilding it on restore.")
//! @yah:cleanup("Drop orphan .yah/cache/snapshot.json + v2 snapshot.msgpack from rigs predating the v3 bump (now v4). Version mismatch already falls through to full boot+save; orphans just waste disk.")
//! @yah:verify("cargo test -p yah-kg-daemon --test e2e --release boot_with_snapshot_is_faster_than_full_boot -- --ignored --nocapture")
//! @yah:verify("YAH_SNAPSHOT_DEBUG=1 cargo test -p yah-kg-daemon --test e2e --release boot_with_snapshot_is_faster_than_full_boot -- --ignored --nocapture # phase breakdown — postcard_parse should be ~14ms")
//! @yah:verify("cargo test -p yah-kg-daemon --release # round-trip e2e tests should still pass (34/34)")

pub mod agent;
pub mod agent_approval;
pub mod agent_process;
pub mod agent_settings;
pub mod agent_tools;
pub mod api_keys;
pub mod backend;
pub mod claude_cli;
pub mod claude_shape;
pub mod commands;
pub mod event_bridge;
pub mod hetzner;
pub mod identities;
pub mod ssh_keys;
pub mod state;
pub mod terminal;

use crate::agent::AgentSessions;
use crate::state::{load_rigs_file, save_rigs_file, AppState, RemoteRigSpec, RigKind};
use crate::terminal::TerminalSessions;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Initialise the tracing subscriber so `tracing::warn!` / `error!` etc
/// from across the host (agent runtime, daemon, runner) actually reach
/// the terminal. Falls back to `info` level filter when `RUST_LOG` is
/// unset so a stock `cargo run -p yah-tauri` stays readable but agent /
/// tool-call errors still surface.
///
/// Idempotent — `try_init()` returns `Err` if a subscriber is already
/// installed (e.g. tests, hot-reloads), and we silently ignore it.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,yah_tauri=debug,yah_runner=debug,yah_kg_daemon=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .compact()
        .try_init();
}

/// Tauri entry point. Public so `main.rs` and any future mobile entry
/// can call it.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    // Load pinned `crab` (HAo) wire constants from the embedded TOML.
    // Failure leaves the global as None; AnthropicAuth::Oauth detects
    // that and falls back to its hardcoded degraded path.
    claude_shape::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::empty();
            app.manage(state.clone());
            // Agent runtime registry — process-wide, separate from the
            // rig-keyed AppState because sessions span rig lifetimes
            // (closing a rig shouldn't kill an active chat) and the
            // shape (HashMap<SessionId, Arc<Mutex<AgentSession>>>) is
            // unrelated to the rig registry's API.
            app.manage(AgentSessions::new());
            // Terminal session registry — process-wide, same rationale
            // as AgentSessions. Wrapped in Arc so the spawned per-
            // session task can drop itself from the registry on
            // disconnect via app_handle.try_state.
            app.manage(Arc::new(TerminalSessions::new()));
            // Identity registry serializing-state — gates the
            // load → mutate → save sequence inside identity_* commands
            // so concurrent invokes don't race on identities.json
            // (R034-T1).
            app.manage(crate::identities::IdentitiesState::new());
            // R027-T7: fold any per-provider keychain entries from
            // before the single-blob vault into the vault, then delete
            // the originals. Best-effort and idempotent — second boot
            // finds nothing to migrate. Run on a tauri::async_runtime
            // task so the keychain prompt (if any) doesn't block the
            // first window paint.
            tauri::async_runtime::spawn(async move {
                match crate::api_keys::migrate_legacy_entries() {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(
                        migrated = n,
                        "api_keys: per-provider → vault migration complete",
                    ),
                    Err(e) => tracing::warn!(
                        error = %e,
                        "api_keys: per-provider → vault migration failed",
                    ),
                }
            });
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
            commands::arch_list_authored_files,
            commands::arch_read_authored_file,
            commands::file_read,
            commands::dir_list,
            commands::file_watch,
            commands::dir_watch,
            commands::file_unwatch,
            commands::arch_languages,
            commands::arch_reindex_path,
            commands::arch_touch,
            commands::arch_list_tickets,
            commands::arch_list_relays,
            commands::arch_get_ticket,
            commands::arch_validate,
            commands::arch_ticket_prompt,
            commands::arch_move_ticket,
            commands::arch_archive_ticket,
            commands::rig_list,
            commands::rig_attach,
            commands::rig_attach_remote,
            commands::rig_detach,
            commands::rig_set_active,
            api_keys::api_key_set,
            api_keys::api_key_has,
            api_keys::api_key_delete,
            claude_cli::claude_cli_probe,
            claude_cli::ollama_serve_probe,
            hetzner::hetzner_list_servers,
            hetzner::hetzner_list_ssh_keys,
            hetzner::hetzner_upload_ssh_key,
            hetzner::hetzner_create_server,
            hetzner::hetzner_list_server_types,
            hetzner::hetzner_list_locations,
            hetzner::hetzner_list_images,
            ssh_keys::ssh_key_list_local,
            ssh_keys::ssh_key_generate,
            identities::identity_list,
            identities::identity_create,
            identities::identity_import,
            identities::identity_remove,
            identities::identity_probe_all,
            identities::identity_probe_hetzner,
            identities::identity_probe_github,
            identities::identity_authorize_hetzner,
            identities::identity_deauthorize_hetzner,
            identities::identity_authorize_github,
            identities::identity_deauthorize_github,
            agent::agent_start_session,
            agent::agent_start_chat_session,
            agent::agent_send,
            agent::agent_stop,
            agent::agent_list_sessions,
            agent::agent_list_models,
            agent::agent_approval_decide,
            agent::agent_approval_rules_list,
            agent::agent_approval_rules_add,
            agent::agent_approval_rules_remove,
            agent::agent_settings_get,
            agent::agent_settings_set,
            terminal::terminal_open_ssh,
            terminal::terminal_open_local,
            terminal::terminal_input,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::terminal_list_sessions,
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
    // R034-T6: walk rigs.json for legacy `keyPath`-only entries and
    // populate the identity registry + stamp `identity_id` back onto
    // the rig. Idempotent on every boot (already-migrated rigs no-op),
    // and best-effort — failures are logged but never block reattach.
    match identities::migrate_rigs_keypath_to_identity_id() {
        Ok(0) => {}
        Ok(n) => tracing::info!(migrated = n, "rigs.json keyPath → identityId migration complete"),
        Err(e) => tracing::warn!(error = %e, "rigs.json keyPath → identityId migration failed"),
    }
    let file = load_rigs_file();
    for rig in &file.rigs {
        match rig.kind {
            RigKind::Local => {
                let _ = state
                    .attach_rig(rig.path.clone(), rig.name.clone(), app_handle.clone())
                    .await;
            }
            RigKind::Remote => {
                // Persisted remotes need host/user to be reattachable;
                // a malformed entry (older rigs.json from before the
                // remote spec landed) is logged and skipped rather
                // than aborting boot for the rest of the registry.
                let (Some(host), Some(user)) = (rig.host.clone(), rig.user.clone()) else {
                    tracing::warn!(
                        rig = %rig.id.as_str(),
                        "remote rig in rigs.json missing host/user; skipping reattach",
                    );
                    continue;
                };
                let spec = RemoteRigSpec {
                    host,
                    user,
                    workspace_path: rig.path.clone(),
                    port: rig.port,
                    key_path: rig.key_path.clone(),
                    name: Some(rig.name.clone()),
                };
                let _ = state.attach_remote_rig(spec, app_handle.clone()).await;
            }
        }
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
                let snap_path = yah_kg_daemon::default_snapshot_path(&path);
                if let Err(e) = svc.boot_with_snapshot(path, &snap_path).await {
                    tracing::error!(error = %e, "auto-boot failed");
                    return;
                }
                if let Err(e) = svc.start_watching().await {
                    tracing::error!(error = %e, "watcher start failed");
                }
                if let Err(e) = svc.save_default().await {
                    tracing::warn!(error = %e, "failed to persist KG snapshot after auto-boot");
                }
            });
        }
    }
}
