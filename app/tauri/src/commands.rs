//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Tauri commands that wrap [`KgService`].
//!
//! Each command:
//! 1. Pulls `AppState` out of Tauri-managed state.
//! 2. Calls the matching async method on the daemon.
//! 3. Returns the result, mapping errors to `String` for the JS bridge.
//!
//! The param/return types come straight from `yah_kg::rpc` — they're
//! already `Serialize + Deserialize`, so no conversion layer is needed.
//!
//! Naming: `arch_*` keeps the JS side aligned with the conceptual
//! `arch.*` namespace from the daemon's RPC surface, while satisfying
//! Tauri's restriction that command names be valid Rust identifiers.
//!
//! @yah:ticket(R017-F5, "Tauri commands for ticket mutation paths (move ticket → status rewrites source comment)")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Mutation rewrites the @yah:status(...) line in source; watcher re-indexes; UI gets event")
//! @yah:next("Cover the same transition matrix as the existing board server (open→active, active→{open,handoff,review}, handoff→{active,review}, review→handoff)")
//!
//! @yah:ticket(R018-F3, "Tauri command surface: send prompt + stream agent responses to AgentView")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:next("agent.send(sessionId, text), agent.stop(sessionId)")
//! @yah:next("Stream events through the existing event_bridge so AgentView re-renders live")
//!
//! @yah:ticket(R024-T2, "Tauri rig-management commands + rig_id parameter on every arch_* command")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R024)
//! @yah:next("New commands: list_rigs() -> Vec<RigDto>, attach_rig(path, name) -> RigDto, detach_rig(id), set_active_rig(id)")
//! @yah:next("Add rig_id: RigId to every existing arch_* command param; route to the right KgService via AppState lookup")
//! @yah:next("RigDto: { id, name, path, kind: 'local'|'remote', reachable, lastActiveAt }; matches yah-ui Rig interface in src/types.ts")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:handoff("Rig-management surface landed. Four new commands: rig_list -> Vec<RigDto>, rig_attach(path, name) -> RigDto, rig_detach(rig_id) -> bool, rig_set_active(rig_id) -> bool. Each mutation snapshots the registry and writes it through state::save_rigs_file (warning logged on I/O failure; never aborts the command). Every existing arch_* command now takes a rig_id: RigId param and routes via state.svc_for(&id) (was: active(&state)). arch_open_rig drops its rig_root: String param — path is now looked up from the rig entry via state.path_for, since attach precedes open in the new flow. Renderer side: yah-ui's `Rig` interface in src/types.ts is a subset of RigDto (missing path/lastActiveAt) — those fields are forward-compatible. Frontend integration is mock-driven via R024-T3 and isn't blocked.")
//! @yah:next("yah-ui: pass rig_id alongside every arch.* invoke once env adapter switches to the Tauri target (R024-T3 stays mock-driven for now)")
//! @yah:next("Extend yah-ui Rig interface to include path + lastActiveAt to consume the new fields")

use crate::state::{save_rigs_file, AppState, RigDto, RigId};
use std::path::PathBuf;
use std::sync::Arc;
use yah_kg::event::IndexReason;
use yah_kg::ids::{NodeFull, NodeId};
use yah_kg::kind::Lang;
use yah_kg::rpc::{
    GetTicketParams, GetTicketResult, ListRelaysParams, ListRelaysResult, ListTicketsParams,
    ListTicketsResult, LookupParams, LookupResult, NeighborsParams, NeighborsResult, RootsParams,
    RootsResult, StatsResult, Subgraph, SubgraphParams, TicketPromptParams, TicketPromptResult,
    ValidateParams, ValidateResult,
};
use yah_kg_daemon::KgService;
use yah_kg_store::WalkSummary;

/// Resolve a rig's daemon by id. The renderer always knows which rig
/// it's targeting (the rig selector is its source of truth), so every
/// `arch_*` command takes a `rig_id` and looks up the daemon explicitly
/// rather than depending on a mutable "active" pointer.
async fn svc_by_id(state: &AppState, id: &RigId) -> Result<Arc<KgService>, String> {
    state
        .svc_for(id)
        .await
        .ok_or_else(|| format!("rig {} not attached", id.as_str()))
}

/// Persist the registry after a mutation. I/O failures are logged but
/// don't propagate — the in-memory state is still correct, and the
/// next mutation will retry the write.
async fn persist(state: &AppState) {
    let snapshot = state.snapshot_to_file().await;
    if let Err(e) = save_rigs_file(&snapshot) {
        tracing::warn!(error = %e, "failed to persist rigs.json");
    }
}

