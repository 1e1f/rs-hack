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
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Mutation rewrites the @yah:status(...) line in source; watcher re-indexes; UI gets event")
//! @yah:next("Cover the same transition matrix as the existing board server (open→active, active→{open,handoff,review}, handoff→{active,review}, review→handoff)")
//! @yah:handoff("Tauri ticket-move surface landed. New shared module yah-kg/src/board_mutate.rs (pure-string) owns the transition matrix (bucket_to_status, status_to_bucket, allowed_transitions) and the source-rewrite helpers (locate_ticket_block, set_or_insert_annotation, comment_sigil) — same matrix the existing yah board move CLI and hack-board TS server enforce, but now callable from a library rather than only from the yah binary. KgService::move_ticket(MoveTicketParams { id, to_bucket }) -> MoveTicketResult { id, from_status, to_status, file, line } in yah-kg-daemon: takes read locks, derives epic-ness via Board::from_work_items, walks the lex-first anchor, validates transition, rewrites source, and calls reindex_path with IndexReason::AgentEdit so subscribers see one WorkItemChanged event with the right cause. Domain rejections surface as new DaemonError::Conflict(String) — analogous to the board server's HTTP 409. Tauri command arch_move_ticket(rig_id, params) is a thin wrapper, registered in app/tauri/src/lib.rs invoke_handler. Tests: 7 unit tests in board_mutate (transitions, bucket round-trip, rewrite in //!/// /# blocks, multi-id isolation, missing-id) + 4 new e2e tests (happy path with AgentEdit event assertion, disallowed transition, unknown bucket / missing id, epic refusal). cargo test -p yah-kg 33/33, cargo test -p yah-kg-daemon 26/26, cargo build -p yah-tauri green.")
//! @yah:next("yah-ui: add rpc.move_ticket(rigId, id, to_bucket) -> MoveTicketResult to env/index.ts + tauri.ts (invoke('arch_move_ticket', { rigId, params: { id, toBucket } })); browser.ts can throw 'not implemented' as a stub")
//! @yah:next("BoardView drag-and-drop: call rpc.move_ticket on column drop; show the Conflict error message as a toast on rejection (epic mutation, transition denied) — these aren't bugs, they're the same UI dim semantics the existing hack-board server returns")
//! @yah:next("Cleanup: yah/src/main.rs handle_move() still has its own copies of bucket_to_status, status_to_bucket, allowed_transitions, locate_ticket_block, set_or_insert_annotation, comment_sigil — fold them onto yah_kg::board_mutate so the matrix lives in exactly one place (currently lives in 2: this new module + yah/src/main.rs; the TS copy in hack-board/src/server.ts is a separate language so it'll always be a parallel mirror)")
//! @yah:next("Cleanup: rich payload mutations (handoff/next/verify/cleanup/gotcha/assumes append) are still CLI-only — handle_move accepts them via flags but the Tauri move_ticket only handles status. Add a separate arch_append_annotation command if/when the UI grows handoff/next-step authoring affordances")
//!
//! @yah:ticket(R018-F3, "Tauri command surface: agent.start_session / send / stop")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R018)
//! @yah:next("agent.start_session(ticket_id) -> SessionId; consults R028-F2 prelude assembler then dispatches to backend per @yah:engine")
//! @yah:next("agent.send(session_id, text), agent.stop(session_id)")
//! @yah:next("Stream AgentEvent through the existing event_bridge so AgentView re-renders live")
//! @yah:next("Dispatch matrix: engine=claude:* -> Claude SDK runner (R028-F1); else -> yah-runner (R018-F2)")
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
//!
//! @yah:relay(R026, "Wire arch_list_authored_files + arch_read_authored_file Tauri commands")
//! @yah:status(handoff)
//! @yah:assignee(agent:tauri-owner)
//! @yah:handoff("yah-kg + yah-ui sides of authored .mmd discovery are landed and tested. Daemon exposes KgService::list_authored_files() and KgService::read_authored_file(params) (yah-kg-daemon/src/service.rs ~lines 800-895). RPC types live in yah-kg/src/rpc.rs (ListAuthoredFilesParams/Result, ReadAuthoredFileParams/Result, AuthoredFile, method::LIST_AUTHORED_FILES + READ_AUTHORED_FILE constants, RpcRequest/RpcResponse variants). yah-ui calls invoke('arch_list_authored_files', { rigId }) and invoke('arch_read_authored_file', { rigId, params: { rel_path } }) — see yah-ui/src/env/tauri.ts. UI integration: AuthoredFilesPicker in the left rail (replaces the live-graph sections when a .mmd is selected) and AuthoredMmdPane on the canvas (raw mermaid render with pan/zoom). Three new e2e tests pass in yah-kg-daemon (tests/e2e.rs). Until the Tauri commands land, the picker shows 'No authored .mmd files' even when files exist on disk.")
//! @yah:next("Add #[tauri::command] async fn arch_list_authored_files(state: tauri::State<'_, AppState>, rig_id: RigId) -> Result<ListAuthoredFilesResult, String> in app/tauri/src/commands.rs. Body: Ok(svc_by_id(&state, &rig_id).await?.list_authored_files(ListAuthoredFilesParams::default()).await.map_err(|e| e.to_string())?). Mirror existing arch_stats/arch_validate shape.")
//! @yah:next("Add #[tauri::command] async fn arch_read_authored_file(state: tauri::State<'_, AppState>, rig_id: RigId, params: ReadAuthoredFileParams) -> Result<ReadAuthoredFileResult, String> in same file.")
//! @yah:next("Register both in app/tauri/src/lib.rs invoke_handler!() macro alongside the existing arch_* entries.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-kg-daemon --test e2e authored")
//!
//! @yah:ticket(R026-T1, "Add arch_list_authored_files Tauri command")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R026)
//! @yah:next("#[tauri::command] async fn arch_list_authored_files(state: tauri::State<'_, AppState>, rig_id: RigId) -> Result<ListAuthoredFilesResult, String>; body Ok(svc_by_id(&state, &rig_id).await?.list_authored_files(ListAuthoredFilesParams::default()).await.map_err(|e| e.to_string())?). Mirror arch_stats shape.")
//! @yah:handoff("arch_list_authored_files command added to app/tauri/src/commands.rs; ListAuthoredFilesParams/Result imports added. cargo build -p yah-tauri green; cargo test -p yah-kg-daemon --test e2e authored 4/4 green. Command not yet wired into invoke_handler!() — that lives in R026-T3. T2 (arch_read_authored_file) is independent and can proceed in parallel.")
//!
//! @yah:ticket(R026-T2, "Add arch_read_authored_file Tauri command")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R026)
//! @yah:next("#[tauri::command] async fn arch_read_authored_file(state: tauri::State<'_, AppState>, rig_id: RigId, params: ReadAuthoredFileParams) -> Result<ReadAuthoredFileResult, String>; same shape as task #1, calls svc.read_authored_file(params).")
//! @yah:handoff("arch_read_authored_file command added in app/tauri/src/commands.rs (mirrors T1's shape: thin wrapper, DaemonError::Conflict from sandbox-escape mapped to string error). ReadAuthoredFileParams/Result added to imports. cargo build -p yah-tauri green; cargo test -p yah-kg-daemon --test e2e authored 4/4 green. Still not in invoke_handler!() — that's R026-T3.")
//!
//! @yah:ticket(R027-F3, "Secure storage backend: keyring or stronghold + api_key_set/has/delete Tauri commands (no _get from renderer)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R027)
//! @yah:handoff("Secure storage backend landed. New module app/tauri/src/api_keys.rs uses the keyring crate (3.6, features=apple-native+windows-native+sync-secret-service+crypto-rust) — Apple Keychain on macOS, Credential Manager on Windows, libsecret/dbus on Linux, no openssl dep. Service identity is (\"yah\", provider). Provider names validated to ASCII alphanumeric+-/_ as defense-in-depth even though the renderer's apiKey enum is allowlisted. Three #[tauri::command]s registered in lib.rs invoke_handler: api_key_set/has/delete; no api_key_get exposed — Rust-only api_keys::get for provider clients (Hetzner reader in F6). 2 unit tests on the validator pass; cargo build -p yah-tauri green in 16s.")
//! @yah:next("R027-F4 picks this up: add apiKey { set, has, delete } RPC surface to yah-ui/src/env/index.ts (Rpc trait), implement in tauri.ts via invoke('api_key_set'|'api_key_has'|'api_key_delete'), browser.ts stubs (set/delete no-op, has returns false). Then swap api-keys-context.tsx from in-memory Map to env().rpc.apiKey calls + remove the P1 'not yet persisted' banner.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib api_keys")
//!
//! @yah:ticket(R027-F6, "Hetzner read-only smoke test: GET /v1/servers from Rust-side client, render server list in Infra tab")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R027)
//! @yah:handoff("Hetzner smoke-test path landed end-to-end. New Rust module app/tauri/src/hetzner.rs reads the stored token via api_keys::get(\"hetzner\") and calls GET https://api.hetzner.cloud/v1/servers using reqwest 0.12 (rustls-tls, no openssl) — token never leaves the Tauri host (matches the architecture/settings-api-keys.md threat model). HetznerServer DTO is the renderer-facing subset (id, name, status, server_type, location, ipv4, created); upstream Server fields we don't display are dropped at parse time. New #[tauri::command] hetzner_list_servers() registered in lib.rs invoke_handler. env adapter: HetznerRpc { listServers } added to Rpc trait in yah-ui/src/env/index.ts; tauri.ts invokes the command, browser.ts returns []. New components/infra/HetznerServerList.tsx renders a table (Name/Status/Type/Location/IPv4) with loading/empty/error panes and a header reload button — replaces the F5 'server list pending' splash in App.tsx InfraTab. cargo build -p yah-tauri green; cargo test -p yah-tauri --lib 9/9 green; bun run typecheck + bun run build green.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:next("Real-token round-trip: with a valid Hetzner read-only token set via Settings → API Keys → Hetzner, open the Infra tab; expect the server list to populate (or the empty pane if the project has no servers). Bad token → ErrorPane with 'Hetzner token rejected: 401 unauthorized'.")
//!
//! @yah:ticket(R033-T5, "Mount Monaco + monaco-vscode-api in <FilesView>; replace splash")
//! @yah:assignee(agent:claude)
//! @yah:status(in-progress)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T6, "<FileTree> component: virtualized + dir.watch subscription")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:handoff("FileTree component landed. New file yah-ui/src/components/files/FileTree.tsx renders a lazy-expand tree on top of dir.list, with dir.watch subscribed to the rig root for live refresh. env adapter extended: WireDirEntry / WireDirListResult / WireRigFileEvent / WireWatchId types in env/types.ts; Rpc interface gained dirList + dirWatch + fileUnwatch + onFileEvent; tauri.ts wires invoke('dir_list'/'dir_watch'/'file_unwatch') + listen('file:event'); browser.ts stubs return empty/no-op so component-level inspection still works. FilesView now uses a flex split: 256px FileTree on the left, Monaco on the right. Tree state preserves expanded sub-tree loaded children across parent re-lists (the dir.watch refresh path re-lists the affected parent only, not the whole tree). Ancestor walk in findDir + post-load re-render via setRoot({...prev}) so React picks up Map mutations. Watch-id cleanup uses an unmount-time fileUnwatch with disposed-flag race protection (rig switch mid-arm). Build: typecheck clean; bun build:js still 8.62MB JS / 0.31MB CSS / 2906 modules.")
//! @yah:next("R033-T7 (useFile hook) is the natural pickup: lift selectedPath from FilesView state into a hook that calls env.rpc.fileRead(selectedPath) — note env adapter does NOT yet expose fileRead, since this ticket only added dir.list/dir.watch surface. T7 should add fileRead(rigId, path, range?) to the Rpc interface in env/index.ts + wire invoke('file_read') in tauri.ts + browser.ts stub.")
//! @yah:next("Virtualization was deliberately skipped — the v1 renderer walks the in-memory Map recursively. For monorepos with deeply expanded trees this will hitch. Pattern when needed: flatten visible rows to a list[] and feed react-window or hand-rolled item-renderer. Cleanup; not blocking the P2 acceptance criterion.")
//! @yah:next("FileTree highlights selectedPath when it matches a file row but the current FilesView only sets selectedPath on click — files opened from arch.jumpToFile or KG-overlay won't auto-expand-and-reveal. Reveal-in-tree needs an effect that walks the path, expands each ancestor, and triggers loadDir() until the leaf row is rendered. Sub-ticket once useFile lands.")
//! @yah:gotcha("dir.watch is recursive on the rig root. Big repos with active build outputs (target/, node_modules/) will spam file:event. Skip rules in the daemon's walker do NOT apply to the watcher — every modify event fires. If event spam shows up under big rigs, debounce the loadDir() call by parent path on the renderer side, or push a skip-rules flag into dir.watch on the daemon side.")
//!
//! @yah:ticket(R033-T7, "useFile hook: file.read + Monaco model swap on URI change")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T8, "Monaco theme port: scriptorium + vellum-by-candlelight tokens")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-F9, "KG-overlay extension: hover + CodeLens providers (vscode API)")
//! @yah:assignee(agent:claude)
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:handoff("Released — P3 work, blocked on P1 (file RPCs) and P2 (Monaco mount). Pick up after R033-T5 lands.")
//!
//! @yah:ticket(R033-T10, "KG-overlay decorations: gutter glyphs + relay rail")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T13, "Wire vscode-languageclient through env.rpc.lsp; rust-analyzer + tsserver defaults")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T14, "Monaco un-readonly + save with mtime + external-change prompt")
//! @yah:status(open)
//! @yah:phase(P5)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//!
//! @yah:ticket(R033-T16, "Wire anno-wasm as DiagnosticCollection in KG-overlay extension")
//! @yah:status(open)
//! @yah:phase(P5)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)

