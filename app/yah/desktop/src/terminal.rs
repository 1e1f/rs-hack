//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! SSH-backed terminal session manager. Bridges russh PTY channels to
//! the renderer's xterm.js panes via Tauri events. The renderer never
//! sees SSH keys or raw socket bytes — it just spawns a session by
//! `(host, user, key_path)`, writes keystrokes through `terminal_input`,
//! and consumes `terminal:event` payloads carrying base64-encoded
//! stdout chunks.
//!
//! Sessions live in process-wide state (separate from `AgentSessions`
//! and the rig registry) because SSH connections span rig lifetimes —
//! flipping rigs in the UI shouldn't tear down an open shell.
//!
//! Event payloads (`terminal:event`):
//! * `{ session_id, kind: "ready" }` — auth succeeded, PTY allocated
//! * `{ session_id, kind: "host_key", fingerprint }` — TOFU hook
//! * `{ session_id, kind: "data", bytes_b64 }` — stdout/stderr chunk
//! * `{ session_id, kind: "closed", reason }` — channel closed normally
//! * `{ session_id, kind: "error", message }` — fatal error
//!
//! Host-key trust: v1 accepts any server key (TOFU disabled). The
//! `check_server_key` callback emits a `host_key` event with the
//! fingerprint so a future TOFU UI can hook in without changing the
//! contract.
//!
//! @yah:relay(R030, "SSH-backed Terminal tab: russh client + xterm.js + cmd-click link provider")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R013)
//! @yah:next("T5 (defer) session persistence + reconnect — write summaries to KV on close, rehydrate panes (without auto-reconnect) on next launch")
//! @yah:gotcha("yah-kg-daemon is currently uncompilable due to in-flight R017-T9 (snapshot string-interning) work — `cargo build -p yah-tauri` fails on the daemon's snapshot_wire.rs missing-arg errors. R030's own crate (yah-tauri) compiles in isolation; the verify gate clears once R017-T9 lands.")
//!
//! @yah:ticket(R030-T1, "Rust russh PTY session manager + Tauri commands + host-key trust")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R030)
//! @yah:handoff("russh-backed SSH terminal session manager landed at app/tauri/src/terminal.rs. Deps: russh 0.51 (pure-rust, openssl-free, brings own rustls/ed25519/aes-gcm), base64 0.22 (encodes PTY bytes for the Tauri event seam — Vec<u8> in serde_json blows up to a number-per-byte array), async-trait 0.1 (russh client::Handler). Process-wide Arc<TerminalSessions> registry registered via app.manage in lib.rs; sessions span rig lifetimes (separate from AgentSessions/AppState rationale). Session lifecycle: terminal_open_ssh(spec) mints a session id synchronously, spawns a tokio task that connects, authenticates (publickey via load_secret_key + PrivateKeyWithHashAlg with best_supported_rsa_hash), allocates xterm-256color PTY (request_pty + request_shell), then enters a select! loop pumping channel.wait() messages → `terminal:event {kind:'data', bytes_b64}` and ControlMsg::{Input,Resize,Close} → channel.data/window_change/close. Eof/Close/transport-drop emit `closed`; auth or transport errors emit `error` then `closed`. TOFU: check_server_key accepts any but emits a `host_key {fingerprint}` event so a future trust prompt can plug in without contract changes. Key resolution: explicit keyPath (with ~ expansion) or discover ~/.ssh/id_ed25519 → id_ecdsa → id_rsa. ControlMsg channel depth 1024 with try_send so a frozen session can't block the IPC. Five Tauri commands registered in lib.rs invoke_handler!: terminal_open_ssh, terminal_input, terminal_resize, terminal_close, terminal_list_sessions. yah-kg-daemon is currently broken on R017-T9's in-flight refactor — yah-tauri compiles in isolation; verify the build once T9 lands.")
//! @yah:verify("cargo build -p yah-tauri  # blocked by R017-T9 pre-existing daemon breakage")
//! @yah:verify("cargo test -p yah-tauri --lib  # blocked by same")
//!
//! @yah:ticket(R030-T2, "yah-ui TerminalView with xterm.js (WebGL renderer + scrollback + search)")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R030)
//! @yah:handoff("xterm.js Terminal pane landed. New deps: @xterm/xterm 6.0, @xterm/addon-fit, @xterm/addon-webgl, @xterm/addon-search, @xterm/addon-web-links, @xterm/addon-unicode11. Module-level singleton at yah-ui/src/components/terminal/terminal-store.ts holds the live xterm Terminal instances keyed by session_id — Terminals outlive React mounts so scrollback survives tab flips and session switches. The store owns one terminal:event subscription that routes by session_id. Each Terminal: 10k-line scrollback, JetBrains Mono 13px, blinking block cursor, macOptionIsMeta=true (matches Terminal.app for readline word jumps over SSH), unicode 11 width, ink/paper color theme. Keystroke pump: term.onData → UTF-8 encode → base64 → terminal_input invoke (chunked btoa for paste safety). Resize pump: term.onResize → terminal_resize invoke. Bytes from Tauri are atob → Uint8Array → term.write so xterm's parser sees raw VT100 instead of decoded strings. TerminalView (yah-ui/src/components/terminal/TerminalView.tsx) renders a left session rail + a single host pane bound via term.open(host); ResizeObserver triggers fit.fit() on bounding-box changes. Detach on unmount keeps the Terminal warm; re-attach on mount calls term.open(host) again. Wired into App.tsx's TabPane case 'terminal' replacing the ComingSoon splash. bun run typecheck + bun run build green; yah-ui bundle grew to 4.19MB (xterm + WebGL + addons).")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")
//! @yah:gotcha("Didn't load the WebGL addon yet — xterm 6 + WebglAddon path requires careful ordering after term.open() and we'll add it in T5 when we tune perf. Canvas renderer is the current default; fine for typical workloads.")
//!
//! @yah:ticket(R030-T3, "HetznerServerList 'Open in terminal' button + tab nav")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R030)
//! @yah:handoff("Per-row '▸ ssh' button on HetznerServerList (yah-ui/src/components/infra/HetznerServerList.tsx) shown only when server.status === 'running' and server.ipv4 is present. New onOpenTerminal prop threads through HetznerServerList → InfraView → InfraTab → App.tsx's TabPane. App.tsx hosts openTerminalForServer callback: terminalStore.open({ host: server.ipv4, user: 'root', label: '<name> (<ipv4>)' }) then setTab('terminal'). Tauri side discovers the SSH key via ~/.ssh/id_ed25519 → ed25519 → ecdsa → rsa fallback (matches what R029's Generate-yah-key flow writes). Per-server 'use this key' picker is deferred until an operator has multiple keypairs in flight.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//!
//! @yah:ticket(R030-T4, "cmd-click link provider — file path patterns route to existing jumpToFile")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R030)
//! @yah:handoff("xterm.js registerLinkProvider on every Terminal in the store. matchFilePaths(text) regex catches both `path/with/slashes(.ext)?(:line(:col)?)?` and bare-basename `name.ext(:line(:col)?)?` against a curated SOURCE_EXTS list (rs/ts/tsx/js/py/go/etc). activate() gates on event.metaKey || event.ctrlKey so plain clicks don't fight cursor selection — matches VS Code's follow-link convention. Routes through terminalStore.getLinkHandler() which TerminalView populates from App's existing onJumpToFile (re-roots ArchView on the file's basename today; ready for a future Files tab). Heuristic, not authoritative: false positives are possible on word-like segments but the modifier-required gesture keeps that harmless.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:gotcha("Path resolution uses just the basename → ArchView re-root. For SSH sessions to non-rig hosts, paths in remote shell output don't map to local files — the click silently does nothing useful. We'll revisit once a proper Files tab + remote-rig mirror lookup land.")
//!
//! @yah:ticket(R030-T5, "Per-server key resolution: kill ~/.ssh/id_* fallback, match Hetzner ssh_keys[] to yah-owned keys")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R030)
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:handoff("Backend (terminal.rs): keyPath now required in TerminalOpenSpec (no Option). Dropped discover_default_key + try_agent_auth + NoKey error; added KeyPathRequired/KeyMissing; EncryptedKey message points at deferred passphrase prompt instead of ssh-add. Renderer (App.openTerminalForServer): async, parallel-fetches ssh.listLocal() + hetzner.listSshKeys(), intersects by canonical public-key string (algo + base64 prefix, comment dropped) — fingerprint comparison was wrong because ssh-key crate emits SHA256:… while Hetzner returns MD5 colon-hex. Prefers yah-generated keys (comment ends @yah); derives private path from public_key_path minus .pub; only flips tab on successful resolve. types.ts: keyPath required.")
//! @yah:gotcha("Auth path will only see keys yah uploaded to Hetzner — id_rsa and other ~/.ssh/id_* keys are intentionally invisible. If a yah-managed key ever gains a passphrase, it surfaces as TerminalError::EncryptedKey; the in-renderer prompt for that case is deferred (still need #3 from the original triage).")

