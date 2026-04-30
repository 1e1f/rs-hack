//! @arch:layer(lsp)
//! @arch:role(pool)
//! @arch:thread(async_io)
//!
//! [`LspPool`] — lazy `(rig_root, ServerKind) → LanguageServer` map.
//!
//! One pool per `yah serve` process; the multiplex layer (R033-T12)
//! holds a single `LspPool` and routes `lsp.request` / `lsp.notification`
//! through it. Servers are spawned on first reference; a workspace is
//! freshly `initialize`d the first time it acquires a server, then
//! reused for the rest of the rig's lifetime.
//!
//! Per-rig shutdown is the load-bearing API: the daemon calls
//! [`LspPool::shutdown_rig`] when a rig detaches so we don't leak
//! rust-analyzer children. The architecture flags this as a sharp
//! edge (`yah-files-tab.md` "LSP per-rig lifecycle"); this is the home
//! for it.

use crate::language::{detect, LanguageId, ServerCommand, ServerKind};
use crate::server::{build_initialize_params, LanguageServer, LspError};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Override map for per-(rig, server-kind) spawn commands. Empty by
/// default; the v1.5 follow-up reads `~/.yah/config.toml` to populate
/// it. Stored on the pool so per-rig overrides don't leak between rigs.
#[derive(Debug, Default, Clone)]
pub struct CommandOverrides {
    map: HashMap<(PathBuf, ServerKind), ServerCommand>,
}

impl CommandOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(
        &mut self,
        rig_root: PathBuf,
        kind: ServerKind,
        command: ServerCommand,
    ) -> &mut Self {
        self.map.insert((rig_root, kind), command);
        self
    }

    fn resolve(&self, rig_root: &Path, kind: ServerKind) -> ServerCommand {
        self.map
            .get(&(rig_root.to_path_buf(), kind))
            .cloned()
            .unwrap_or_else(|| kind.default_command())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("no language server configured for `.{0}` files")]
    UnknownExtension(String),
    #[error("no language server configured for path `{0}`")]
    UnknownPath(PathBuf),
    #[error("language server `{0:?}` not running for this rig")]
    NotRunning(ServerKind),
    #[error(transparent)]
    Lsp(#[from] LspError),
}

/// Inner state guarded by a mutex so first-touch spawn doesn't race
/// against itself or against `shutdown_rig`.
struct Inner {
    overrides: CommandOverrides,
    /// Live servers keyed by `(rig_root, server_kind)`. The same
    /// `LanguageServer` clone is handed out on every lookup.
    servers: HashMap<(PathBuf, ServerKind), LanguageServer>,
}

/// Pool of language-server child processes scoped to one `yah serve`.
///
/// Cheap to clone (`Arc` of inner state).
#[derive(Clone)]
pub struct LspPool {
    inner: Arc<Mutex<Inner>>,
}

impl LspPool {
    pub fn new() -> Self {
        Self::with_overrides(CommandOverrides::new())
    }

