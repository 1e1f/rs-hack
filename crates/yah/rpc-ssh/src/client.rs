//! @arch:layer(rpc)
//! @arch:role(transport)
//! @arch:thread(async_io)
//!
//! [`SshRpcClient`] — drives a remote `yah serve --stdio` over SSH.
//!
//! Implementation is "shell out to the system `ssh` binary": no Rust SSH
//! library dependency, the user's existing `~/.ssh/config` and agent
//! work without translation, and the connection lifetime is tied to the
//! child process. Trade-off vs. a library transport (`russh` /
//! `openssh-rs`): we don't get fine-grained control over auth retry,
//! channel multiplexing, or sftp out of the box. None of that is on
//! R019's path today; if it shows up we swap the [`spawn_ssh`] internal
//! helper without changing the public surface.
//!
//! The typed methods below are all thin wrappers around
//! [`JsonRpcSession::call`]. They mirror the surface of `KgService` so
//! the Tauri host's RigBackend dispatch (R019-F3) is one match per arm
//! with no shape conversion.

use crate::session::{ArchEventStream, JsonRpcSession, RpcError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use kg::ids::{NodeFull, NodeId};
use kg::kind::Lang;
use rpc::{
    method, ArchiveTicketParams, ArchiveTicketResult, DirListParams, DirListResult,
    DirWatchParams, FileReadParams, FileReadResult, FileWatchParams, GetTicketParams,
    GetTicketResult, ListAuthoredFilesParams, ListAuthoredFilesResult, ListRelaysParams,
    ListRelaysResult, ListTicketsParams, ListTicketsResult, LookupParams, LookupResult,
    MoveTicketParams, MoveTicketResult, NeighborsParams, NeighborsResult, ReadAuthoredFileParams,
    ReadAuthoredFileResult, RootsParams, RootsResult, StatsResult, Subgraph, SubgraphParams,
    TicketPromptParams, TicketPromptResult, UnwatchParams, UnwatchResult, ValidateParams,
    ValidateResult, WatchResult,
};

/// Connection spec for one remote rig. Mirrors the renderer-side
/// `Rig` fields the Connect-Remote modal collects (R019-T5) so the
/// Tauri host can construct an [`SshRpcClient`] from the existing
/// `Rig` shape with no extra plumbing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshRpcConfig {
    /// SSH host (DNS name or IP).
    pub host: String,
    /// SSH user (always required for remote rigs).
    pub user: String,
    /// `None` falls back to the SSH default (22). The system `ssh`
    /// binary already honours `~/.ssh/config` when both are blank, so
    /// "leave it default" really does just work.
    pub port: Option<u16>,
    /// Path to a private key file. `None` falls back to SSH agent +
    /// `~/.ssh/id_*` defaults at connection time.
    pub key_path: Option<PathBuf>,
    /// Remote rig workspace path (passed to `yah serve --stdio --rig`).
    pub remote_workspace: PathBuf,
    /// Path to the `yah` binary on the remote box. Defaults to `"yah"`
    /// (relies on the remote `$PATH`); `yah ssh-install` puts the
    /// binary on the remote and may prefer an explicit path.
    pub remote_yah_bin: Option<String>,
    /// Extra flags forwarded to the `ssh` invocation. `-o
    /// ServerAliveInterval=30` defaults are always applied; this is
    /// for things like `-J jump-host`.
    #[serde(default)]
    pub extra_ssh_args: Vec<String>,
}

impl SshRpcConfig {
    fn remote_command(&self) -> String {
        let bin = self.remote_yah_bin.as_deref().unwrap_or("yah");
        // Quote the workspace path naively — paths with single quotes
        // are vanishingly rare and would require shell-escape handling
        // we don't owe today. The renderer enforces a sane workspace
        // path at the Connect-Remote modal.
        format!(
            "{} serve --stdio --rig '{}'",
            bin,
            self.remote_workspace.display()
        )
    }
}

/// Reconnect strategy used by [`SshRpcClient::call`] after a
/// [`RpcError::TransportClosed`].
///
/// `initial` and `max` are the bounds on the per-attempt sleep; the
/// delay doubles after each failed attempt up to `max`. `max_attempts`
/// caps the retry budget per call — once exhausted the call surfaces
/// the underlying error to the renderer so the ConnectionStrip can
/// flip red.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub max: Duration,
    pub max_attempts: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(250),
            max: Duration::from_secs(8),
            max_attempts: 5,
        }
    }
}