use base64::Engine;
use russh::client::{self, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, Mutex};

const TERMINAL_EVENT: &str = "terminal:event";
const DEFAULT_PORT: u16 = 22;
const DEFAULT_USER: &str = "root";
const DEFAULT_TERM: &str = "xterm-256color";
const DEFAULT_COLS: u32 = 120;
const DEFAULT_ROWS: u32 = 32;
/* Bounded so a runaway producer can't pile up unbounded keystrokes if
the SSH side is blocked. 1024 is generous for typing; long pastes
chunk through fine. */
const CONTROL_CHANNEL_DEPTH: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("session {0} not found")]
    NotFound(String),
    #[error("home directory not resolvable; set $HOME")]
    NoHomeDir,
    #[error(
        "terminal_open_ssh requires keyPath — yah will not auto-discover keys it didn't create"
    )]
    KeyPathRequired,
    #[error("key file not found: {0}")]
    KeyMissing(String),
    #[error(
        "key at {path} is passphrase-protected. Passphrase prompt isn't wired yet — \
         remove the passphrase or use a yah-generated key (which has none) for now."
    )]
    EncryptedKey { path: String },
    #[error("ssh transport error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("ssh key error: {0}")]
    Key(#[from] russh::keys::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("authentication failed for {user}@{host} using {key_path}")]
    AuthFailed {
        user: String,
        host: String,
        key_path: String,
    },
}

