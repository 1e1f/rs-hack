//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Tauri-managed state: a registry of attached rigs (each holding a
//! [`KgService`]) plus a pointer at the currently-active rig.
//!
//! `AppState` is `Clone` because Tauri commands receive `&State<T>`
//! references and we sometimes want to hand the underlying service to
//! a spawned task (the event bridge, the auto-boot loop). The rigs
//! map and active pointer live behind `Arc<RwLock<…>>` so cloning is
//! cheap.
//!
//! ## Multi-rig model (R024-T1)
//!
//! Each [`Rig`] is a folder with `.yah/` + `@yah:` annotations. We
//! keep one [`KgService`] per rig in [`RigEntry`], plus a per-rig
//! event-bridge task that stamps every `ArchEvent` with its origin
//! [`RigId`] before forwarding to Tauri's window event bus.
//!
//! The persisted registry lives at `~/.yah/rigs.json` (override via
//! `YAH_HOME`). On boot, every persisted rig is reattached so the
//! UI's rig selector lights up the same set the user had last
//! session, even before any of them have been booted/indexed.
//!
//! @yah:relay(R024, "Multi-rig Tauri host: registry, attention badges, rig-management commands")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R013)
//! @yah:handoff("Sprint scope expansion to support 3+ local rigs day-one with per-rig attention badges in RigSelector. Sibling to R017 (KG features) and R018 (pi-mono agent). Builds on R016 watcher/event-bridge wiring.")
//! @yah:next("Land T1 (multi-rig AppState) before T2/T3 — both depend on the rig-keyed state shape")
//! @yah:next("T2 may go in parallel with T3 once T1 lands: T2 is Rust-only (Tauri commands), T3 is yah-ui-only (mock-driven)")
//!
//! @yah:ticket(R024-T1, "Multi-rig AppState: HashMap<RigId, KgService> + per-rig watchers + ~/.yah/rigs.json persistence")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R024)
//! @yah:next("Replace AppState.svc: Arc<KgService> with rigs: Arc<RwLock<HashMap<RigId, Arc<KgService>>>> + active: Arc<RwLock<Option<RigId>>>")
//! @yah:next("Each KgService gets its own watcher task; per-rig event channel multiplexed onto a single bridge that tags every ArchEvent with rig_id")
//! @yah:next("Persist rig list to ~/.yah/rigs.json ({rigs: [{id, name, path, kind: 'local'}], lastActive: id}); load on bootstrap()")
//! @yah:verify("cargo build -p yah-tauri")
//! @arch:see(.yah/arch/authored/yah-ui-implementation-guide.md)
//! @yah:handoff("Multi-rig AppState landed. New types in app/tauri/src/state.rs: RigId (blake3 of canonicalized path, 'rig:hex12' form, deterministic), Rig (id/name/path/kind), RigKind (Local/Remote), RigEntry (rig + Arc<KgService> + bridge JoinHandle), RigsFile (rigs[], lastActive, camelCase JSON via serde rename). AppState replaced its single Arc<KgService> with rigs: Arc<RwLock<HashMap<RigId, RigEntry>>> + active: Arc<RwLock<Option<RigId>>>; methods attach_rig (idempotent via path-derived id), detach_rig (aborts bridge + clears active if matching), list_rigs, set_active, active_id, active_svc (compat accessor for arch_* commands). Persistence: rigs_file_path() defaults ~/.yah/rigs.json (override YAH_HOME for tests), load_rigs_file/save_rigs_file. event_bridge::spawn replaced with spawn_for(rig_id, svc, app_handle) — emits a flattened RigEvent { rig_id, ...event } payload via Tauri events so the renderer can route by rig. lib.rs setup() now: AppState::empty → manage → spawn boot_registry which reattaches every persisted rig (no auto-boot, indexing is lazy on user click), restores lastActive, then handles YAH_RIG_ROOT (attach + activate + auto-boot+watch) for dev. commands.rs: every arch_* command routes through active(&state) -> Arc<KgService> which returns 'no active rig — attach one via the rig selector' if none. Tests: 3 unit tests in state::tests (RigId determinism, RigId differs across paths, RigsFile round-trips through camelCase JSON). cargo build -p yah-tauri green; full workspace builds clean (only the pre-existing 42 dead-code warnings on yah Ticket methods).")
//! @yah:next("R024-T2 (Tauri rig-management commands): wire list_rigs/attach_rig/detach_rig/set_active_rig as #[tauri::command]s on top of the AppState methods that now exist; saving the registry should call state.snapshot_to_file().await + save_rigs_file() after each mutation. Also add rig_id: RigId arg to every arch_* command and route via state.svc_for(&id) instead of active(&state).")
//! @yah:next("Renderer (yah-ui) listens to 'arch:event' and now needs to read the rig_id field from the wrapped payload — that's the breaking change for the env adapter when the Tauri target ships.")
//! @yah:gotcha("Spawn handle types: tauri::async_runtime::spawn returns tauri::async_runtime::JoinHandle, NOT tokio::task::JoinHandle. The two have similar names but distinct types — using the wrong import gives an opaque E0308 mismatch at the spawn site. RigEntry::bridge and event_bridge::spawn_for must keep the tauri import.")
//!
//! @yah:ticket(R016-T6, "Wire JsonIndexer/YamlIndexer/TomlIndexer into the daemon's IndexerRegistry")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R016)
//! @yah:next("app/tauri/src/state.rs:331 currently registers only RustIndexer + TsIndexer; add Json/Yaml/Toml so config files appear in the architecture tab at runtime, not just in unit tests.")
//! @yah:next("Mirror the same registration in yah-kg-daemon/tests/e2e.rs to cover the multi-language walk.")
//!
//! @yah:ticket(R019-F3, "RigBackend enum + dispatch layer (Local/Remote)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R019)
//! @yah:next("Replace RigEntry.svc: Arc<KgService> with backend: RigBackend (enum Local/Remote) in app/tauri/src/state.rs")
//! @yah:next("RigBackend exposes the same async surface as KgService — every arch_* command in app/tauri/src/commands.rs dispatches via state.backend_for(&rig_id) instead of svc_by_id")
//! @yah:next("Remote arm = unimplemented!() stub; Local arm keeps existing semantics byte-identical")
//! @yah:next("Boot/attach flow unchanged for Local; remote attach stores spec without connecting (lazy)")
//! @yah:next("Verify: cargo build -p yah-tauri && cargo test -p yah-tauri (every existing arch_* test passes through Local arm)")
//! @arch:see(.yah/arch/authored/rig-backend-dispatch.md)
//! @yah:handoff("RigBackend dispatch landed end-to-end. New module app/tauri/src/backend.rs owns the enum (Local(Arc<KgService>) / Remote(SshRpcClient)) + every method mirroring KgService surface (subgraph, lookup, node, neighbors, roots, stats, languages, list_authored_files, read_authored_file, list_tickets, list_relays, get_ticket, validate, ticket_prompt, move_ticket, reindex_path, touch) + open_rig/close_rig lifecycle. Local arm zero-cost direct calls; Remote arm forwards via yah-rpc-ssh::SshRpcClient (lazy first-call connect, exponential reconnect inside .call()). state.rs: RigEntry.svc: Arc<KgService> -> backend: RigBackend; attach_rig builds Local; attach_remote_rig builds Remote(SshRpcClient::new(SshRpcConfig{...})); svc_for / active_svc kept as local-only accessors (return None on remote) for the agent runtime which still needs direct KgService access. event_bridge.rs: spawn_for now takes RigBackend and branches — Local arm subscribes to KgService broadcast as before; Remote arm calls client.subscribe_events() in an outer loop that resubscribes after session-close (matches KgService's resubscribe-after-restart contract). commands.rs: every arch_* command resolves via backend_by_id (was svc_by_id) and dispatches through RigBackend; arch_open_rig's 'remote not wired yet' early-return is gone — RigBackend::open_rig handles both arms (Local: boot_with_snapshot+start_watching+save_default; Remote: client.open_rig() RPC). WalkSummaryDto and IndexReasonDto moved from commands.rs to backend.rs since both arms produce/consume them. yah-rpc-ssh re-exports OpenRigResult + ReindexReasonWire from lib.rs. cargo build -p yah-tauri green; cargo test -p yah-tauri --lib 27/27 green; cargo test -p yah-rpc-ssh 8/8 green; cargo test -p yah-kg-daemon --lib 2/2 green.")
//! @yah:next("Real-host smoke test: with a daemon installed on a Hetzner box (R019-F6 dev workflow), Connect remote rig... in the UI -> click open. Expect arch.open_rig to fire over SSH, the workspace to walk on the remote, a WalkSummary dto back, and arch:event notifications to start streaming. The sub-tickets at this point all flip to review-archive: F2 (transport), T5 (modal), F3 (this).")
//! @yah:next("Gap: serve.rs dispatch is missing arch.list_authored_files + arch.read_authored_file + arch.archive_ticket. SshRpcClient already has the first two methods on its surface, so on remote rigs they will fail with method-not-found until the daemon side is wired. Add the two trivial cases to yah/src/serve.rs::dispatch (list_authored_files + read_authored_file). archive_ticket is harder — local commands.rs shells out to the yah CLI; for remote it needs a KgService::archive_ticket method (mirror move_ticket pattern). Track as a follow-up sub-ticket of R019 or under R017.")
//! @yah:next("Polish: agent runtime is local-only — agent_start_session etc. call svc_for (returns None for remote rigs). Decide whether remote agents need a separate path (agent process running on the remote box, started via SSH) or whether the local agent calls remote rigs through SshRpcClient for KG queries. The user's mental model in this conversation suggested 'sometimes a local agent, sometimes a remote agent' which lines up with running a separate agent on the remote box. Track as a new ticket under R013/R028.")
//! @yah:next("Polish: Test connection button on Connect-remote-rig modal — modal already has the field set; rpc.rigTestRemote(spec) -> a new Tauri command that constructs an ephemeral SshRpcClient + calls ensure_connected().await + reports back true/error string. Skip until users ask.")
//! @yah:next("UX: local-vs-remote agent visual distinction (icons + names) — the user flagged this at the top of the R019-F3 conversation. Now that remote rigs activate, design the rig-pill / agent-shell affordances. New ticket under R013 (rig UX) or R024 (multi-rig).")
//!
//! @yah:ticket(R033-T17, "RigBackend::Remote arms for file/dir/lsp methods (after R019-F2)")
//! @yah:status(open)
//! @yah:phase(P6)
//! @yah:parent(R033)
//! @arch:see(.yah/arch/authored/yah-files-tab.md)
//! @arch:see(.yah/arch/authored/rig-backend-dispatch.md)
//!
//! @yah:ticket(R033-T18, "e2e: open file on remote rig + rust-analyzer hover over SSH")
//! @yah:status(open)
//! @yah:phase(P6)
//! @yah:parent(R033)
//! @arch:see(.yah/arch/authored/yah-files-tab.md)
//!
//! @yah:relay(R034, "Identity registry: SSH-key first-class object + cross-target authorization")
//! @yah:status(open)
//! @yah:parent(R013)
//! @arch:see(.yah/arch/authored/yah-identities.md)
//!
//! @yah:ticket(R034-T1, "Identity registry foundation: types + identities.json + 4 Tauri commands (no probes)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R034)
//! @arch:see(.yah/arch/authored/yah-identities.md)
//! @yah:handoff("Identity registry foundation landed. New module app/tauri/src/identities.rs owns: Identity (id=SHA256 fingerprint, name, algorithm, public_key, source, authorized_at, created_at, last_used_at), IdentitySource (YahGenerated{private_key_path} | Imported{private_key_path?, public_key_path}), Authorization (Hetzner | Github | Gitlab | SshHost variants — caches with last_seen timestamps; unused at T1, populated by P2 probes), IdentitiesFile, IdentityError. Storage at $YAH_HOME/identities.json (default ~/.yah/identities.json), camelCase + tagged-enum serde matching the architecture doc. yah-managed private keys go under $YAH_HOME/keys/<name> (mode 0600 on Unix; dir 0700). Imported keys are referenced by path only — yah never reads or copies private bytes. Four Tauri commands wired through invoke_handler in lib.rs: identity_list, identity_create(name), identity_import(public_key_path, name?), identity_remove(id). Each command takes a per-process tokio Mutex (IdentitiesState, Tauri-managed) so concurrent invokes don't race the load->mutate->save sequence. De-dup is by fingerprint: re-creating or re-importing the same key refreshes display name/source path on the canonical record without losing authorized_at. Tests: 6/6 in identities::tests — name validator (accept + reject), JSON round-trip with both enum tags asserted, create+remove deletes keyfile, import does not copy private bytes (original file untouched after remove), duplicate import returns canonical record. cargo build -p yah-tauri green; cargo test -p yah-tauri --lib 65/65 green.")
//! @yah:next("T2 (P2 probes): identity_probe_local walks ~/.ssh + $YAH_HOME/keys, fingerprints, merges into the registry; identity_probe_hetzner reuses hetzner_list_ssh_keys + matches fingerprints; identity_probe_github needs a new GET /user/keys client (allowlist 'github' provider in api_keys::validate_provider first).")
//! @yah:next("Probe results update Identity.authorized_at + last_used_at. Missing PAT for forge probes is a no-op (skip silently with a tracing::info that the row will appear unchecked). identity_probe_all fans out the three.")
//! @yah:next("Renderer (R034-F4) can start mock-driven against this surface today: rpc.identity.list/create/import/remove on env/index.ts; tauri.ts invokes the four commands; browser.ts can return a fixed mock list for component inspection.")
//! @yah:next("Open question for the user before P5 lands: should yah-managed keys move into the OS keychain (R027-T7's vault under identity:<fingerprint>) or stay as files at $YAH_HOME/keys/? Doc currently picks files for ssh -i compat; revisit once T1 is in real use.")
//!
//! @yah:ticket(R034-T6, "Migrate rigs.json keyPath \\u2192 identityId via fingerprint match on first identities.json boot")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P5)
//! @yah:parent(R034)
//! @arch:see(.yah/arch/authored/yah-identities.md)