/// Open a rig: boot its daemon against the path stored in its registry
/// entry and start the file watcher. Idempotent. The rig must already
/// have been attached via `rig_attach`.
#[tauri::command]
pub async fn arch_open_rig(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<WalkSummaryDto, String> {
    let svc = svc_by_id(&state, &rig_id).await?;
    let path = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} not attached", rig_id.as_str()))?;
    let summary = svc.boot(path).await.map_err(|e| e.to_string())?;
    svc.start_watching().await.map_err(|e| e.to_string())?;
    Ok(WalkSummaryDto::from(summary))
}

/// Stop the named rig's watcher.
#[tauri::command]
pub async fn arch_close_rig(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<(), String> {
    let svc = svc_by_id(&state, &rig_id).await?;
    svc.stop_watching().await;
    Ok(())
}

#[tauri::command]
pub async fn arch_subgraph(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: SubgraphParams,
) -> Result<Subgraph, String> {
    Ok(svc_by_id(&state, &rig_id).await?.subgraph(params).await)
}

#[tauri::command]
pub async fn arch_lookup(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: LookupParams,
) -> Result<LookupResult, String> {
    Ok(svc_by_id(&state, &rig_id).await?.lookup(params).await)
}

#[tauri::command]
pub async fn arch_node(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    id: NodeId,
) -> Result<Option<NodeFull>, String> {
    Ok(svc_by_id(&state, &rig_id).await?.node(id).await)
}

#[tauri::command]
pub async fn arch_neighbors(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: NeighborsParams,
) -> Result<NeighborsResult, String> {
    Ok(svc_by_id(&state, &rig_id).await?.neighbors(params).await)
}

#[tauri::command]
pub async fn arch_roots(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: RootsParams,
) -> Result<RootsResult, String> {
    Ok(svc_by_id(&state, &rig_id).await?.roots(params).await)
}

#[tauri::command]
pub async fn arch_stats(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<StatsResult, String> {
    Ok(svc_by_id(&state, &rig_id).await?.stats().await)
}

#[tauri::command]
pub async fn arch_languages(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<Vec<Lang>, String> {
    Ok(svc_by_id(&state, &rig_id).await?.languages())
}

/// Enumerate every synthetic Ticket node currently in the graph. The Board
/// UI calls this instead of file-scanning for `@yah:ticket(...)` directives.
#[tauri::command]
pub async fn arch_list_tickets(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: Option<ListTicketsParams>,
) -> Result<ListTicketsResult, String> {
    Ok(svc_by_id(&state, &rig_id)
        .await?
        .list_tickets(params.unwrap_or_default())
        .await)
}

/// Same as `arch_list_tickets` but for synthetic Relay nodes — feeds the
/// Epics column and the relay-pickers in the Board UI.
#[tauri::command]
pub async fn arch_list_relays(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: Option<ListRelaysParams>,
) -> Result<ListRelaysResult, String> {
    Ok(svc_by_id(&state, &rig_id)
        .await?
        .list_relays(params.unwrap_or_default())
        .await)
}

/// Look up one synthetic Ticket node by its bare ID (e.g. `R042-T1`). The
/// Board UI calls this when opening a ticket card to refresh the full
/// payload (handoff text, next steps, etc.) without re-listing the world.
#[tauri::command]
pub async fn arch_get_ticket(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: GetTicketParams,
) -> Result<GetTicketResult, String> {
    Ok(svc_by_id(&state, &rig_id).await?.get_ticket(params).await)
}

/// Run the rule validator on the rig and return the violations. The Arch
/// view calls this on demand (and after any reindex) to surface inline
/// warnings on the offending nodes; an empty `violations` list means
/// every authored `@yah:rule(...)` in scope passed.
///
/// `params` is optional — omitting it (or omitting `scope`) validates the
/// whole rig (`Scope::All`).
#[tauri::command]
pub async fn arch_validate(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: Option<ValidateParams>,
) -> Result<ValidateResult, String> {
    Ok(svc_by_id(&state, &rig_id)
        .await?
        .validate(params.unwrap_or_default())
        .await)
}

/// Render the canonical pickup or review markdown for one work-item id.
/// The TicketCard's "Prompt" / "Review" buttons call this so the
/// clipboard payload matches `yah board show <id> --prompt` byte-for-byte
/// — both paths flow through `yah_kg::prompt::render`.
///
/// `result.markdown` is `null` when the id isn't on the board (mirrors
/// `arch_get_ticket`'s null-when-missing convention).
#[tauri::command]
pub async fn arch_ticket_prompt(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: TicketPromptParams,
) -> Result<TicketPromptResult, String> {
    Ok(svc_by_id(&state, &rig_id)
        .await?
        .ticket_prompt(params)
        .await)
}

/// Manually reindex one path. The watcher already covers most cases;
/// the frontend uses this when it knows pi-mono just wrote to a file
/// and wants the update to carry `reason = AgentEdit` instead of
/// `FileWatch`.
#[tauri::command]
pub async fn arch_reindex_path(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    path: String,
    reason: IndexReasonDto,
) -> Result<(), String> {
    svc_by_id(&state, &rig_id)
        .await?
        .reindex_path(std::path::Path::new(&path), reason.into())
        .await
        .map_err(|e| e.to_string())
}

/// Forward a pi-mono tool result so the daemon can resolve `path:line`
/// strings to NodeIds and fan out an `AgentTouch` event.
#[tauri::command]
pub async fn arch_touch(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    paths: Vec<String>,
    tool: String,
    relay: String,
) -> Result<(), String> {
    svc_by_id(&state, &rig_id)
        .await?
        .touch(&paths, &tool, &relay)
        .await;
    Ok(())
}

// ---------- Rig-management commands ----------
//
// These don't carry a `rig_id` (or take it as the operand) because they
// manage the registry itself rather than dispatching to a daemon. Every
// mutation persists the registry to `~/.yah/rigs.json` so the next boot
// reattaches the same set.

const NO_RIG_FOUND_AFTER_ATTACH: &str =
    "rig_attach succeeded but rig vanished from registry — concurrent detach?";

/// Snapshot of every attached rig as the wire DTO. The renderer's rig
/// selector calls this at startup and after every attach/detach event.
#[tauri::command]
pub async fn rig_list(state: tauri::State<'_, AppState>) -> Result<Vec<RigDto>, String> {
    Ok(state.list_rig_dtos().await)
}

/// Add a folder to the rig registry, constructing its daemon and
/// spawning the per-rig event bridge. Idempotent: re-attaching the same
/// path returns the existing rig with `name` refreshed. Does not boot
/// indexing — the user pays for that lazily by clicking the rig.
#[tauri::command]
pub async fn rig_attach(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    path: String,
    name: String,
) -> Result<RigDto, String> {
    let id = state.attach_rig(PathBuf::from(path), name, app).await;
    persist(&state).await;
    state
        .rig_dto_for(&id)
        .await
        .ok_or_else(|| NO_RIG_FOUND_AFTER_ATTACH.to_string())
}

/// Drop a rig from the registry: aborts its event bridge and clears the
/// active pointer if it pointed here. Returns whether the rig existed.
#[tauri::command]
pub async fn rig_detach(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<bool, String> {
    let removed = state.detach_rig(&rig_id).await;
    if removed {
        persist(&state).await;
    }
    Ok(removed)
}

/// Mark a rig as the focused one. Stamps `lastActiveAt` so the selector
/// can sort by recency. Returns `false` if the rig isn't attached.
#[tauri::command]
pub async fn rig_set_active(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<bool, String> {
    let ok = state.set_active(rig_id).await;
    if ok {
        persist(&state).await;
    }
    Ok(ok)
}

// ---------- DTOs ----------
//
// `WalkSummary` doesn't `Serialize`, and we don't want to leak the full
// `IndexReason` enum's tag-shape into the JS payload. Tiny camelCase DTOs
// here keep the wire format predictable.

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalkSummaryDto {
    pub files_seen: u32,
    pub files_indexed: u32,
    pub files_skipped: u32,
    pub parse_errors: u32,
}

impl From<WalkSummary> for WalkSummaryDto {
    fn from(s: WalkSummary) -> Self {
        Self {
            files_seen: s.files_seen,
            files_indexed: s.files_indexed,
            files_skipped: s.files_skipped,
            parse_errors: s.parse_errors,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexReasonDto {
    Boot,
    FileWatch,
    Manual,
    AgentEdit,
}

impl From<IndexReasonDto> for IndexReason {
    fn from(d: IndexReasonDto) -> Self {
        match d {
            IndexReasonDto::Boot => IndexReason::Boot,
            IndexReasonDto::FileWatch => IndexReason::FileWatch,
            IndexReasonDto::Manual => IndexReason::Manual,
            IndexReasonDto::AgentEdit => IndexReason::AgentEdit,
        }
    }
}