/// Renderer-facing spec for `terminal_open_ssh`. `key_path` is
/// **required** — yah only authenticates with keys the operator
/// explicitly authorized through the renderer (typically a
/// yah-generated key whose public half was uploaded to the cloud
/// provider during provisioning). The Tauri side never walks
/// `~/.ssh/id_*` and never speaks to ssh-agent; both would let a
/// stranger key (or a passphrase-protected `id_rsa`) slip into the
/// auth attempt against a server yah didn't authorize it for.
/// May be absolute or `~`-relative.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenSpec {
    pub host: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    pub key_path: String,
    /// Initial PTY dimensions. Renderer measures the xterm grid before
    /// invoking so the first prompt renders correctly; subsequent
    /// resizes flow through `terminal_resize`.
    #[serde(default)]
    pub cols: Option<u32>,
    #[serde(default)]
    pub rows: Option<u32>,
    /// Optional human label; defaults to `user@host`. Used by the
    /// session rail in the UI; never sent to sshd.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionSummary {
    pub session_id: String,
    pub host: String,
    pub user: String,
    pub label: String,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TerminalEvent<'a> {
    Ready {
        session_id: &'a str,
    },
    HostKey {
        session_id: &'a str,
        fingerprint: String,
    },
    Data {
        session_id: &'a str,
        bytes_b64: String,
    },
    Closed {
        session_id: &'a str,
        reason: &'a str,
    },
    Error {
        session_id: &'a str,
        message: String,
    },
}

