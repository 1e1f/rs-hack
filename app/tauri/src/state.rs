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
//! @arch:see(architecture/yah-ui-implementation-guide.md)
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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tokio::sync::RwLock;
use yah_kg_daemon::KgService;
use yah_kg_json_yaml::{JsonIndexer, TomlIndexer, YamlIndexer};
use yah_kg_rust::RustIndexer;
use yah_kg_store::IndexerRegistry;
use yah_kg_ts::TsIndexer;

/// Stable id for a rig — derived from the canonical absolute path so
/// renaming the rig (changing `name`) leaves the id alone, while
/// moving the folder mints a fresh id (which is the right behaviour
/// for a per-folder cache key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RigId(pub String);

impl RigId {
    pub fn from_path(path: &Path) -> Self {
        let canon = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        let s = canon.to_string_lossy();
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigKind {
    Local,
    Remote,
}

/// In-memory entry: persisted metadata + a running daemon + the
/// JoinHandle for the per-rig event-bridge task. Detaching a rig
/// drops the entry, which aborts the bridge and drops the daemon's
/// Arc count to zero (any in-flight watcher task observes the
/// channel close).
pub struct RigEntry {
    pub rig: Rig,
    pub svc: Arc<KgService>,
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

    /// Clone of the active rig's [`KgService`], or `None` if no rig is
    /// attached / activated. Existing `arch_*` commands route through
    /// this for backward compatibility while R024-T2 introduces
    /// explicit `rig_id` parameters.
    pub async fn active_svc(&self) -> Option<Arc<KgService>> {
        let active = self.active.read().await.clone()?;
        let rigs = self.rigs.read().await;
        rigs.get(&active).map(|e| e.svc.clone())
    }

    /// Look up a rig's daemon by id without changing the active pointer.
    pub async fn svc_for(&self, id: &RigId) -> Option<Arc<KgService>> {
        self.rigs.read().await.get(id).map(|e| e.svc.clone())
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
    pub async fn attach_rig(
        &self,
        path: PathBuf,
        name: String,
        app_handle: AppHandle,
    ) -> RigId {
        let id = RigId::from_path(&path);
        let mut rigs = self.rigs.write().await;
        if let Some(existing) = rigs.get_mut(&id) {
            existing.rig.name = name;
            existing.rig.path = path;
            return id;
        }
        let svc = Arc::new(make_kg_service());
        let bridge = crate::event_bridge::spawn_for(id.clone(), svc.clone(), app_handle);
        rigs.insert(
            id.clone(),
            RigEntry {
                rig: Rig {
                    id: id.clone(),
                    name,
                    path,
                    kind: RigKind::Local,
                    last_active_at: None,
                },
                svc,
                bridge: Some(bridge),
            },
        );
        id
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
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("lastActiveAt"), "{json}");
        assert!(json.contains("\"reachable\":true"), "{json}");
        assert!(json.contains("\"kind\":\"local\""), "{json}");
    }
}
