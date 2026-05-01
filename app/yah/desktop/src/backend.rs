//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! `RigBackend` — uniform dispatch over local and remote rigs.
//!
//! Every `arch_*` Tauri command in [`crate::commands`] resolves the rig
//! to a [`RigBackend`] and dispatches the request through it. The Local
//! arm goes straight to an `Arc<KgService>` (zero-cost vs. a direct
//! method call); the Remote arm forwards each call as line-delimited
//! JSON-RPC over an SSH channel via [`SshRpcClient`].
//!
//! The wire shapes used by both arms come from `yah-rpc` — the
//! contract is the same on both sides, so the renderer is unaware of
//! the backend kind. See `.yah/arch/authored/rig-backend-dispatch.md` for the
//! design rationale.
//!
//! ## Errors
//!
//! Methods return `Result<T, String>` to match the `#[tauri::command]`
//! contract. Local errors come from `DaemonError`; remote errors come
//! from `RpcError` (transport drops, daemon error frames, decode
//! failures). Both stringify into the same JS-side error shape.

use kg::event::IndexReason;
use kg::ids::{NodeFull, NodeId};
use kg::kind::Lang;
use kg_daemon::KgService;
use rpc::{
    ArchiveTicketParams, ArchiveTicketResult, DirListParams, DirListResult, DirWatchParams,
    FileReadParams, FileReadResult, FileWatchParams, GetTicketParams, GetTicketResult,
    ListAuthoredFilesParams, ListAuthoredFilesResult, ListRelaysParams, ListRelaysResult,
    ListTicketsParams, ListTicketsResult, LookupParams, LookupResult, MoveTicketParams,
    MoveTicketResult, NeighborsParams, NeighborsResult, ReadAuthoredFileParams,
    ReadAuthoredFileResult, RootsParams, RootsResult, StatsResult, Subgraph, SubgraphParams,
    TicketPromptParams, TicketPromptResult, UnwatchParams, UnwatchResult, ValidateParams,
    ValidateResult, WatchResult,
};
use rpc_ssh::{ReindexReasonWire, SshRpcClient};
use std::path::PathBuf;
use std::sync::Arc;

/// One rig's runtime — either an in-process daemon (local rig) or an
/// SSH-RPC client driving a remote `yah serve --stdio`.
///
/// Cheap to clone: both arms hold `Arc`-backed handles.
#[derive(Clone)]
pub enum RigBackend {
    Local(Arc<KgService>),
    Remote(SshRpcClient),
}

impl RigBackend {
    /// Borrow the local `KgService` if this is a local rig. Used by the
    /// `YAH_RIG_ROOT` auto-boot dev path in `lib.rs` and by callers that
    /// need direct daemon access (e.g. snapshot persistence) where the
    /// remote shape doesn't apply.
    pub fn local(&self) -> Option<Arc<KgService>> {
        match self {
            Self::Local(svc) => Some(Arc::clone(svc)),
            Self::Remote(_) => None,
        }
    }

    /// Open the rig: for Local, boots from snapshot, starts the watcher,
    /// and saves a fresh snapshot; for Remote, fires `arch.open_rig` over
    /// the SSH session which triggers the same lifecycle on the remote
    /// daemon. Idempotent on both sides.
    pub async fn open_rig(&self, path: PathBuf) -> Result<WalkSummaryDto, String> {
        match self {
            Self::Local(svc) => {
                let snap_path = kg_daemon::default_snapshot_path(&path);
                let summary = svc
                    .boot_with_snapshot(path, &snap_path)
                    .await
                    .map_err(|e| e.to_string())?;
                svc.start_watching().await.map_err(|e| e.to_string())?;
                if let Err(e) = svc.save_default().await {
                    tracing::warn!(error = %e, "failed to persist KG snapshot after boot");
                }
                Ok(WalkSummaryDto::from_walk_summary(&summary))
            }
            Self::Remote(client) => {
                let r = client.open_rig().await.map_err(|e| e.to_string())?;
                Ok(WalkSummaryDto {
                    files_seen: r.files_seen,
                    files_indexed: r.files_indexed,
                    files_skipped: r.files_skipped,
                    parse_errors: r.parse_errors,
                })
            }
        }
    }

    pub async fn close_rig(&self) -> Result<(), String> {
        match self {
            Self::Local(svc) => {
                svc.stop_watching().await;
                Ok(())
            }
            Self::Remote(client) => client.close_rig().await.map_err(|e| e.to_string()),
        }
    }