#[derive(Debug)]
enum ControlMsg {
    Input(Vec<u8>),
    Resize(u32, u32),
    Close,
}

struct SessionEntry {
    summary: TerminalSessionSummary,
    control_tx: mpsc::Sender<ControlMsg>,
}

#[derive(Default)]
pub struct TerminalSessions {
    inner: Mutex<HashMap<String, SessionEntry>>,
}

impl TerminalSessions {
    pub fn new() -> Self {
        Self::default()
    }

    async fn insert(&self, id: String, entry: SessionEntry) {
        self.inner.lock().await.insert(id, entry);
    }

    async fn get_control_tx(&self, id: &str) -> Option<mpsc::Sender<ControlMsg>> {
        self.inner
            .lock()
            .await
            .get(id)
            .map(|e| e.control_tx.clone())
    }

    async fn remove(&self, id: &str) -> bool {
        self.inner.lock().await.remove(id).is_some()
    }

    async fn summaries(&self) -> Vec<TerminalSessionSummary> {
        self.inner
            .lock()
            .await
            .values()
            .map(|e| e.summary.clone())
            .collect()
    }
}

struct ClientHandler {
    app_handle: AppHandle,
    session_id: String,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    /* TOFU placeholder. v1 accepts any server key but emits a
    host_key event so a future trust prompt can land without a
    contract change. */
    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = server_public_key
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        let _ = self.app_handle.emit(
            TERMINAL_EVENT,
            TerminalEvent::HostKey {
                session_id: &self.session_id,
                fingerprint,
            },
        );
        Ok(true)
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn mint_session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("term-{}-{}", now_ms(), n)
}

fn home_dir() -> Result<PathBuf, TerminalError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(TerminalError::NoHomeDir)
}

fn expand_tilde(path: &str) -> Result<PathBuf, TerminalError> {
    if let Some(rest) = path.strip_prefix("~/") {
        Ok(home_dir()?.join(rest))
    } else if path == "~" {
        home_dir()
    } else {
        Ok(PathBuf::from(path))
    }
}

fn resolve_key_path(spec: &TerminalOpenSpec) -> Result<PathBuf, TerminalError> {
    if spec.key_path.is_empty() {
        return Err(TerminalError::KeyPathRequired);
    }
    let path = expand_tilde(&spec.key_path)?;
    if !path.is_file() {
        return Err(TerminalError::KeyMissing(path.display().to_string()));
    }
    Ok(path)
}

// File-only auth. ssh-agent is intentionally not consulted: the
// renderer is expected to pass the exact yah-authorized key for the
// target server (cross-referenced from Hetzner project keys to
// local ~/.ssh entries by fingerprint). Trying every agent identity
// would risk auth attempts with stranger keys against a server that
// only authorized a yah-managed one. Encrypted yah keys are not
// currently supported — yah-generated keys have no passphrase, and
// the in-renderer prompt for imported keys is deferred.
async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    key_path: &PathBuf,
    host: &str,
) -> Result<(), TerminalError> {
    let key_pair = match load_secret_key(key_path, None) {
        Ok(kp) => kp,
        Err(russh::keys::Error::KeyIsEncrypted) => {
            return Err(TerminalError::EncryptedKey {
                path: key_path.display().to_string(),
            });
        }
        Err(e) => return Err(e.into()),
    };

    let hash = handle.best_supported_rsa_hash().await?.flatten();
    let auth_res = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash))
        .await?;
    if !auth_res.success() {
        return Err(TerminalError::AuthFailed {
            user: user.to_string(),
            host: host.to_string(),
            key_path: key_path.display().to_string(),
        });
    }
    Ok(())
}

