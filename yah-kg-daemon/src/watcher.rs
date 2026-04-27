//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! `notify`-driven file watcher.
//!
//! `notify`'s callback runs on its own OS thread; we forward events through
//! a `tokio::sync::mpsc` channel to a runtime task that debounces and calls
//! the user-provided handler. Debouncing is a flat 50 ms window: editors
//! typically emit several events per save (rename → write → chmod), and a
//! short coalesce keeps us from reindexing the same file three times.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const DEBOUNCE_MS: u64 = 50;

#[derive(Debug, Clone, Copy)]
pub enum WatcherKind {
    Recursive,
}

impl From<WatcherKind> for RecursiveMode {
    fn from(k: WatcherKind) -> Self {
        match k {
            WatcherKind::Recursive => RecursiveMode::Recursive,
        }
    }
}

pub struct WatcherHandle {
    stop: Option<oneshot::Sender<()>>,
    /// Hold the underlying watcher alive for the lifetime of the handle.
    /// `notify` stops watching when the watcher value is dropped.
    _watcher: RecommendedWatcher,
}

impl WatcherHandle {
    /// Stop the watcher loop. Idempotent.
    pub async fn stop(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

type Handler = Box<
    dyn Fn(Vec<PathBuf>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

/// Spawn a watcher rooted at `root` that calls `handler` with a deduped
/// batch of changed paths.
pub async fn spawn_watcher<F, Fut>(
    kind: WatcherKind,
    root: PathBuf,
    handler: F,
) -> Result<WatcherHandle, notify::Error>
where
    F: Fn(Vec<PathBuf>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let handler: Handler = Box::new(move |paths| Box::pin(handler(paths)));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(ev) = res {
            let _ = event_tx.send(ev);
        }
    })?;
    watcher.watch(&root, kind.into())?;

    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = &mut stop_rx => break,
                event = event_rx.recv() => {
                    let Some(first) = event else { break };
                    // Pull more events that arrive within the debounce window.
                    let mut batch: HashSet<PathBuf> = HashSet::new();
                    if is_actionable(&first) {
                        for p in &first.paths { batch.insert(p.clone()); }
                    }
                    let deadline = tokio::time::Instant::now()
                        + Duration::from_millis(DEBOUNCE_MS);
                    loop {
                        let timeout = deadline.saturating_duration_since(
                            tokio::time::Instant::now(),
                        );
                        if timeout.is_zero() { break; }
                        match tokio::time::timeout(timeout, event_rx.recv()).await {
                            Ok(Some(ev)) => {
                                if is_actionable(&ev) {
                                    for p in &ev.paths { batch.insert(p.clone()); }
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                    if !batch.is_empty() {
                        let paths: Vec<PathBuf> = batch.into_iter().collect();
                        handler(paths).await;
                    }
                }
            }
        }
    });

    Ok(WatcherHandle {
        stop: Some(stop_tx),
        _watcher: watcher,
    })
}

fn is_actionable(ev: &Event) -> bool {
    matches!(
        ev.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Test-only helper: check whether a path's extension would be picked up by
/// any registered indexer. Not used by the daemon at runtime — the registry
/// makes that determination on dispatch.
#[doc(hidden)]
pub fn _path_has_extension(p: &Path) -> bool {
    p.extension().is_some()
}