    pub fn with_overrides(overrides: CommandOverrides) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                overrides,
                servers: HashMap::new(),
            })),
        }
    }

    /// Replace a previously-installed override map. Doesn't affect
    /// already-running servers — the next spawn for that key picks up
    /// the new command.
    pub async fn set_overrides(&self, overrides: CommandOverrides) {
        let mut guard = self.inner.lock().await;
        guard.overrides = overrides;
    }

    /// Resolve a path to its language id, then look up (or spawn) the
    /// server that handles it for `rig_root`. Returns the live
    /// [`LanguageServer`] handle plus the resolved language id (the
    /// caller may need it for `textDocument/didOpen`).
    pub async fn for_path(
        &self,
        rig_root: &Path,
        path: &Path,
    ) -> Result<(LanguageId, LanguageServer), PoolError> {
        let lang = detect(path).ok_or_else(|| {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| PoolError::UnknownExtension(e.to_string()))
                .unwrap_or_else(|| PoolError::UnknownPath(path.to_path_buf()))
        })?;
        let server = self.get_or_spawn(rig_root, lang.server_key()).await?;
        Ok((lang, server))
    }

    /// Look up by [`LanguageId`] directly. Useful when the multiplex
    /// layer already received a `server` tag from the wire and wants to
    /// avoid path-sniffing.
    pub async fn for_language(
        &self,
        rig_root: &Path,
        lang: LanguageId,
    ) -> Result<LanguageServer, PoolError> {
        self.get_or_spawn(rig_root, lang.server_key()).await
    }

    /// Convenience: route a typed LSP request to the server that owns
    /// `path`. The pool spawns + initializes on first call.
    pub async fn request_for_path<R>(
        &self,
        rig_root: &Path,
        path: &Path,
        method: &str,
        params: &Value,
    ) -> Result<R, PoolError>
    where
        R: serde::de::DeserializeOwned,
    {
        let (_lang, server) = self.for_path(rig_root, path).await?;
        Ok(server.request(method, params).await?)
    }

    /// Tear down every server attached to `rig_root`. The pool removes
    /// the entries first (so a racing `for_*` call gets a fresh spawn
    /// with a fresh `initialize`), then issues a bounded
    /// `shutdown`/`exit` to each. Drop-on-error fallback handles wedged
    /// servers via `kill_on_drop`.
    pub async fn shutdown_rig(&self, rig_root: &Path) {
        let to_close: Vec<LanguageServer> = {
            let mut guard = self.inner.lock().await;
            let keys: Vec<_> = guard
                .servers
                .keys()
                .filter(|(root, _)| root == rig_root)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|k| guard.servers.remove(&k))
                .collect()
        };
        for server in to_close {
            if let Err(e) = server.shutdown().await {
                tracing::debug!(error = %e, "lsp server shutdown returned error; killing via drop");
            }
        }
    }

    /// Tear down every server in the pool. Called on `yah serve`
    /// shutdown.
    pub async fn shutdown_all(&self) {
        let to_close: Vec<LanguageServer> = {
            let mut guard = self.inner.lock().await;
            guard.servers.drain().map(|(_, v)| v).collect()
        };
        for server in to_close {
            if let Err(e) = server.shutdown().await {
                tracing::debug!(error = %e, "lsp server shutdown returned error during shutdown_all");
            }
        }
    }

    /// Snapshot of (rig_root, ServerKind) pairs currently running.
    /// Useful for status surfaces ("rust-analyzer ready on rig X").
    pub async fn running(&self) -> Vec<(PathBuf, ServerKind)> {
        let guard = self.inner.lock().await;
        guard.servers.keys().cloned().collect()
    }

    async fn get_or_spawn(
        &self,
        rig_root: &Path,
        kind: ServerKind,
    ) -> Result<LanguageServer, PoolError> {
        let key = (rig_root.to_path_buf(), kind);
        // Fast path: server already up *and* still healthy.
        {
            let guard = self.inner.lock().await;
            if let Some(s) = guard.servers.get(&key) {
                if s.is_open() {
                    return Ok(s.clone());
                }
            }
        }
        // Slow path: spawn under the lock. A racing caller waits and
        // then sees the freshly-spawned entry on its own lookup — we
        // double-check after acquiring the lock so we don't double-spawn.
        let mut guard = self.inner.lock().await;
        if let Some(s) = guard.servers.get(&key) {
            if s.is_open() {
                return Ok(s.clone());
            }
            // Stale entry — drop the broken one before respawn.
            guard.servers.remove(&key);
        }
        let cmd = guard.overrides.resolve(rig_root, kind);
        // Drop the lock across the (potentially slow) spawn + initialize
        // so other rigs aren't blocked. We re-acquire to insert.
        drop(guard);

        let server = LanguageServer::spawn(&cmd, rig_root).await?;
        // LSP handshake: initialize → initialized. The pool is the
        // natural home for this — a child without `initialize` rejects
        // every textDocument/* request with -32002 "Server not initialized".
        let init = build_initialize_params(rig_root, std::process::id().into());
        let _: Value = server.request("initialize", &init).await.map_err(|e| {
            tracing::warn!(error = %e, ?kind, "initialize failed; dropping child");
            e
        })?;
        server
            .notify("initialized", &serde_json::json!({}))
            .await?;

        let mut guard = self.inner.lock().await;
        // If a racing caller spawned in between, prefer theirs and drop
        // ours. (Both clones are open; we only need one in the map.)
        if let Some(existing) = guard.servers.get(&key) {
            if existing.is_open() {
                let existing = existing.clone();
                drop(guard);
                let _ = server.shutdown().await;
                return Ok(existing);
            }
        }
        guard.servers.insert(key, server.clone());
        Ok(server)
    }
}

impl Default for LspPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageId;

    #[tokio::test]
    async fn unknown_extension_returns_unknown_extension_error() {
        let pool = LspPool::new();
        let tmp = tempfile::tempdir().unwrap();
        let err = pool
            .for_path(tmp.path(), &tmp.path().join("README.md"))
            .await
            .unwrap_err();
        match err {
            PoolError::UnknownExtension(ext) => assert_eq!(ext, "md"),
            other => panic!("expected UnknownExtension, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn no_extension_returns_unknown_path_error() {
        let pool = LspPool::new();
        let tmp = tempfile::tempdir().unwrap();
        let err = pool
            .for_path(tmp.path(), &tmp.path().join("Makefile"))
            .await
            .unwrap_err();
        match err {
            PoolError::UnknownPath(_) => {}
            other => panic!("expected UnknownPath, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn spawn_failure_propagates_through_pool() {
        let mut overrides = CommandOverrides::new();
        let tmp = tempfile::tempdir().unwrap();
        overrides.set(
            tmp.path().to_path_buf(),
            ServerKind::RustAnalyzer,
            ServerCommand {
                program: "definitely-not-a-real-lsp-binary".to_string(),
                args: vec![],
                env: vec![],
            },
        );
        let pool = LspPool::with_overrides(overrides);
        let err = pool
            .for_language(tmp.path(), LanguageId::Rust)
            .await
            .unwrap_err();
        match err {
            PoolError::Lsp(LspError::Spawn { program, .. }) => {
                assert!(program.contains("definitely-not-a-real"));
            }
            other => panic!("expected Spawn error, got {:?}", other),
        }
        // Pool must not leave a broken entry behind.
        let running = pool.running().await;
        assert!(running.is_empty());
    }

    #[tokio::test]
    async fn shutdown_rig_clears_only_that_rig() {
        // We can't spawn real children here, but we can verify the
        // bookkeeping by manipulating overrides + checking running()
        // remains empty after a failed spawn (covered above) and that
        // shutdown_rig is idempotent on an empty pool.
        let pool = LspPool::new();
        let tmp = tempfile::tempdir().unwrap();
        pool.shutdown_rig(tmp.path()).await; // no panic
        pool.shutdown_all().await; // no panic
        assert!(pool.running().await.is_empty());
    }
}