/* Drive one SSH session end-to-end on its own task. Owns the channel,
so all reads/writes/resizes happen here — the registry only holds
the control sender and metadata. Emits `ready` once the shell is
allocated, then pumps PTY output to the renderer as base64-encoded
`data` events until the channel closes or `Close` arrives. */
async fn run_session(
    app_handle: AppHandle,
    session_id: String,
    user: String,
    addr: (String, u16),
    key_path: PathBuf,
    cols: u32,
    rows: u32,
    mut control_rx: mpsc::Receiver<ControlMsg>,
) {
    let result = run_session_inner(
        &app_handle,
        &session_id,
        &user,
        &addr,
        &key_path,
        cols,
        rows,
        &mut control_rx,
    )
    .await;
    if let Err(e) = result {
        let _ = app_handle.emit(
            TERMINAL_EVENT,
            TerminalEvent::Error {
                session_id: &session_id,
                message: e.to_string(),
            },
        );
        let _ = app_handle.emit(
            TERMINAL_EVENT,
            TerminalEvent::Closed {
                session_id: &session_id,
                reason: "error",
            },
        );
    }
    /* Whether the session ended cleanly or with an error, drop it from
    the registry so a subsequent `terminal_close` doesn't see a
    phantom entry. */
    if let Some(state) = app_handle.try_state::<Arc<TerminalSessions>>() {
        state.remove(&session_id).await;
    }
}

async fn run_session_inner(
    app_handle: &AppHandle,
    session_id: &str,
    user: &str,
    addr: &(String, u16),
    key_path: &PathBuf,
    cols: u32,
    rows: u32,
    control_rx: &mut mpsc::Receiver<ControlMsg>,
) -> Result<(), TerminalError> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(600)),
        ..<_>::default()
    });
    let handler = ClientHandler {
        app_handle: app_handle.clone(),
        session_id: session_id.to_string(),
    };
    let mut handle: Handle<ClientHandler> =
        client::connect(config, (addr.0.as_str(), addr.1), handler).await?;

    authenticate(&mut handle, user, key_path, &addr.0).await?;

    let mut channel = handle.channel_open_session().await?;
    channel
        .request_pty(false, DEFAULT_TERM, cols, rows, 0, 0, &[])
        .await?;
    channel.request_shell(false).await?;

    let _ = app_handle.emit(TERMINAL_EVENT, TerminalEvent::Ready { session_id });

    let b64 = base64::engine::general_purpose::STANDARD;
    let closed_reason: &str;

    loop {
        tokio::select! {
            biased;
            ctl = control_rx.recv() => match ctl {
                Some(ControlMsg::Input(bytes)) => {
                    if let Err(e) = channel.data(&bytes[..]).await {
                        return Err(e.into());
                    }
                }
                Some(ControlMsg::Resize(c, r)) => {
                    let _ = channel.window_change(c, r, 0, 0).await;
                }
                Some(ControlMsg::Close) | None => {
                    closed_reason = "client";
                    break;
                }
            },
            msg = channel.wait() => match msg {
                Some(ChannelMsg::Data { data }) => {
                    let payload = TerminalEvent::Data {
                        session_id,
                        bytes_b64: b64.encode(&data),
                    };
                    let _ = app_handle.emit(TERMINAL_EVENT, payload);
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    /* sshd routes stderr over ext channel 1; xterm
                       wants it interleaved with stdout, so we forward
                       it as a Data event too. */
                    let payload = TerminalEvent::Data {
                        session_id,
                        bytes_b64: b64.encode(&data),
                    };
                    let _ = app_handle.emit(TERMINAL_EVENT, payload);
                }
                Some(ChannelMsg::ExitStatus { .. }) => {
                    /* Don't break here — the server may still flush
                       output before sending Eof/Close. The wait loop
                       exits naturally on those. */
                }
                Some(ChannelMsg::Eof) => {
                    closed_reason = "eof";
                    break;
                }
                Some(ChannelMsg::Close) => {
                    closed_reason = "close";
                    break;
                }
                Some(_) => {}
                None => {
                    closed_reason = "transport";
                    break;
                }
            },
        }
    }

    let _ = channel.close().await;
    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;

    let _ = app_handle.emit(
        TERMINAL_EVENT,
        TerminalEvent::Closed {
            session_id,
            reason: closed_reason,
        },
    );
    Ok(())
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn terminal_open_ssh(
    spec: TerminalOpenSpec,
    sessions: State<'_, Arc<TerminalSessions>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let user = spec
        .user
        .clone()
        .unwrap_or_else(|| DEFAULT_USER.to_string());
    let port = spec.port.unwrap_or(DEFAULT_PORT);
    let cols = spec.cols.unwrap_or(DEFAULT_COLS);
    let rows = spec.rows.unwrap_or(DEFAULT_ROWS);
    let host = spec.host.clone();
    let label = spec
        .label
        .clone()
        .unwrap_or_else(|| format!("{}@{}", user, host));

    let key_path = resolve_key_path(&spec).map_err(|e| e.to_string())?;
    let session_id = mint_session_id();

    let (control_tx, control_rx) = mpsc::channel::<ControlMsg>(CONTROL_CHANNEL_DEPTH);
    let summary = TerminalSessionSummary {
        session_id: session_id.clone(),
        host: host.clone(),
        user: user.clone(),
        label,
        created_at_ms: now_ms(),
    };
    sessions
        .insert(
            session_id.clone(),
            SessionEntry {
                summary,
                control_tx,
            },
        )
        .await;

    let app = app_handle.clone();
    let sid = session_id.clone();
    tauri::async_runtime::spawn(async move {
        run_session(
            app,
            sid,
            user,
            (host, port),
            key_path,
            cols,
            rows,
            control_rx,
        )
        .await;
    });

    Ok(session_id)
}