/// One live or recently-closed SSH session: the running `ssh` child
/// (kept alive so its stdin/stdout don't drop) plus the JSON-RPC
/// session built on top of those handles.
struct Live {
    session: JsonRpcSession,
    /// `_child` is owned solely to keep the SSH process alive for as
    /// long as the session is in use; reads and writes go through
    /// [`session`].
    _child: Child,
}

/// Inner state guarded by a single mutex so reconnects don't race with
/// each other or with in-flight calls grabbing a fresh session handle.
struct Inner {
    config: SshRpcConfig,
    policy: ReconnectPolicy,
    live: Mutex<Option<Live>>,
}

/// Client handle for one remote rig. Cheap to clone (`Arc` of internals).
#[derive(Clone)]
pub struct SshRpcClient {
    inner: Arc<Inner>,
}

impl SshRpcClient {
    /// Construct without opening the connection. The first
    /// [`SshRpcClient::call`] (or an explicit
    /// [`SshRpcClient::ensure_connected`]) is what spawns `ssh` — this
    /// matches the renderer's "lazy on first activation" model
    /// (R019-T5).
    pub fn new(config: SshRpcConfig) -> Self {
        Self::with_policy(config, ReconnectPolicy::default())
    }

    pub fn with_policy(config: SshRpcConfig, policy: ReconnectPolicy) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                policy,
                live: Mutex::new(None),
            }),
        }
    }

    /// Read-only view of the config the client was constructed with.
    pub fn config(&self) -> &SshRpcConfig {
        &self.inner.config
    }

    /// Force a connection open if one isn't already. Useful for the
    /// renderer's "Test connection" affordance (R019-T5 polish item).
    pub async fn ensure_connected(&self) -> Result<(), RpcError> {
        self.with_session(|_| Ok::<(), RpcError>(())).await
    }

    /// Subscribe to the current session's `arch:event` stream. If the
    /// session is closed, opens a new one first so the subscriber binds
    /// to a session that's actually receiving frames.
    ///
    /// Note: a stream becomes inert when its underlying session closes
    /// (e.g. transport drop forcing reconnect). The caller is expected
    /// to resubscribe after reconnects — same contract `KgService::subscribe`
    /// has after a daemon restart.
    pub async fn subscribe_events(&self) -> Result<ArchEventStream, RpcError> {
        self.with_session(|s| Ok::<ArchEventStream, RpcError>(s.subscribe_events()))
            .await
    }

    /// Generic call: route to the live session, opening one if needed.
    /// On [`RpcError::TransportClosed`] re-establishes per the
    /// [`ReconnectPolicy`] and retries up to the policy's
    /// `max_attempts`. Other errors propagate immediately — only
    /// transport drops are treated as recoverable.
    pub async fn call<P, R>(&self, method: &str, params: &P) -> Result<R, RpcError>
    where
        P: serde::Serialize + ?Sized,
        R: serde::de::DeserializeOwned,
    {
        let mut backoff = self.inner.policy.initial;
        let mut attempts: u32 = 0;
        loop {
            let session = self.session_handle().await?;
            match session.call::<P, R>(method, params).await {
                Ok(value) => return Ok(value),
                Err(RpcError::TransportClosed) => {
                    self.mark_closed().await;
                    attempts = attempts.saturating_add(1);
                    if attempts >= self.inner.policy.max_attempts {
                        return Err(RpcError::TransportClosed);
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff.saturating_mul(2)).min(self.inner.policy.max);
                }
                Err(other) => return Err(other),
            }
        }
    }

    /// Acquire a clone of the current live session, opening one if
    /// needed.
    async fn session_handle(&self) -> Result<JsonRpcSession, RpcError> {
        let mut guard = self.inner.live.lock().await;
        if let Some(live) = guard.as_ref() {
            if live.session.is_open() {
                return Ok(live.session.clone());
            }
        }
        // Either no session yet, or the existing one closed — open fresh.
        let live = open_session(&self.inner.config).await?;
        let session = live.session.clone();
        *guard = Some(live);
        Ok(session)
    }

    async fn mark_closed(&self) {
        let mut guard = self.inner.live.lock().await;
        // Drop the prior live entry so the next session_handle reopens.
        // The Drop on Child sends SIGKILL — fine for a transport that's
        // already broken; ssh's own keepalive timeouts already failed.
        *guard = None;
    }

    /// Internal helper: run `f` against a live session.
    async fn with_session<R, F>(&self, f: F) -> Result<R, RpcError>
    where
        F: FnOnce(&JsonRpcSession) -> Result<R, RpcError>,
    {
        let session = self.session_handle().await?;
        f(&session)
    }

    // ---------- Typed surface mirroring KgService ----------

    pub async fn open_rig(&self) -> Result<OpenRigResult, RpcError> {
        let path = self.inner.config.remote_workspace.display().to_string();
        self.call("arch.open_rig", &serde_json::json!({ "path": path }))
            .await
    }

    pub async fn close_rig(&self) -> Result<(), RpcError> {
        let _: serde_json::Value = self
            .call("arch.close_rig", &serde_json::Value::Null)
            .await?;
        Ok(())
    }

    pub async fn subgraph(&self, params: SubgraphParams) -> Result<Subgraph, RpcError> {
        self.call("arch.subgraph", &params).await
    }

    pub async fn lookup(&self, params: LookupParams) -> Result<LookupResult, RpcError> {
        self.call("arch.lookup", &params).await
    }

    pub async fn node(&self, id: NodeId) -> Result<Option<NodeFull>, RpcError> {
        let result: NodeWire = self.call("arch.node", &serde_json::json!({ "id": id })).await?;
        Ok(result.node)
    }

    pub async fn neighbors(&self, params: NeighborsParams) -> Result<NeighborsResult, RpcError> {
        self.call("arch.neighbors", &params).await
    }

    pub async fn roots(&self, params: RootsParams) -> Result<RootsResult, RpcError> {
        self.call("arch.roots", &params).await
    }

    pub async fn stats(&self) -> Result<StatsResult, RpcError> {
        self.call("arch.stats", &serde_json::Value::Null).await
    }

    pub async fn languages(&self) -> Result<Vec<Lang>, RpcError> {
        let result: LanguagesWire = self
            .call("arch.languages", &serde_json::Value::Null)
            .await?;
        Ok(result.langs)
    }

    pub async fn list_tickets(
        &self,
        params: ListTicketsParams,
    ) -> Result<ListTicketsResult, RpcError> {
        self.call("arch.list_tickets", &params).await
    }

    pub async fn list_relays(
        &self,
        params: ListRelaysParams,
    ) -> Result<ListRelaysResult, RpcError> {
        self.call("arch.list_relays", &params).await
    }

    pub async fn get_ticket(&self, params: GetTicketParams) -> Result<GetTicketResult, RpcError> {
        self.call("arch.get_ticket", &params).await
    }

    pub async fn validate(&self, params: ValidateParams) -> Result<ValidateResult, RpcError> {
        self.call("arch.validate", &params).await
    }

    pub async fn ticket_prompt(
        &self,
        params: TicketPromptParams,
    ) -> Result<TicketPromptResult, RpcError> {
        self.call("arch.ticket_prompt", &params).await
    }

    pub async fn move_ticket(
        &self,
        params: MoveTicketParams,
    ) -> Result<MoveTicketResult, RpcError> {
        self.call("arch.move_ticket", &params).await
    }

    pub async fn archive_ticket(
        &self,
        params: ArchiveTicketParams,
    ) -> Result<ArchiveTicketResult, RpcError> {
        self.call(method::ARCHIVE_TICKET, &params).await
    }

    pub async fn list_authored_files(
        &self,
        params: ListAuthoredFilesParams,
    ) -> Result<ListAuthoredFilesResult, RpcError> {
        self.call("arch.list_authored_files", &params).await
    }

    pub async fn read_authored_file(
        &self,
        params: ReadAuthoredFileParams,
    ) -> Result<ReadAuthoredFileResult, RpcError> {
        self.call("arch.read_authored_file", &params).await
    }

    pub async fn file_read(
        &self,
        params: FileReadParams,
    ) -> Result<FileReadResult, RpcError> {
        self.call(method::FILE_READ, &params).await
    }

    pub async fn dir_list(
        &self,
        params: DirListParams,
    ) -> Result<DirListResult, RpcError> {
        self.call(method::DIR_LIST, &params).await
    }

    pub async fn watch_file(
        &self,
        params: FileWatchParams,
    ) -> Result<WatchResult, RpcError> {
        self.call(method::FILE_WATCH, &params).await
    }

    pub async fn watch_dir(
        &self,
        params: DirWatchParams,
    ) -> Result<WatchResult, RpcError> {
        self.call(method::DIR_WATCH, &params).await
    }

    pub async fn unwatch(
        &self,
        params: UnwatchParams,
    ) -> Result<UnwatchResult, RpcError> {
        self.call(method::FILE_UNWATCH, &params).await
    }

    pub async fn reindex_path(
        &self,
        path: &str,
        reason: ReindexReasonWire,
    ) -> Result<(), RpcError> {
        let _: serde_json::Value = self
            .call(
                "arch.reindex_path",
                &serde_json::json!({ "path": path, "reason": reason.as_str() }),
            )
            .await?;
        Ok(())
    }

    pub async fn touch(
        &self,
        paths: Vec<String>,
        tool: String,
        relay: String,
    ) -> Result<(), RpcError> {
        let _: serde_json::Value = self
            .call(
                "arch.touch",
                &serde_json::json!({ "paths": paths, "tool": tool, "relay": relay }),
            )
            .await?;
        Ok(())
    }
}