use crate::backend::{IndexReasonDto, RigBackend, WalkSummaryDto};
use crate::state::{save_rigs_file, AppState, RemoteRigSpec, RigDto, RigId};
use std::path::PathBuf;
use yah_kg::ids::{NodeFull, NodeId};
use yah_kg::kind::Lang;
use yah_kg::rpc::{
    ArchiveTicketParams, ArchiveTicketResult, DirListParams, DirListResult, DirWatchParams,
    FileReadParams, FileReadResult, FileWatchParams, GetTicketParams, GetTicketResult,
    ListAuthoredFilesParams, ListAuthoredFilesResult, ListRelaysParams, ListRelaysResult,
    ListTicketsParams, ListTicketsResult, LookupParams, LookupResult, MoveTicketParams,
    MoveTicketResult, NeighborsParams, NeighborsResult, ReadAuthoredFileParams,
    ReadAuthoredFileResult, RootsParams, RootsResult, StatsResult, Subgraph, SubgraphParams,
    TicketPromptParams, TicketPromptResult, UnwatchParams, UnwatchResult, ValidateParams,
    ValidateResult, WatchResult,
};

/// Resolve a rig's [`RigBackend`] by id. The renderer always knows which
/// rig it's targeting (the rig selector is its source of truth), so every
/// `arch_*` command takes a `rig_id` and looks up the backend explicitly
/// rather than depending on a mutable "active" pointer.
async fn backend_by_id(state: &AppState, id: &RigId) -> Result<RigBackend, String> {
    state
        .backend_for(id)
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

/// Open a rig: for local rigs, boots from snapshot, starts the file
/// watcher, and saves a fresh snapshot. For remote rigs, calls
/// `arch.open_rig` over the SSH session (which lazily opens on first
/// call) — the same lifecycle runs on the remote daemon. Idempotent on
/// both sides. The rig must already have been attached via `rig_attach`
/// or `rig_attach_remote`.
#[tauri::command]
pub async fn arch_open_rig(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<WalkSummaryDto, String> {
    let backend = backend_by_id(&state, &rig_id).await?;
    let path = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} not attached", rig_id.as_str()))?;
    backend.open_rig(path).await
}

/// Stop the named rig's watcher (local) or close the remote daemon's
/// watcher via RPC.
#[tauri::command]
pub async fn arch_close_rig(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<(), String> {
    backend_by_id(&state, &rig_id).await?.close_rig().await
}

#[tauri::command]
pub async fn arch_subgraph(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: SubgraphParams,
) -> Result<Subgraph, String> {
    backend_by_id(&state, &rig_id).await?.subgraph(params).await
}

#[tauri::command]
pub async fn arch_lookup(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: LookupParams,
) -> Result<LookupResult, String> {
    backend_by_id(&state, &rig_id).await?.lookup(params).await
}

#[tauri::command]
pub async fn arch_node(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    id: NodeId,
) -> Result<Option<NodeFull>, String> {
    backend_by_id(&state, &rig_id).await?.node(id).await
}

#[tauri::command]
pub async fn arch_neighbors(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: NeighborsParams,
) -> Result<NeighborsResult, String> {
    backend_by_id(&state, &rig_id).await?.neighbors(params).await
}

#[tauri::command]
pub async fn arch_roots(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: RootsParams,
) -> Result<RootsResult, String> {
    backend_by_id(&state, &rig_id).await?.roots(params).await
}

#[tauri::command]
pub async fn arch_stats(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<StatsResult, String> {
    backend_by_id(&state, &rig_id).await?.stats().await
}

#[tauri::command]
pub async fn arch_languages(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<Vec<Lang>, String> {
    backend_by_id(&state, &rig_id).await?.languages().await
}

/// Enumerate `.mmd` files under `<rig_root>/.yah/arch/authored/`. The
/// renderer's AuthoredFilesPicker calls this when the Arch view's left
/// rail mounts; missing directory comes back as an empty list.
#[tauri::command]
pub async fn arch_list_authored_files(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<ListAuthoredFilesResult, String> {
    backend_by_id(&state, &rig_id)
        .await?
        .list_authored_files(ListAuthoredFilesParams::default())
        .await
}

/// Read one authored `.mmd` file selected via `arch_list_authored_files`.
/// `params.rel_path` is canonicalized inside the daemon and rejected
/// with a `Conflict` (mapped here to a string error) if it escapes the
/// `<rig_root>/.yah/arch/authored/` sandbox.
#[tauri::command]
pub async fn arch_read_authored_file(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: ReadAuthoredFileParams,
) -> Result<ReadAuthoredFileResult, String> {
    backend_by_id(&state, &rig_id)
        .await?
        .read_authored_file(params)
        .await
}

/// Read bytes from a rig-relative path (`file.read`). Sandboxed to the
/// rig root by the daemon — paths that resolve outside are rejected as
/// errors. `params.range` pages a slice; without it the response clips
/// at the 5MB soft cap and sets `truncated`.
#[tauri::command]
pub async fn file_read(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: FileReadParams,
) -> Result<FileReadResult, String> {
    backend_by_id(&state, &rig_id).await?.file_read(params).await
}

/// One-shot directory listing under the rig root (`dir.list`). The
/// daemon canonicalizes `params.path` and rejects anything that escapes
/// the rig. Empty `path` lists the rig root itself.
#[tauri::command]
pub async fn dir_list(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: DirListParams,
) -> Result<DirListResult, String> {
    backend_by_id(&state, &rig_id).await?.dir_list(params).await
}

/// Subscribe to filesystem changes on a single file (`file.watch`).
/// Returns a handle id; the daemon emits `file.event` notifications for
/// every change until [`file_unwatch`] is called or the rig closes.
#[tauri::command]
pub async fn file_watch(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: FileWatchParams,
) -> Result<WatchResult, String> {
    backend_by_id(&state, &rig_id).await?.watch_file(params).await
}

/// Subscribe to recursive filesystem changes under a directory
/// (`dir.watch`). Returns a handle id; semantics otherwise match
/// [`file_watch`].
#[tauri::command]
pub async fn dir_watch(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: DirWatchParams,
) -> Result<WatchResult, String> {
    backend_by_id(&state, &rig_id).await?.watch_dir(params).await
}

/// Drop a watch handle previously returned by [`file_watch`] or
/// [`dir_watch`] (`file.unwatch`). Idempotent — unknown ids are a no-op.
#[tauri::command]
pub async fn file_unwatch(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: UnwatchParams,
) -> Result<UnwatchResult, String> {
    backend_by_id(&state, &rig_id).await?.unwatch(params).await
}

/// Enumerate every synthetic Ticket node currently in the graph. The Board
/// UI calls this instead of file-scanning for `@yah:ticket(...)` directives.
#[tauri::command]
pub async fn arch_list_tickets(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: Option<ListTicketsParams>,
) -> Result<ListTicketsResult, String> {
    backend_by_id(&state, &rig_id)
        .await?
        .list_tickets(params.unwrap_or_default())
        .await
}

/// Same as `arch_list_tickets` but for synthetic Relay nodes — feeds the
/// Epics column and the relay-pickers in the Board UI.
#[tauri::command]
pub async fn arch_list_relays(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: Option<ListRelaysParams>,
) -> Result<ListRelaysResult, String> {
    backend_by_id(&state, &rig_id)
        .await?
        .list_relays(params.unwrap_or_default())
        .await
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
    backend_by_id(&state, &rig_id).await?.get_ticket(params).await
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
    backend_by_id(&state, &rig_id)
        .await?
        .validate(params.unwrap_or_default())
        .await
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
    backend_by_id(&state, &rig_id).await?.ticket_prompt(params).await
}

/// Rewrite a ticket's `@yah:status(...)` line in source to mirror a
/// column drag-and-drop in the Board UI. Validates the same transition
/// matrix the existing `yah board move` CLI enforces (open→active,
/// active→{open,handoff,review}, handoff→{active,review}, review→
/// handoff). The watcher will pick up the rewritten file and the
/// daemon emits a `WorkItemChanged` event with `IndexReason::AgentEdit`.
///
/// Domain rejections (epic mutation, transition not on the matrix,
/// ticket not on the board) surface as `DaemonError::Conflict` and map
/// to a string error here; the renderer should treat these as toast
/// material rather than throw.
#[tauri::command]
pub async fn arch_move_ticket(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    params: MoveTicketParams,
) -> Result<MoveTicketResult, String> {
    backend_by_id(&state, &rig_id).await?.move_ticket(params).await
}

/// Archive a ticket from the review/done bucket. Strips the `@yah:*`
/// annotation lines from source and appends a single `archived` event
/// to the per-relay shard at `.yah/events/<shard>.jsonl` — the audit
/// log alone is enough to rehydrate the ticket later.
///
/// Effects: rewrites the source file (the daemon's `AgentEdit` reindex
/// fires immediately; the renderer sees the ticket vanish on the next
/// index_finished). Errors surface to the renderer as a toast (epic
/// with live children, ticket not in a terminal state).
#[tauri::command]
pub async fn arch_archive_ticket(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    id: String,
) -> Result<ArchiveTicketResult, String> {
    backend_by_id(&state, &rig_id)
        .await?
        .archive_ticket(ArchiveTicketParams { id })
        .await
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
    backend_by_id(&state, &rig_id)
        .await?
        .reindex_path(&path, reason)
        .await
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
    backend_by_id(&state, &rig_id)
        .await?
        .touch(paths, tool, relay)
        .await
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

/// Add a remote (SSH) rig to the registry. Stores the spec only — no
/// SSH session is opened here; the lazy `SshRpcClient` construction
/// and dispatch land with R019-F2/F3, at which point activating the
/// rig will connect for the first time. Idempotent on the spec
/// `(user, host, port, workspace)` — re-attaching with the same tuple
/// returns the existing rig with display name + key path refreshed.
///
/// `name` is optional on the renderer side; the wire payload always
/// carries it (defaulted to `host` by the daemon when blank). Errors
/// other than "rig vanished" can't surface today since attach is
/// pure metadata; that changes once the spec is validated against
/// the SSH transport.
#[tauri::command]
pub async fn rig_attach_remote(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    spec: RemoteRigSpec,
) -> Result<RigDto, String> {
    let id = state.attach_remote_rig(spec, app).await;
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

// `WalkSummaryDto` and `IndexReasonDto` live in `crate::backend` — both
// dispatch arms produce/consume them, so they belong with the
// `RigBackend` enum rather than this command-binding layer.
