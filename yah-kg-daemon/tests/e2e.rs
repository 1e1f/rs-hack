//! End-to-end daemon tests.
//!
//! Each test stands up a `KgService` against a tempdir, optionally starts
//! a watcher, then exercises the `arch.*` surface against in-memory state.
//! Watcher tests have generous timeouts to absorb filesystem-event jitter
//! across hosts; if a test hangs, the timeout fires rather than leaving
//! the suite stuck.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::timeout;
use yah_kg::anno::AnnotationKind;
use yah_kg::edge::EdgeKind;
use yah_kg::event::{ArchEvent, IndexReason};
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg::rpc::{Direction, LookupParams, NeighborsParams, RootsParams, SubgraphParams};
use yah_kg_daemon::{DaemonError, KgService};
use yah_kg_rust::RustIndexer;
use yah_kg_store::IndexerRegistry;
use yah_kg_ts::TsIndexer;

const TIMEOUT: Duration = Duration::from_secs(5);

fn registry() -> IndexerRegistry {
    let mut r = IndexerRegistry::new();
    r.register(Box::new(RustIndexer::new()));
    r.register(Box::new(TsIndexer::new()));
    r
}

fn write(dir: &TempDir, rel: &str, body: &str) -> PathBuf {
    let abs = dir.path().join(rel);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&abs, body).unwrap();
    abs
}

async fn wait_for<F: FnMut(&ArchEvent) -> bool>(
    rx: &mut tokio::sync::broadcast::Receiver<ArchEvent>,
    mut pred: F,
) -> ArchEvent {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let now = tokio::time::Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            panic!("timed out waiting for predicate event");
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(ev)) if pred(&ev) => return ev,
            Ok(Ok(_)) => continue,
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(RecvError::Closed)) | Err(_) => {
                panic!("channel closed or timed out before matching event");
            }
        }
    }
}

#[tokio::test]
async fn boot_indexes_initial_tree_and_serves_queries() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "pub fn boot_lib() {}\npub struct Foo;\n",
    );
    write(
        &dir,
        "src/bin.rs",
        "fn main() {}\n",
    );
    write(&dir, "src/types.ts", "export interface Bar { id: string; }\n");

    let svc = KgService::new(registry());
    let summary = svc.boot(dir.path().to_path_buf()).await.unwrap();
    assert_eq!(summary.files_indexed, 3, "all three sources indexed");

    // arch.lookup
    let lib = svc
        .lookup(LookupParams {
            file: "src/lib.rs".into(),
            line: None,
            col: None,
        })
        .await;
    assert!(!lib.ids.is_empty());

    // arch.subgraph from the file
    let mut file_id = None;
    for id in &lib.ids {
        if let Some(full) = svc.node(*id).await {
            if matches!(full.node.kind, NodeKind::Common(CommonKind::File)) {
                file_id = Some(*id);
                break;
            }
        }
    }
    let file_id = file_id.expect("file node");
    let sg = svc
        .subgraph(SubgraphParams {
            root: file_id,
            depth: 2,
            edges: None,
            kinds: None,
            langs: None,
            node_limit: None,
        })
        .await;
    assert!(
        sg.nodes.iter().any(|n| n.label == "Foo"),
        "subgraph should include Foo"
    );

    // arch.stats
    let stats = svc.stats().await;
    assert!(stats.node_count >= 5);
    assert!(stats.last_index_ms.is_some());

    // arch.roots: with no kind filter, the root directory should appear.
    let roots = svc
        .roots(RootsParams {
            lang: None,
            kind: None,
        })
        .await;
    assert!(!roots.roots.is_empty());

    // arch.languages
    let langs = svc.languages();
    assert!(langs.contains(&Lang::Rust));
    assert!(langs.contains(&Lang::Ts));
}

#[tokio::test]
async fn reindex_path_emits_node_added_changed_removed() {
    let dir = tempfile::tempdir().unwrap();
    let lib = write(&dir, "src/lib.rs", "pub fn alpha() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let mut rx = svc.subscribe();

    // Replace `alpha` with `beta` and reindex.
    fs::write(&lib, "pub fn beta() {}\n").unwrap();
    svc.reindex_path(&lib, IndexReason::Manual).await.unwrap();

    // Expect: IndexStarted, NodeAdded(beta), NodeRemoved(alpha) — order may
    // vary for the node events. We assert presence of each type.
    let mut saw_added = false;
    let mut saw_removed = false;
    let mut saw_finished = false;
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    while !saw_finished {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("did not observe IndexFinished within timeout");
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(ev)) => match ev {
                ArchEvent::NodeAdded { node } if node.label == "beta" => saw_added = true,
                ArchEvent::NodeRemoved { .. } => saw_removed = true,
                ArchEvent::IndexFinished { .. } => saw_finished = true,
                _ => {}
            },
            _ => panic!("event channel ended"),
        }
    }
    assert!(saw_added, "missing NodeAdded for beta");
    assert!(saw_removed, "missing NodeRemoved for alpha");
}