/// Reason tag for `arch.reindex_path`. Mirrors the daemon's
/// `IndexReason` but as a stringly-typed wire enum so the SSH client
/// doesn't have to depend on `yah-kg-store`'s indexer types directly.
#[derive(Debug, Clone, Copy)]
pub enum ReindexReasonWire {
    Boot,
    FileWatch,
    Manual,
    AgentEdit,
}

impl ReindexReasonWire {
    fn as_str(self) -> &'static str {
        match self {
            ReindexReasonWire::Boot => "boot",
            ReindexReasonWire::FileWatch => "file_watch",
            ReindexReasonWire::Manual => "manual",
            ReindexReasonWire::AgentEdit => "agent_edit",
        }
    }
}

/// `arch.open_rig` returns the walk summary in camelCase (see
/// `yah/src/serve.rs`). The wire shape duplicates `WalkSummary` rather
/// than depending on `yah-kg-store` here.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRigResult {
    pub files_seen: u32,
    pub files_indexed: u32,
    pub files_skipped: u32,
    pub parse_errors: u32,
}

#[derive(Deserialize)]
struct NodeWire {
    node: Option<NodeFull>,
}

#[derive(Deserialize)]
struct LanguagesWire {
    langs: Vec<Lang>,
}

/// Spawn the local `ssh` binary, piping stdin/stdout for JSON-RPC and
/// inheriting stderr so the user sees authentication / banner output
/// in the Tauri host's stderr stream.
async fn spawn_ssh(config: &SshRpcConfig) -> Result<Child, RpcError> {
    let mut cmd = Command::new("ssh");
    cmd.arg("-T") // no TTY allocation; we want raw stdio for JSON-RPC.
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ServerAliveInterval=30")
        .arg("-o")
        .arg("ServerAliveCountMax=3");
    if let Some(port) = config.port {
        cmd.arg("-p").arg(port.to_string());
    }
    if let Some(key) = &config.key_path {
        cmd.arg("-i").arg(key);
    }
    for arg in &config.extra_ssh_args {
        cmd.arg(arg);
    }
    cmd.arg(format!("{}@{}", config.user, config.host));
    cmd.arg(config.remote_command());
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    cmd.spawn()
        .map_err(|e| RpcError::Io(format!("failed to spawn ssh: {}", e)))
}