    pub async fn subgraph(&self, params: SubgraphParams) -> Result<Subgraph, String> {
        match self {
            Self::Local(svc) => Ok(svc.subgraph(params).await),
            Self::Remote(client) => client.subgraph(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn lookup(&self, params: LookupParams) -> Result<LookupResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.lookup(params).await),
            Self::Remote(client) => client.lookup(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn node(&self, id: NodeId) -> Result<Option<NodeFull>, String> {
        match self {
            Self::Local(svc) => Ok(svc.node(id).await),
            Self::Remote(client) => client.node(id).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn neighbors(&self, params: NeighborsParams) -> Result<NeighborsResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.neighbors(params).await),
            Self::Remote(client) => client.neighbors(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn roots(&self, params: RootsParams) -> Result<RootsResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.roots(params).await),
            Self::Remote(client) => client.roots(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn stats(&self) -> Result<StatsResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.stats().await),
            Self::Remote(client) => client.stats().await.map_err(|e| e.to_string()),
        }
    }

    pub async fn languages(&self) -> Result<Vec<Lang>, String> {
        match self {
            Self::Local(svc) => Ok(svc.languages()),
            Self::Remote(client) => client.languages().await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list_authored_files(
        &self,
        params: ListAuthoredFilesParams,
    ) -> Result<ListAuthoredFilesResult, String> {
        match self {
            Self::Local(svc) => svc
                .list_authored_files(params)
                .await
                .map_err(|e| e.to_string()),
            Self::Remote(client) => client
                .list_authored_files(params)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn read_authored_file(
        &self,
        params: ReadAuthoredFileParams,
    ) -> Result<ReadAuthoredFileResult, String> {
        match self {
            Self::Local(svc) => svc
                .read_authored_file(params)
                .await
                .map_err(|e| e.to_string()),
            Self::Remote(client) => client
                .read_authored_file(params)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn file_read(&self, params: FileReadParams) -> Result<FileReadResult, String> {
        match self {
            Self::Local(svc) => svc.file_read(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.file_read(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn dir_list(&self, params: DirListParams) -> Result<DirListResult, String> {
        match self {
            Self::Local(svc) => svc.dir_list(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.dir_list(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn watch_file(&self, params: FileWatchParams) -> Result<WatchResult, String> {
        match self {
            Self::Local(svc) => svc.watch_file(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.watch_file(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn watch_dir(&self, params: DirWatchParams) -> Result<WatchResult, String> {
        match self {
            Self::Local(svc) => svc.watch_dir(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.watch_dir(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn unwatch(&self, params: UnwatchParams) -> Result<UnwatchResult, String> {
        match self {
            Self::Local(svc) => svc.unwatch(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.unwatch(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list_tickets(
        &self,
        params: ListTicketsParams,
    ) -> Result<ListTicketsResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.list_tickets(params).await),
            Self::Remote(client) => client.list_tickets(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list_relays(&self, params: ListRelaysParams) -> Result<ListRelaysResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.list_relays(params).await),
            Self::Remote(client) => client.list_relays(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn get_ticket(&self, params: GetTicketParams) -> Result<GetTicketResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.get_ticket(params).await),
            Self::Remote(client) => client.get_ticket(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn validate(&self, params: ValidateParams) -> Result<ValidateResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.validate(params).await),
            Self::Remote(client) => client.validate(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn ticket_prompt(
        &self,
        params: TicketPromptParams,
    ) -> Result<TicketPromptResult, String> {
        match self {
            Self::Local(svc) => Ok(svc.ticket_prompt(params).await),
            Self::Remote(client) => client
                .ticket_prompt(params)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn move_ticket(&self, params: MoveTicketParams) -> Result<MoveTicketResult, String> {
        match self {
            Self::Local(svc) => svc.move_ticket(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client.move_ticket(params).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn archive_ticket(
        &self,
        params: ArchiveTicketParams,
    ) -> Result<ArchiveTicketResult, String> {
        match self {
            Self::Local(svc) => svc.archive_ticket(params).await.map_err(|e| e.to_string()),
            Self::Remote(client) => client
                .archive_ticket(params)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn reindex_path(&self, path: &str, reason: IndexReasonDto) -> Result<(), String> {
        match self {
            Self::Local(svc) => svc
                .reindex_path(std::path::Path::new(path), reason.into_local())
                .await
                .map_err(|e| e.to_string()),
            Self::Remote(client) => client
                .reindex_path(path, reason.into_remote())
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn touch(
        &self,
        paths: Vec<String>,
        tool: String,
        relay: String,
    ) -> Result<(), String> {
        match self {
            Self::Local(svc) => {
                svc.touch(&paths, &tool, &relay).await;
                Ok(())
            }
            Self::Remote(client) => client
                .touch(paths, tool, relay)
                .await
                .map_err(|e| e.to_string()),
        }
    }
}

/// Walk-summary wire DTO returned by `arch_open_rig`. Both backends
/// produce the same numeric shape; this type lives here (rather than in
/// `commands.rs`) so the dispatch impl can return it directly.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalkSummaryDto {
    pub files_seen: u32,
    pub files_indexed: u32,
    pub files_skipped: u32,
    pub parse_errors: u32,
}

impl WalkSummaryDto {
    fn from_walk_summary(s: &kg_store::WalkSummary) -> Self {
        Self {
            files_seen: s.files_seen,
            files_indexed: s.files_indexed,
            files_skipped: s.files_skipped,
            parse_errors: s.parse_errors,
        }
    }
}

/// Reindex reason as the renderer sends it. Translates to either the
/// in-process [`IndexReason`] or the wire-only [`ReindexReasonWire`]
/// depending on backend. Kept here so `commands.rs` doesn't need to
/// know about both downstream types.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexReasonDto {
    Boot,
    FileWatch,
    Manual,
    AgentEdit,
}

impl IndexReasonDto {
    fn into_local(self) -> IndexReason {
        match self {
            Self::Boot => IndexReason::Boot,
            Self::FileWatch => IndexReason::FileWatch,
            Self::Manual => IndexReason::Manual,
            Self::AgentEdit => IndexReason::AgentEdit,
        }
    }

    fn into_remote(self) -> ReindexReasonWire {
        match self {
            Self::Boot => ReindexReasonWire::Boot,
            Self::FileWatch => ReindexReasonWire::FileWatch,
            Self::Manual => ReindexReasonWire::Manual,
            Self::AgentEdit => ReindexReasonWire::AgentEdit,
        }
    }
}