#[tokio::test]
async fn reindex_after_disk_delete_wipes_file_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let p = write(&dir, "src/temp.rs", "pub fn gone() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // Confirm `gone` is present.
    let before = svc
        .lookup(LookupParams {
            file: "src/temp.rs".into(),
            line: None,
            col: None,
        })
        .await;
    assert!(!before.ids.is_empty());

    fs::remove_file(&p).unwrap();
    svc.reindex_path(&p, IndexReason::Manual).await.unwrap();

    let after = svc
        .lookup(LookupParams {
            file: "src/temp.rs".into(),
            line: None,
            col: None,
        })
        .await;
    assert!(
        after.ids.is_empty(),
        "deleted file should have no nodes; got {:?}",
        after.ids
    );
}

#[tokio::test]
async fn watcher_picks_up_disk_changes_and_emits_events() {
    let dir = tempfile::tempdir().unwrap();
    let lib = write(&dir, "src/watched.rs", "pub fn first() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let mut rx = svc.subscribe();
    svc.start_watching().await.unwrap();

    // Give the watcher a beat to register.
    tokio::time::sleep(Duration::from_millis(100)).await;

    fs::write(&lib, "pub fn second() {}\npub struct Added;\n").unwrap();

    // Wait for an IndexFinished from FileWatch, signalling our edit landed.
    let ev = wait_for(&mut rx, |e| {
        matches!(
            e,
            ArchEvent::IndexFinished { .. }
        )
    })
    .await;
    let ArchEvent::IndexFinished {
        nodes_added,
        nodes_removed,
        ..
    } = ev
    else {
        unreachable!()
    };
    assert!(
        nodes_added >= 1 && nodes_removed >= 1,
        "expected at least one add and one remove from file edit; got +{nodes_added} -{nodes_removed}"
    );

    // The store now reflects the change.
    let after = svc
        .lookup(LookupParams {
            file: "src/watched.rs".into(),
            line: None,
            col: None,
        })
        .await;
    let mut labels: Vec<String> = Vec::new();
    for id in &after.ids {
        if let Some(n) = svc.node(*id).await {
            labels.push(n.node.label);
        }
    }
    assert!(labels.iter().any(|l| l == "second"));
    assert!(labels.iter().any(|l| l == "Added"));

    svc.stop_watching().await;
}

#[tokio::test]
async fn touch_emits_agent_touch_with_resolved_node_ids() {
    let dir = tempfile::tempdir().unwrap();
    let _lib = write(&dir, "src/lib.rs", "pub fn target_fn() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let mut rx = svc.subscribe();

    // pi-mono would call this with the path:line of a tool result.
    // The function spans line 1, so :1 is sufficient.
    svc.touch(
        &["src/lib.rs:1".to_string()],
        "Read",
        "R007",
    )
    .await;

    let ev = wait_for(&mut rx, |e| matches!(e, ArchEvent::AgentTouch { .. })).await;
    let ArchEvent::AgentTouch {
        ids, tool, relay, ..
    } = ev
    else {
        unreachable!()
    };
    assert_eq!(tool, "Read");
    assert_eq!(relay, "R007");
    assert!(!ids.is_empty(), "should have resolved at least one node");

    // Confirm `target_fn` is among the resolved nodes.
    let mut found_target = false;
    for id in ids {
        if let Some(n) = svc.node(id).await {
            if n.node.label == "target_fn" {
                found_target = true;
                break;
            }
        }
    }
    assert!(found_target, "AgentTouch should resolve to target_fn");
}

#[tokio::test]
async fn reindex_path_before_boot_returns_not_booted() {
    let svc = KgService::new(registry());
    let p = std::env::temp_dir().join("does-not-exist.rs");
    let err = svc
        .reindex_path(&p, IndexReason::Manual)
        .await
        .expect_err("should error before boot");
    assert!(matches!(err, DaemonError::NotBooted));
}

#[tokio::test]
async fn boot_is_idempotent_and_rebinds_to_new_root() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    write(&a, "src/a.rs", "pub fn from_a() {}\n");
    write(&b, "src/b.rs", "pub fn from_b() {}\n");

    let svc = KgService::new(registry());
    svc.boot(a.path().to_path_buf()).await.unwrap();
    let stats_a = svc.stats().await;
    assert!(stats_a.node_count > 0);

    svc.boot(b.path().to_path_buf()).await.unwrap();
    let stats_b = svc.stats().await;
    assert!(stats_b.node_count > 0);

    // After rebinding, A's nodes are gone.
    let from_a = svc
        .lookup(LookupParams {
            file: "src/a.rs".into(),
            line: None,
            col: None,
        })
        .await;
    assert!(
        from_a.ids.is_empty(),
        "boot should wipe state from previous rig"
    );
    let from_b = svc
        .lookup(LookupParams {
            file: "src/b.rs".into(),
            line: None,
            col: None,
        })
        .await;
    assert!(!from_b.ids.is_empty());
}


#[tokio::test]
async fn boot_runs_annotation_pass_and_node_returns_annotations() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        r#"//! @yah:tag(layer:core)

/// @yah:tag(audio, hot-path)
pub struct Mixer;

/// @yah:flow(Mixer, "shared frame buffer")
pub struct Dispatcher;
"#,
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // Find the Mixer node and confirm it has typed annotations on NodeFull.
    let mut mixer = None;
    for id in svc
        .lookup(LookupParams {
            file: "src/lib.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids
    {
        if let Some(n) = svc.node(id).await {
            if n.node.label == "Mixer" {
                mixer = Some(n);
                break;
            }
        }
    }
    let mixer = mixer.expect("Mixer node");
    let tags: Vec<String> = mixer
        .annotations
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Tag(t) => Some(t.label()),
            _ => None,
        })
        .collect();
    assert!(
        tags.contains(&"audio".to_string())
            && tags.contains(&"hot-path".to_string()),
        "Mixer annotations should include audio + hot-path; got {tags:?}"
    );

    // Synthetic Tag node should be reachable via Tag edges from the
    // structural node.
    let tag_edges = svc
        .neighbors(NeighborsParams {
            id: mixer.node.id,
            dir: Direction::Out,
            edges: Some(vec![EdgeKind::Tag]),
        })
        .await;
    let mut tag_labels = Vec::new();
    for e in &tag_edges.edges {
        if let Some(n) = svc.node(e.to).await {
            tag_labels.push(n.node.label);
        }
    }
    assert!(
        tag_labels.contains(&"audio".to_string()),
        "tag node `audio` should be reachable; got {tag_labels:?}"
    );

    // Flow edges resolved within the file: find Dispatcher.
    let mut dispatcher_node = None;
    for id in svc
        .lookup(LookupParams {
            file: "src/lib.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids
    {
        if let Some(n) = svc.node(id).await {
            if n.node.label == "Dispatcher" {
                dispatcher_node = Some(n);
                break;
            }
        }
    }
    let dispatcher = dispatcher_node.expect("Dispatcher node");
    let flow_edges = svc
        .neighbors(NeighborsParams {
            id: dispatcher.node.id,
            dir: Direction::Out,
            edges: Some(vec![EdgeKind::Flow]),
        })
        .await;
    assert_eq!(
        flow_edges.edges.len(),
        1,
        "Dispatcher should have one flow edge"
    );
    let flow_target = svc.node(flow_edges.edges[0].to).await.unwrap();
    assert_eq!(flow_target.node.label, "Mixer");
}

#[tokio::test]
async fn reindex_path_refreshes_annotations() {
    let dir = tempfile::tempdir().unwrap();
    let lib = write(
        &dir,
        "src/lib.rs",
        "/// @yah:tag(audio)\npub struct Mixer;\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // Sanity: Mixer is tagged audio.
    let mut mixer_id = None;
    for id in svc
        .lookup(LookupParams {
            file: "src/lib.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids
    {
        if let Some(n) = svc.node(id).await {
            if n.node.label == "Mixer" {
                mixer_id = Some(id);
                break;
            }
        }
    }
    let mixer_id = mixer_id.unwrap();
    let before = svc.node(mixer_id).await.unwrap();
    let before_tags: Vec<String> = before
        .annotations
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Tag(t) => Some(t.label()),
            _ => None,
        })
        .collect();
    assert_eq!(before_tags, vec!["audio".to_string()]);

    // Edit the file: change `audio` → `view`.
    fs::write(&lib, "/// @yah:tag(view)\npub struct Mixer;\n").unwrap();
    svc.reindex_path(&lib, IndexReason::Manual).await.unwrap();

    let after = svc.node(mixer_id).await.unwrap();
    let after_tags: Vec<String> = after
        .annotations
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Tag(t) => Some(t.label()),
            _ => None,
        })
        .collect();
    assert_eq!(
        after_tags,
        vec!["view".to_string()],
        "annotation should refresh on reindex; got {after_tags:?}"
    );

    // Old `audio` tag edge should no longer exist on Mixer.
    let tag_edges = svc
        .neighbors(NeighborsParams {
            id: mixer_id,
            dir: Direction::Out,
            edges: Some(vec![EdgeKind::Tag]),
        })
        .await;
    let mut labels = Vec::new();
    for e in &tag_edges.edges {
        if let Some(n) = svc.node(e.to).await {
            labels.push(n.node.label);
        }
    }
    assert_eq!(
        labels,
        vec!["view".to_string()],
        "old audio tag edge should be gone"
    );
}