#[tauri::command]
pub async fn terminal_input(
    session_id: String,
    bytes_b64: String,
    sessions: State<'_, Arc<TerminalSessions>>,
) -> Result<(), String> {
    let tx = sessions
        .get_control_tx(&session_id)
        .await
        .ok_or_else(|| TerminalError::NotFound(session_id.clone()).to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_b64.as_bytes())
        .map_err(|e| format!("invalid base64: {e}"))?;
    /* Drop the keystroke if the per-session channel is full — better
    than blocking the IPC for seconds while a misbehaving session
    drains, and a dropped char on a frozen shell is recoverable. */
    let _ = tx.try_send(ControlMsg::Input(bytes));
    Ok(())
}

#[tauri::command]
pub async fn terminal_resize(
    session_id: String,
    cols: u32,
    rows: u32,
    sessions: State<'_, Arc<TerminalSessions>>,
) -> Result<(), String> {
    let tx = sessions
        .get_control_tx(&session_id)
        .await
        .ok_or_else(|| TerminalError::NotFound(session_id.clone()).to_string())?;
    let _ = tx.try_send(ControlMsg::Resize(cols, rows));
    Ok(())
}

#[tauri::command]
pub async fn terminal_close(
    session_id: String,
    sessions: State<'_, Arc<TerminalSessions>>,
) -> Result<bool, String> {
    if let Some(tx) = sessions.get_control_tx(&session_id).await {
        let _ = tx.send(ControlMsg::Close).await;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn terminal_list_sessions(
    sessions: State<'_, Arc<TerminalSessions>>,
) -> Result<Vec<TerminalSessionSummary>, String> {
    Ok(sessions.summaries().await)
}

// ---------- local PTY (renderer-isolation diagnostic) ----------
//
// Spawns the user's shell ($SHELL or /bin/bash) inside a local PTY and
// pumps bytes through the same `terminal:event` stream as the SSH path.
// Lets us isolate xterm.js renderer issues from remote-side quirks
// (MOTDs, alt-charset bleed, prompt scripts) — if the local terminal
// shows the same artifact, the bug is in the renderer / IPC seam, not
// the SSH transport.

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenLocalSpec {
    /// Override the shell binary. Defaults to `$SHELL`, then `/bin/bash`.
    #[serde(default)]
    pub shell: Option<String>,
    /// Working directory for the spawned shell. Defaults to `$HOME`.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub cols: Option<u32>,
    #[serde(default)]
    pub rows: Option<u32>,
    /// Display label; defaults to the shell basename (e.g. `bash`).
    #[serde(default)]
    pub label: Option<String>,
}

fn resolve_shell(spec: &TerminalOpenLocalSpec) -> String {
    spec.shell
        .clone()
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "/bin/bash".to_string())
}

#[tauri::command]
pub async fn terminal_open_local(
    spec: TerminalOpenLocalSpec,
    sessions: State<'_, Arc<TerminalSessions>>,
    app_handle: AppHandle,
) -> Result<String, String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let shell = resolve_shell(&spec);
    let cwd = spec
        .cwd
        .clone()
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| ".".to_string());
    let cols = spec.cols.unwrap_or(DEFAULT_COLS) as u16;
    let rows = spec.rows.unwrap_or(DEFAULT_ROWS) as u16;
    let label = spec.label.clone().unwrap_or_else(|| {
        std::path::Path::new(&shell)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&shell)
            .to_string()
    });

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("openpty failed: {e}"))?;

    let mut cmd = CommandBuilder::new(&shell);
    /* Login-shell flag so /etc/profile + ~/.bash_profile run, matching
    the experience the SSH path gets from sshd's login session. */
    cmd.arg("-l");
    cmd.cwd(&cwd);
    cmd.env("TERM", DEFAULT_TERM);
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn shell failed: {e}"))?;
    /* Drop the slave fd in the parent so EOF on the master propagates
    cleanly when the child exits (otherwise the reader thread would
    block forever on a still-open slave handle). */
    drop(pair.slave);

    let session_id = mint_session_id();
    let (control_tx, mut control_rx) = mpsc::channel::<ControlMsg>(CONTROL_CHANNEL_DEPTH);
    let summary = TerminalSessionSummary {
        session_id: session_id.clone(),
        host: "local".to_string(),
        user: shell.clone(),
        label,
        created_at_ms: now_ms(),
    };
    sessions
        .insert(
            session_id.clone(),
            SessionEntry {
                summary,
                control_tx,
            },
        )
        .await;

    /* Reader: a blocking std::thread that loops on master.read() and
    forwards bytes as Data events. portable-pty's reader is `Read +
    Send` but not async; spawn_blocking would tie up a tokio worker
    indefinitely, so a dedicated OS thread is the right shape. */
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("try_clone_reader: {e}"))?;
    let app_for_reader = app_handle.clone();
    let sid_for_reader = session_id.clone();
    std::thread::spawn(move || {
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let payload = TerminalEvent::Data {
                        session_id: &sid_for_reader,
                        bytes_b64: b64.encode(&buf[..n]),
                    };
                    let _ = app_for_reader.emit(TERMINAL_EVENT, payload);
                }
                Err(_) => break,
            }
        }
        let _ = app_for_reader.emit(
            TERMINAL_EVENT,
            TerminalEvent::Closed {
                session_id: &sid_for_reader,
                reason: "eof",
            },
        );
    });

    /* Writer / control: own the master writer + the PtyMaster (for
    resize) on a dedicated task. Use spawn_blocking so blocking
    writes don't stall the tokio runtime. The master is moved here
    so it stays alive — dropping it would HUP the child. */
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("take_writer: {e}"))?;
    let master = pair.master;
    let app_for_writer = app_handle.clone();
    let sid_for_writer = session_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = app_for_writer.emit(
            TERMINAL_EVENT,
            TerminalEvent::Ready {
                session_id: &sid_for_writer,
            },
        );
        use std::io::Write;
        while let Some(msg) = control_rx.recv().await {
            match msg {
                ControlMsg::Input(bytes) => {
                    /* writer.write_all is technically blocking but PTY
                    writes hit the kernel buffer and return fast;
                    acceptable on the multi-threaded runtime. */
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
                ControlMsg::Resize(c, r) => {
                    let _ = master.resize(PtySize {
                        rows: r as u16,
                        cols: c as u16,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
                ControlMsg::Close => break,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        if let Some(state) = app_for_writer.try_state::<Arc<TerminalSessions>>() {
            state.remove(&sid_for_writer).await;
        }
    });

    Ok(session_id)
}