use crate::backend::RigBackend;
use kg_daemon::KgService;
use kg_json_yaml::{JsonIndexer, TomlIndexer, YamlIndexer};
use kg_rust::RustIndexer;
use kg_store::IndexerRegistry;
use kg_ts::TsIndexer;
use rpc_ssh::{SshRpcClient, SshRpcConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tokio::sync::RwLock;

/// Stable id for a rig — derived from the canonical absolute path so
/// renaming the rig (changing `name`) leaves the id alone, while
/// moving the folder mints a fresh id (which is the right behaviour
/// for a per-folder cache key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RigId(pub String);

impl RigId {
    pub fn from_path(path: &Path) -> Self {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let s = canon.to_string_lossy();
        let h = blake3::hash(s.as_bytes());
        let hex = h.to_hex();
        Self(format!("rig:{}", &hex.as_str()[..12]))
    }

    /// Stable id for a remote rig — derived from the spec the user typed
    /// in the Connect modal. Re-attaching the same `(user, host, port,
    /// workspace)` returns the same id even though no canonical local
    /// path exists. Port defaults to 22 in the hash so users who omit
    /// the field don't get a different id from users who type `22`.
    pub fn from_remote(user: &str, host: &str, port: Option<u16>, workspace: &Path) -> Self {
        let p = port.unwrap_or(22);
        let s = format!("ssh://{}@{}:{}{}", user, host, p, workspace.display());
        let h = blake3::hash(s.as_bytes());
        let hex = h.to_hex();
        Self(format!("rig:{}", &hex.as_str()[..12]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Persisted rig metadata. The runtime [`KgService`] is *not* stored
/// here — it gets reconstructed on attach so a freshly-loaded rigs
/// file produces a clean daemon.
///
/// For [`RigKind::Remote`], the SSH spec lives directly on the rig
/// (`host`, `port`, `user`, `key_path`) and `path` holds the *remote*
/// workspace path the daemon will index. This keeps `path_for(...)` /
/// `Rig.path` semantics uniform across local + remote — both answer
/// "the folder this rig points at" — and the renderer's RigSelector
/// can show the remote workspace under the host pill without any
/// kind-specific branching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rig {
    pub id: RigId,
    pub name: String,
    pub path: PathBuf,
    /// `Local` today; `Remote` (SSH-RPC) lands when remote rigs ship.
    pub kind: RigKind,
    /// Unix-ms timestamp of the most recent `set_active`. The rig
    /// selector sorts by this to surface "what was I just on?". `None`
    /// until the user has ever activated this rig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    /// Remote-only: SSH host (DNS or IP). `None` for local rigs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Remote-only: SSH port. `None` means "default 22".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Remote-only: SSH user. Required for remote rigs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Remote-only: path to a private key file. `None` falls back to
    /// SSH agent / `~/.ssh/id_*` defaults at connection time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
    /// Identity-registry id (SHA256 fingerprint) of the keypair this
    /// rig connects with. Populated by the keyPath→identityId migration
    /// (R034-T6) on boot when only `key_path` was set; once populated,
    /// the registry is the canonical source for the public key + auth
    /// state and the renderer renders the rig's identity row from it.
    /// `key_path` stays alongside for one release as a fallback for any
    /// connection path that hasn't been switched over yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigKind {
    Local,
    Remote,
}

/// Wire shape for the renderer's "Connect remote rig…" modal. The
/// payload is stored as-is on the `Rig` (no SSH connection happens
/// here — the spec sits dormant until activation, at which point an
/// `SshRpcClient` is constructed lazily; that piece lands with R019-F2).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRigSpec {
    pub host: String,
    pub user: String,
    pub workspace_path: PathBuf,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub key_path: Option<PathBuf>,
    /// Display name in the rig selector. Defaults to `host` when the
    /// renderer leaves it blank.
    #[serde(default)]
    pub name: Option<String>,
}

/// In-memory entry: persisted metadata + a [`RigBackend`] (an in-process
/// daemon for local rigs, or an `SshRpcClient` for remote rigs) + the
/// JoinHandle for the per-rig event-bridge task. Detaching a rig drops
/// the entry, which aborts the bridge; the Local arm's daemon Arc count
/// drops to zero (watcher observes the channel close), and the Remote
/// arm's `SshRpcClient` drops, killing the `ssh` child via `kill_on_drop`.
pub struct RigEntry {
    pub rig: Rig,
    pub backend: RigBackend,
    pub bridge: Option<JoinHandle<()>>,
}

/// On-disk schema for `~/.yah/rigs.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RigsFile {
    #[serde(default)]
    pub rigs: Vec<Rig>,
    #[serde(default, rename = "lastActive")]
    pub last_active: Option<RigId>,
}

/// Wire shape for the rig selector. Mirrors the renderer's `Rig`
/// interface in `yah-ui/src/types.ts` plus the few fields the UI uses
/// to render attention state and recency.
///
/// @yah:ticket(R024-T3, "Attention badge: per-rig handoff-count surfaced in RigSelector menu")
/// @yah:assignee(agent:claude)
/// @yah:status(review)
/// @yah:parent(R024)
/// @yah:handoff("Selector now lives end-to-end on T2's backend. Pass 1: Rig interface (src/types.ts) gained needsAttention?, path?, lastActiveAt?; RigSelector grew AttentionPill (brass) on menu rows + an oxblood pip on the title-bar dot when any rig has attention. Pass 2 (smoke-test fix): App.tsx no longer hard-codes mockRigs — rigs are state seeded from rpc.rigList() on mount (last-active wins as the initial pick), and the Board fetch effect now keys on rigId so picking a different rig retargets immediately. rpc gained rigSetActive(rigId) (Rpc interface in env/index.ts; tauri.ts invokes 'rig_set_active'; browser.ts returns false). The rigId effect best-effort calls rigSetActive then refetches listTickets/listRelays — failures are swallowed for the mock-id dev path. mockRigs stays as the browser-only seed for component inspection. Verified: cd yah-ui && bun run typecheck (only the pre-existing serve.ts errors under R015-T6); bun run build (1672 modules, 3.67MB); cargo build -p yah-tauri.")
/// @yah:verify("cd yah-ui && bun run typecheck")
/// @yah:verify("cd yah-ui && bun run build")
/// @yah:verify("YAH_RIG_ROOT=$(pwd) cargo run -p yah-tauri  # selector should show the rig's name (not 'synth-engine'); flipping to a mock rig should retarget the Board")
/// @yah:next("Attach-rig flow: 'Open local folder…' / 'Connect remote rig…' menu items still inert — wire to rpc.rigAttach + refresh state.rigs after success")
/// @yah:next("Once tickets carry rig_id from the daemon, replace App.tsx's active-rig-only attribution with a true Object.groupBy(tickets, t => t.rigId)")
/// @yah:next("Col01 smell hits (Active>1, mixed-children zones) should also feed the attention count alongside handoff")
/// @yah:gotcha("yah scanner is Rust-only and reads //! module-level / /// on top-level items only — this ticket lives on RigDto because that's the wire shape the renderer mirrors. The original R024-T3 disappeared because its annotation was placed inside an #[arg(...)] attribute on an enum variant, which the syn-based scanner cannot see (see memory: feedback_annotation_placement).")
/// @yah:gotcha("rigList() is fetched once on mount — runtime attach (when the menu items are wired) needs to setRigs() too, or move that fetch into a refresh callback the attach flow can invoke after success.")
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigDto {
    pub id: RigId,
    pub name: String,
    pub path: PathBuf,
    pub kind: RigKind,
    /// Always `true` for local rigs (the path either resolves or it
    /// doesn't, and we only attach paths the user picked). Remote rigs
    /// (SSH-RPC) will set this from the transport's last heartbeat.
    pub reachable: bool,
    pub last_active_at: Option<i64>,
    /// Remote-only: SSH host (renderer shows this in the rig pill).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Remote-only: SSH port. `None` means default 22.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Remote-only: SSH user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Remote-only: explicit private-key path (when set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
    /// Identity-registry id this rig connects with — see `Rig.identity_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub rigs: Arc<RwLock<HashMap<RigId, RigEntry>>>,
    pub active: Arc<RwLock<Option<RigId>>>,
}

impl AppState {
    /// Construct an empty registry. Rigs are added via [`AppState::attach_rig`]
    /// — typically once at boot per persisted entry, then on demand from
    /// the UI when the user drops a folder onto the rig selector.
    pub fn empty() -> Self {
        Self {
            rigs: Arc::new(RwLock::new(HashMap::new())),
            active: Arc::new(RwLock::new(None)),
        }
    }

    /// Clone of the active rig's local [`KgService`], if any. Returns
    /// `None` for remote rigs (they have no in-process daemon) and when
    /// nothing is active. Used only by the `YAH_RIG_ROOT` auto-boot dev
    /// path; runtime command dispatch goes through [`backend_for`]
    /// instead.
    pub async fn active_svc(&self) -> Option<Arc<KgService>> {
        let active = self.active.read().await.clone()?;
        let rigs = self.rigs.read().await;
        rigs.get(&active).and_then(|e| e.backend.local())
    }

    /// Look up a rig's [`RigBackend`] by id without changing the active
    /// pointer. Every `arch_*` Tauri command resolves the rig this way
    /// and dispatches through the returned backend.
    pub async fn backend_for(&self, id: &RigId) -> Option<RigBackend> {
        self.rigs.read().await.get(id).map(|e| e.backend.clone())
    }

    /// Look up a rig's local [`KgService`] by id. Returns `None` for
    /// remote rigs (which have no in-process daemon) and for unknown
    /// ids. Used by callers that genuinely need direct daemon access —
    /// today, the agent runtime (assemble_prelude / yah-runner session
    /// construction), which doesn't have a remote analogue yet.
    pub async fn svc_for(&self, id: &RigId) -> Option<Arc<KgService>> {
        self.rigs
            .read()
            .await
            .get(id)
            .and_then(|e| e.backend.local())
    }

    /// Look up the on-disk path for a rig — needed by `arch_open_rig`
    /// to call `svc.boot(path)` without making the renderer round-trip
    /// the path it just handed us in `attach_rig`.
    pub async fn path_for(&self, id: &RigId) -> Option<PathBuf> {
        self.rigs.read().await.get(id).map(|e| e.rig.path.clone())
    }

    /// Idempotent: re-attaching the same path returns the existing id
    /// (with name/path metadata refreshed). Otherwise constructs a
    /// fresh [`KgService`], spawns its event bridge, and inserts.
    ///
    /// `app_handle` is needed only on first attach (to spawn the
    /// bridge); re-attach ignores it.
    pub async fn attach_rig(&self, path: PathBuf, name: String, app_handle: AppHandle) -> RigId {
        let id = RigId::from_path(&path);
        let mut rigs = self.rigs.write().await;
        if let Some(existing) = rigs.get_mut(&id) {
            existing.rig.name = name;
            existing.rig.path = path;
            return id;
        }
        let svc = Arc::new(make_kg_service());
        let backend = RigBackend::Local(svc);
        let bridge = crate::event_bridge::spawn_for(id.clone(), backend.clone(), app_handle);
        rigs.insert(
            id.clone(),
            RigEntry {
                rig: Rig {
                    id: id.clone(),
                    name,
                    path,
                    kind: RigKind::Local,
                    last_active_at: None,
                    host: None,
                    port: None,
                    user: None,
                    key_path: None,
                    identity_id: None,
                },
                backend,
                bridge: Some(bridge),
            },
        );
        id
    }

    /// Idempotent attach for a remote rig. Stores the SSH spec and
    /// constructs a lazy [`SshRpcClient`] — no SSH connection is opened
    /// here; the client connects on the first call (typically the
    /// `arch_open_rig` triggered when the user activates the rig).
    pub async fn attach_remote_rig(&self, spec: RemoteRigSpec, app_handle: AppHandle) -> RigId {
        let id = RigId::from_remote(&spec.user, &spec.host, spec.port, &spec.workspace_path);
        let display_name = spec.name.clone().unwrap_or_else(|| spec.host.clone());
        let mut rigs = self.rigs.write().await;
        if let Some(existing) = rigs.get_mut(&id) {
            existing.rig.name = display_name;
            existing.rig.path = spec.workspace_path.clone();
            existing.rig.host = Some(spec.host.clone());
            existing.rig.user = Some(spec.user.clone());
            existing.rig.port = spec.port;
            existing.rig.key_path = spec.key_path.clone();
            return id;
        }
        let cfg = SshRpcConfig {
            host: spec.host.clone(),
            user: spec.user.clone(),
            port: spec.port,
            key_path: spec.key_path.clone(),
            remote_workspace: spec.workspace_path.clone(),
            remote_yah_bin: None,
            extra_ssh_args: vec![],
        };
        let client = SshRpcClient::new(cfg);
        let backend = RigBackend::Remote(client);
        let bridge = crate::event_bridge::spawn_for(id.clone(), backend.clone(), app_handle);
        rigs.insert(
            id.clone(),
            RigEntry {
                rig: Rig {
                    id: id.clone(),
                    name: display_name,
                    path: spec.workspace_path,
                    kind: RigKind::Remote,
                    last_active_at: None,
                    host: Some(spec.host),
                    port: spec.port,
                    user: Some(spec.user),
                    key_path: spec.key_path,
                    identity_id: None,
                },
                backend,
                bridge: Some(bridge),
            },
        );
        id
    }

    /// Look up the kind of a rig — used by `arch_open_rig` to refuse
    /// remote rigs until `RigBackend` dispatch (R019-F3) lands.
    pub async fn kind_for(&self, id: &RigId) -> Option<RigKind> {
        self.rigs.read().await.get(id).map(|e| e.rig.kind)
    }

    /// Remove a rig: aborts its bridge, drops its daemon, clears the
    /// active pointer if it pointed at this rig. Returns whether the
    /// rig existed.
    pub async fn detach_rig(&self, id: &RigId) -> bool {
        let mut rigs = self.rigs.write().await;
        let Some(mut entry) = rigs.remove(id) else {
            return false;
        };
        if let Some(handle) = entry.bridge.take() {
            handle.abort();
        }
        drop(rigs);
        let mut active = self.active.write().await;
        if active.as_ref() == Some(id) {
            *active = None;
        }
        true
    }

    /// Snapshot of every attached rig's metadata. Order is unspecified
    /// (HashMap iteration); the UI sorts by `name` if it wants stable
    /// presentation.
    pub async fn list_rigs(&self) -> Vec<Rig> {
        self.rigs
            .read()
            .await
            .values()
            .map(|e| e.rig.clone())
            .collect()
    }

    /// Same as [`list_rigs`] but as the wire DTO — fills in `reachable`
    /// (always `true` for local rigs today) for the rig selector.
    pub async fn list_rig_dtos(&self) -> Vec<RigDto> {
        self.rigs
            .read()
            .await
            .values()
            .map(rig_dto_from_entry)
            .collect()
    }

    pub async fn rig_dto_for(&self, id: &RigId) -> Option<RigDto> {
        self.rigs.read().await.get(id).map(rig_dto_from_entry)
    }

    /// Set the active rig. Stamps `lastActiveAt` on the rig so the
    /// selector can sort by recency. Returns `false` if the id isn't
    /// attached.
    pub async fn set_active(&self, id: RigId) -> bool {
        let mut rigs = self.rigs.write().await;
        let Some(entry) = rigs.get_mut(&id) else {
            return false;
        };
        entry.rig.last_active_at = Some(now_ms());
        drop(rigs);
        *self.active.write().await = Some(id);
        true
    }

    pub async fn active_id(&self) -> Option<RigId> {
        self.active.read().await.clone()
    }

    /// Snapshot the registry into the on-disk schema.
    pub async fn snapshot_to_file(&self) -> RigsFile {
        RigsFile {
            rigs: self.list_rigs().await,
            last_active: self.active_id().await,
        }
    }
}

fn rig_dto_from_entry(entry: &RigEntry) -> RigDto {
    RigDto {
        id: entry.rig.id.clone(),
        name: entry.rig.name.clone(),
        path: entry.rig.path.clone(),
        kind: entry.rig.kind,
        // Local paths the user picked are reachable by definition; the
        // remote-rig branch will derive this from the SSH-RPC heartbeat.
        reachable: matches!(entry.rig.kind, RigKind::Local),
        last_active_at: entry.rig.last_active_at,
        host: entry.rig.host.clone(),
        port: entry.rig.port,
        user: entry.rig.user.clone(),
        key_path: entry.rig.key_path.clone(),
        identity_id: entry.rig.identity_id.clone(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn make_kg_service() -> KgService {
    let mut registry = IndexerRegistry::new();
    registry.register(Box::new(RustIndexer::new()));
    registry.register(Box::new(TsIndexer::new()));
    registry.register(Box::new(JsonIndexer::new()));
    registry.register(Box::new(YamlIndexer::new()));
    registry.register(Box::new(TomlIndexer::new()));
    KgService::new(registry)
}

/// Path for the rigs registry. Defaults to `~/.yah/rigs.json` — the
/// same dir the board server uses for events. `YAH_HOME` overrides
/// the parent (useful in tests).
pub fn rigs_file_path() -> PathBuf {
    if let Ok(home) = std::env::var("YAH_HOME") {
        return PathBuf::from(home).join("rigs.json");
    }
    let dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".yah"))
        .unwrap_or_else(|_| PathBuf::from(".yah"));
    dir.join("rigs.json")
}

/// Read the persisted rig registry. Missing or malformed file →
/// `RigsFile::default()` (an empty registry); a corrupt rigs file
/// shouldn't deadlock the app.
pub fn load_rigs_file() -> RigsFile {
    let p = rigs_file_path();
    let Ok(bytes) = std::fs::read(&p) else {
        return RigsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|err| {
        tracing::warn!(error = %err, path = %p.display(), "rigs.json malformed; ignoring");
        RigsFile::default()
    })
}

pub fn save_rigs_file(file: &RigsFile) -> std::io::Result<()> {
    let p = rigs_file_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes =
        serde_json::to_vec_pretty(file).expect("RigsFile serializes — only string + enum fields");
    std::fs::write(&p, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_id_is_deterministic_for_same_path() {
        let p = PathBuf::from("/tmp");
        let a = RigId::from_path(&p);
        let b = RigId::from_path(&p);
        assert_eq!(a, b);
        assert!(a.as_str().starts_with("rig:"));
    }

    #[test]
    fn rig_id_differs_for_different_paths() {
        let a = RigId::from_path(&PathBuf::from("/tmp"));
        let b = RigId::from_path(&PathBuf::from("/usr"));
        assert_ne!(a, b);
    }

    #[test]
    fn rigs_file_round_trips_through_serde() {
        let f = RigsFile {
            rigs: vec![Rig {
                id: RigId("rig:abc123".into()),
                name: "rs-hack".into(),
                path: PathBuf::from("/tmp/rs-hack"),
                kind: RigKind::Local,
                last_active_at: Some(1_700_000_000_000),
                host: None,
                port: None,
                user: None,
                key_path: None,
                identity_id: None,
            }],
            last_active: Some(RigId("rig:abc123".into())),
        };
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("lastActive"), "camelCase last_active: {json}");
        assert!(
            json.contains("lastActiveAt"),
            "Rig.last_active_at camelCases: {json}"
        );
        let back: RigsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rigs.len(), 1);
        assert_eq!(back.last_active.unwrap().as_str(), "rig:abc123");
        assert_eq!(back.rigs[0].last_active_at, Some(1_700_000_000_000));
    }

    #[test]
    fn rig_dto_serializes_camel_case() {
        let dto = RigDto {
            id: RigId("rig:abc123".into()),
            name: "rs-hack".into(),
            path: PathBuf::from("/tmp/rs-hack"),
            kind: RigKind::Local,
            reachable: true,
            last_active_at: Some(1_700_000_000_000),
            host: None,
            port: None,
            user: None,
            key_path: None,
            identity_id: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("lastActiveAt"), "{json}");
        assert!(json.contains("\"reachable\":true"), "{json}");
        assert!(json.contains("\"kind\":\"local\""), "{json}");
        // Remote-only fields are skipped when absent so local rigs
        // don't carry dead `host: null` keys around.
        assert!(!json.contains("host"), "{json}");
    }

    #[test]
    fn rig_id_from_remote_is_deterministic() {
        let a = RigId::from_remote(
            "agent",
            "box.example.com",
            Some(2222),
            &PathBuf::from("/srv/code"),
        );
        let b = RigId::from_remote(
            "agent",
            "box.example.com",
            Some(2222),
            &PathBuf::from("/srv/code"),
        );
        assert_eq!(a, b);
        assert!(a.as_str().starts_with("rig:"));
    }

    #[test]
    fn rig_id_from_remote_treats_port_default_as_22() {
        // Users who type nothing in the port field shouldn't get a
        // different id from users who type "22".
        let omitted = RigId::from_remote(
            "agent",
            "box.example.com",
            None,
            &PathBuf::from("/srv/code"),
        );
        let explicit = RigId::from_remote(
            "agent",
            "box.example.com",
            Some(22),
            &PathBuf::from("/srv/code"),
        );
        assert_eq!(omitted, explicit);
    }

    #[test]
    fn rig_id_from_remote_differs_across_specs() {
        let base = RigId::from_remote("a", "h", Some(22), &PathBuf::from("/p"));
        let other_user = RigId::from_remote("b", "h", Some(22), &PathBuf::from("/p"));
        let other_host = RigId::from_remote("a", "g", Some(22), &PathBuf::from("/p"));
        let other_port = RigId::from_remote("a", "h", Some(2200), &PathBuf::from("/p"));
        let other_path = RigId::from_remote("a", "h", Some(22), &PathBuf::from("/q"));
        assert_ne!(base, other_user);
        assert_ne!(base, other_host);
        assert_ne!(base, other_port);
        assert_ne!(base, other_path);
    }
}