async fn open_session(config: &SshRpcConfig) -> Result<Live, RpcError> {
    let mut child = spawn_ssh(config).await?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| RpcError::Io("ssh child missing stdin handle".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RpcError::Io("ssh child missing stdout handle".into()))?;
    let session = JsonRpcSession::spawn(stdout, stdin);
    Ok(Live {
        session,
        _child: child,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_command_quotes_the_workspace_path() {
        let cfg = SshRpcConfig {
            host: "box".into(),
            user: "agent".into(),
            port: None,
            key_path: None,
            remote_workspace: PathBuf::from("/srv/code"),
            remote_yah_bin: None,
            extra_ssh_args: vec![],
        };
        let cmd = cfg.remote_command();
        assert_eq!(cmd, "yah serve --stdio --rig '/srv/code'");
    }

    #[test]
    fn remote_command_honours_explicit_yah_bin() {
        let cfg = SshRpcConfig {
            host: "box".into(),
            user: "agent".into(),
            port: None,
            key_path: None,
            remote_workspace: PathBuf::from("/srv/code"),
            remote_yah_bin: Some("/opt/yah/bin/yah".into()),
            extra_ssh_args: vec![],
        };
        let cmd = cfg.remote_command();
        assert_eq!(cmd, "/opt/yah/bin/yah serve --stdio --rig '/srv/code'");
    }

    #[test]
    fn reconnect_policy_default_is_sane() {
        let p = ReconnectPolicy::default();
        assert!(p.initial < p.max);
        assert!(p.max_attempts >= 1);
    }
}
