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
use yah_kg::anno::{AnnotationKind, TicketStatus};
use yah_kg::edge::EdgeKind;
use yah_kg::event::{ArchEvent, FileEvent, FileEventKind, IndexReason};
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg::prompt::PromptMode;
use yah_kg::rpc::{
    ArchiveTicketParams, AssemblePreludeParams, DirEntryKind, DirListParams, DirWatchParams,
    Direction, FileEncoding, FileReadParams, FileReadRange, FileWatchParams, FileWriteParams,
    GetTicketParams, ListAuthoredFilesParams, ListRelaysParams, ListTicketsParams, LookupParams,
    MoveTicketParams, NeighborsParams, ReadAuthoredFileParams, RootsParams, SubgraphParams,
    TicketPromptParams, UnwatchParams, WatchResult,
};
use yah_kg_daemon::{default_snapshot_path, DaemonError, KgService};
use yah_kg_json_yaml::{JsonIndexer, TomlIndexer, YamlIndexer};
use yah_kg_rust::RustIndexer;
use yah_kg_store::IndexerRegistry;
use yah_kg_ts::TsIndexer;

const TIMEOUT: Duration = Duration::from_secs(5);

fn registry() -> IndexerRegistry {
    let mut r = IndexerRegistry::new();
    r.register(Box::new(RustIndexer::new()));
    r.register(Box::new(TsIndexer::new()));
    r.register(Box::new(JsonIndexer::new()));
    r.register(Box::new(YamlIndexer::new()));
    r.register(Box::new(TomlIndexer::new()));
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
async fn boot_emits_relay_and_ticket_changed_for_work_items() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        r#"//! @yah:relay(R042, "Sample")
//! @yah:status(in-progress)
//!
//! @yah:ticket(R042-T1, "Sub")
//! @yah:status(open)
//! @yah:parent(R042)

pub fn carrier() {}
"#,
    );

    let svc = KgService::new(registry());
    let mut rx = svc.subscribe();
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let mut saw_relay = false;
    let mut saw_ticket = false;
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    while !(saw_relay && saw_ticket) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("did not observe both RelayChanged and TicketChanged");
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(ArchEvent::RelayChanged { work_item_id, .. })) if work_item_id == "R042" => {
                saw_relay = true;
            }
            Ok(Ok(ArchEvent::TicketChanged { work_item_id, .. })) if work_item_id == "R042-T1" => {
                saw_ticket = true;
            }
            Ok(Ok(_)) => continue,
            _ => panic!("event channel closed"),
        }
    }

    // Synthetic ticket node is reachable via the daemon's `node` query.
    let ticket_id = yah_kg::ids::NodeId::compute(
        Lang::Rust,
        "ticket:R042-T1",
        "<work-item>",
    );
    let ticket_full = svc.node(ticket_id).await.expect("synthetic ticket node");
    assert!(matches!(
        ticket_full.node.kind,
        NodeKind::Common(CommonKind::Ticket)
    ));
    let parents = svc
        .neighbors(NeighborsParams {
            id: ticket_id,
            dir: Direction::Out,
            edges: Some(vec![EdgeKind::ParentItem]),
        })
        .await;
    assert_eq!(parents.edges.len(), 1, "ticket should parent its relay");
    let parent_node = svc.node(parents.edges[0].to).await.unwrap();
    assert_eq!(parent_node.node.qualified, "relay:R042");
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

const WORK_ITEM_SRC: &str = r#"
//! @yah:relay(R042, "Sample relay")
//! @yah:status(in-progress)
//! @yah:assignee(agent:claude)
//! @yah:phase(P1)
//! @yah:next("Land the first pass")
//!
//! @yah:ticket(R042-T1, "Sub-task")
//! @yah:status(open)
//! @yah:parent(R042)
//! @yah:kind(bug)
//! @yah:verify("cargo test")

pub fn carrier() {}
"#;

#[tokio::test]
async fn list_tickets_returns_authored_tickets_with_anchors_and_payload() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_tickets(ListTicketsParams::default()).await;
    assert_eq!(
        result.tickets.len(),
        1,
        "exactly one Ticket node was authored; got {:?}",
        result
            .tickets
            .iter()
            .map(|t| &t.id)
            .collect::<Vec<_>>()
    );
    let ticket = &result.tickets[0];
    assert_eq!(ticket.id, "R042-T1");
    assert_eq!(ticket.anno.title, "Sub-task");
    assert_eq!(ticket.anno.parent.as_deref(), Some("R042"));
    assert_eq!(ticket.anno.kind.as_deref(), Some("bug"));
    assert_eq!(ticket.anno.verify, vec!["cargo test".to_string()]);
    assert_eq!(ticket.anchors.len(), 1, "one source anchor");
    assert_eq!(ticket.anchors[0].file, "src/lib.rs");
    assert!(ticket.anchors[0].line >= 1);
}

#[tokio::test]
async fn list_relays_skips_stub_only_parent_references() {
    // The relay's @yah:parent(R013) creates a stub node for R013, but R013
    // itself is never authored. The list should surface only R042.
    const WITH_PARENT: &str = r#"
//! @yah:relay(R042, "Sample relay")
//! @yah:status(in-progress)
//! @yah:parent(R013)

pub fn carrier() {}
"#;
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WITH_PARENT);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_relays(ListRelaysParams::default()).await;
    let ids: Vec<&str> = result.relays.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["R042"],
        "stub R013 has no anchors and must not surface; got {ids:?}"
    );
    let relay = &result.relays[0];
    assert_eq!(relay.anno.title, "Sample relay");
    assert_eq!(relay.anno.parent.as_deref(), Some("R013"));
}

#[tokio::test]
async fn get_ticket_round_trips_by_bare_id() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let hit = svc
        .get_ticket(GetTicketParams {
            id: "R042-T1".to_string(),
        })
        .await;
    let ticket = hit.ticket.expect("R042-T1 should be present");
    assert_eq!(ticket.id, "R042-T1");
    assert_eq!(ticket.anno.title, "Sub-task");

    let miss = svc
        .get_ticket(GetTicketParams {
            id: "R999-T9".to_string(),
        })
        .await;
    assert!(miss.ticket.is_none());

    // Asking for a bare relay id through get_ticket must not match.
    let relay_miss = svc
        .get_ticket(GetTicketParams {
            id: "R042".to_string(),
        })
        .await;
    assert!(
        relay_miss.ticket.is_none(),
        "get_ticket must not return relays"
    );
}

#[tokio::test]
async fn last_modified_ts_prefers_event_shard_over_mtime() {
    // R042-T1 belongs to shard R042 (sub-tickets live in their parent's
    // shard). The newest matching `t` in the shard should win over the
    // source file's mtime.
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);
    let shard = dir.path().join(".yah").join("events");
    std::fs::create_dir_all(&shard).unwrap();
    std::fs::write(
        shard.join("R042.jsonl"),
        // Older event for the relay, then a newer event for the ticket.
        "{\"t\":1700000000,\"type\":\"scan\",\"id\":\"R042\"}\n\
         {\"t\":1800000000,\"type\":\"scan\",\"id\":\"R042-T1\"}\n\
         {\"t\":1750000000,\"type\":\"scan\",\"id\":\"R042-T1\"}\n",
    )
    .unwrap();

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let hit = svc
        .get_ticket(GetTicketParams {
            id: "R042-T1".to_string(),
        })
        .await;
    let ticket = hit.ticket.expect("R042-T1 should be present");
    assert_eq!(
        ticket.last_modified_ts, 1800000000,
        "newest matching `t` from .yah/events/R042.jsonl wins"
    );
}

#[tokio::test]
async fn ticket_prompt_renders_pickup_via_shared_renderer() {
    // The daemon's `arch.ticket_prompt` and the CLI's `yah board show
    // <id> --prompt` both flow through `yah_kg::prompt::render`, so for a
    // workspace with the same source the markdown must be byte-identical.
    // Here we just sanity-check the daemon path produces the canonical
    // sections the CLI tests have always asserted on (sub-tickets +
    // Rule08, parent inheritance, archive/Col01 footer).
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // R042 is the relay with one live sub-ticket (R042-T1) — Rule08 applies
    // and the "Sub-tickets in flight" header should render.
    let pickup = svc
        .ticket_prompt(TicketPromptParams {
            id: "R042".into(),
            mode: PromptMode::Pickup,
        })
        .await;
    let md = pickup.markdown.expect("R042 should be on the board");
    assert!(md.starts_with("# Continue: R042 — Sample relay"), "{md}");
    assert!(md.contains("## Sub-tickets in flight"), "{md}");
    assert!(md.contains("Rule08"), "{md}");
    assert!(
        md.contains("yah board claim R042-T1"),
        "earliest live child is R042-T1 (status open) → claim verb:\n{md}"
    );
    assert!(md.contains("Defined at `src/lib.rs:"), "{md}");

    // Review mode for the sub-ticket renders the verifier framing.
    let review = svc
        .ticket_prompt(TicketPromptParams {
            id: "R042-T1".into(),
            mode: PromptMode::Review,
        })
        .await;
    let r = review.markdown.expect("R042-T1 should be on the board");
    assert!(r.starts_with("# Review: R042-T1 — Sub-task"), "{r}");
    assert!(r.contains("## Decide"), "{r}");
    assert!(r.contains("yah board move R042-T1 handoff"), "{r}");

    // Unknown id returns markdown:None — UI surfaces this as a transient miss.
    let miss = svc
        .ticket_prompt(TicketPromptParams {
            id: "R999-T9".into(),
            mode: PromptMode::Pickup,
        })
        .await;
    assert!(miss.markdown.is_none(), "unknown id should return None");
}

#[tokio::test]
async fn assemble_prelude_builds_a_cached_prefix_for_known_ticket() {
    // R028-F3: the daemon assembles the per-ticket cached prefix the
    // SDK pane injects on every turn. Backed by yah_kg::prelude::assemble
    // so the unit-tested rendering already pins the markdown shape — the
    // daemon's job is just to feed it board + KG slice + arch docs.
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc
        .assemble_prelude(AssemblePreludeParams::new("R042-T1"))
        .await;
    let prelude = result
        .prelude
        .expect("R042-T1 should produce a prelude");
    let rendered = prelude.render();

    assert!(
        rendered.starts_with("# Ticket: R042-T1 — Sub-task"),
        "ticket section is the cached prefix's leading block:\n{rendered}"
    );
    assert!(
        rendered.contains("## Parent chain"),
        "R042-T1 has @yah:parent(R042) — the assembler walks the chain:\n{rendered}"
    );
    assert!(
        prelude.estimated_tokens > 0,
        "estimated_tokens should be non-zero for a non-empty prelude"
    );
    assert_eq!(prelude.cache.key.len(), 32, "cache key is 32 hex chars");

    // Unknown id returns prelude:None — same null-when-missing as ticket_prompt.
    let miss = svc
        .assemble_prelude(AssemblePreludeParams::new("R999-T9"))
        .await;
    assert!(miss.prelude.is_none(), "unknown id should return None");
}

#[tokio::test]
async fn last_modified_ts_falls_back_to_source_mtime() {
    // No shard exists, so the daemon should stat the lex-first anchor's
    // file and surface its mtime in unix seconds. We don't pin an exact
    // value (filesystem mtime resolution varies), only that it's non-zero
    // and within a sane window of "now".
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", WORK_ITEM_SRC);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_tickets(ListTicketsParams::default()).await;
    let ticket = &result.tickets[0];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        ticket.last_modified_ts > 0 && ticket.last_modified_ts <= now,
        "fallback mtime should be a recent unix-seconds value; got {}",
        ticket.last_modified_ts
    );
}

// ---------- R017-F3: snapshot persistence + mtime-diff reconcile ----------

#[tokio::test]
async fn save_then_load_round_trips_store_and_annotations() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "/// @yah:tag(audio)\npub struct Mixer;\n",
    );
    let snap_path = dir.path().join(".yah/cache/snapshot.bin");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    let stats_before = svc.stats().await;
    svc.save(&snap_path).await.unwrap();
    assert!(snap_path.is_file(), "snapshot file should exist after save");

    // Fresh service, never booted. `load` alone restores state.
    let svc2 = KgService::new(registry());
    svc2.load(&snap_path).await.unwrap();
    let stats_after = svc2.stats().await;
    assert_eq!(stats_before.node_count, stats_after.node_count);
    assert_eq!(stats_before.edge_count, stats_after.edge_count);

    // Annotation overlay survived: Mixer should still carry the audio tag.
    let mut mixer = None;
    for id in svc2
        .lookup(LookupParams {
            file: "src/lib.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids
    {
        if let Some(n) = svc2.node(id).await {
            if n.node.label == "Mixer" {
                mixer = Some(n);
                break;
            }
        }
    }
    let mixer = mixer.expect("Mixer node should round-trip through snapshot");
    let tags: Vec<String> = mixer
        .annotations
        .iter()
        .filter_map(|a| match &a.kind {
            AnnotationKind::Tag(t) => Some(t.label()),
            _ => None,
        })
        .collect();
    assert_eq!(tags, vec!["audio".to_string()]);
}

#[tokio::test]
async fn boot_with_snapshot_falls_back_to_full_boot_when_no_file() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", "pub fn boot_lib() {}\n");

    let svc = KgService::new(registry());
    let snap_path = dir.path().join(".yah/cache/snapshot.bin");
    let summary = svc
        .boot_with_snapshot(dir.path().to_path_buf(), &snap_path)
        .await
        .unwrap();
    // Fell through to boot(); summary should reflect a real walk.
    assert!(summary.files_indexed >= 1);
    let stats = svc.stats().await;
    assert!(stats.node_count > 0);
}

#[tokio::test]
async fn boot_with_snapshot_skips_unchanged_files_and_reindexes_changed_ones() {
    let dir = tempfile::tempdir().unwrap();
    let stable = write(&dir, "src/stable.rs", "pub fn untouched() {}\n");
    let edited = write(&dir, "src/edited.rs", "pub fn before() {}\n");
    let _ = stable;
    let snap_path = dir.path().join(".yah/cache/snapshot.bin");

    // Phase 1: boot fresh, save snapshot.
    let svc1 = KgService::new(registry());
    svc1.boot(dir.path().to_path_buf()).await.unwrap();
    svc1.save(&snap_path).await.unwrap();

    // Edit one file while the daemon is "offline". Sleep long enough that
    // mtime granularity (1s on some filesystems) actually advances.
    std::thread::sleep(Duration::from_millis(1100));
    fs::write(&edited, "pub fn after() {}\npub struct Added;\n").unwrap();

    // Phase 2: cold-start a fresh service, boot via snapshot.
    let svc2 = KgService::new(registry());
    let summary = svc2
        .boot_with_snapshot(dir.path().to_path_buf(), &snap_path)
        .await
        .unwrap();

    // The reconcile touched exactly the edited file.
    assert_eq!(
        summary.files_indexed, 1,
        "only the edited file should be reindexed; got {summary:?}"
    );

    // The store reflects the edit: `before` is gone, `after` + `Added` exist.
    let labels: Vec<String> = {
        let ids = svc2
            .lookup(LookupParams {
                file: "src/edited.rs".into(),
                line: None,
                col: None,
            })
            .await
            .ids;
        let mut out = Vec::new();
        for id in ids {
            if let Some(n) = svc2.node(id).await {
                out.push(n.node.label);
            }
        }
        out
    };
    assert!(labels.iter().any(|l| l == "after"), "got {labels:?}");
    assert!(labels.iter().any(|l| l == "Added"), "got {labels:?}");
    assert!(!labels.iter().any(|l| l == "before"), "got {labels:?}");

    // The untouched file is still served.
    let stable_ids = svc2
        .lookup(LookupParams {
            file: "src/stable.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids;
    assert!(!stable_ids.is_empty(), "unchanged file should still resolve");
}

#[tokio::test]
async fn boot_with_snapshot_clean_replay_reindexes_zero_files() {
    // Verify gate for R017-F3: a snapshot replay over an unchanged tree
    // touches the filesystem to fingerprint but reindexes nothing — the
    // mechanism behind "cold boot order-of-magnitude faster than full
    // reindex". R017-T7 measures wall-clock end-to-end via Tauri; this
    // test pins the daemon-level invariant.
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/a.rs", "pub fn a() {}\n");
    write(&dir, "src/b.rs", "pub fn b() {}\npub struct Bee;\n");
    write(&dir, "src/sub/c.rs", "/// @yah:tag(audio)\npub struct Cee;\n");
    let snap_path = dir.path().join(".yah/cache/snapshot.bin");

    let svc1 = KgService::new(registry());
    let walked = svc1.boot(dir.path().to_path_buf()).await.unwrap();
    let stats_before = svc1.stats().await;
    svc1.save(&snap_path).await.unwrap();
    assert!(walked.files_indexed >= 3);

    let svc2 = KgService::new(registry());
    let summary = svc2
        .boot_with_snapshot(dir.path().to_path_buf(), &snap_path)
        .await
        .unwrap();

    assert_eq!(
        summary.files_indexed, 0,
        "clean replay should reindex zero files; got {summary:?}"
    );
    assert_eq!(summary.files_skipped as usize, walked.files_indexed as usize);

    let stats_after = svc2.stats().await;
    assert_eq!(stats_before.node_count, stats_after.node_count);
    assert_eq!(stats_before.edge_count, stats_after.edge_count);
}

#[tokio::test]
async fn boot_with_snapshot_drops_files_deleted_offline() {
    let dir = tempfile::tempdir().unwrap();
    let _kept = write(&dir, "src/kept.rs", "pub fn kept() {}\n");
    let removed = write(&dir, "src/removed.rs", "pub fn going() {}\n");
    let snap_path = dir.path().join(".yah/cache/snapshot.bin");

    let svc1 = KgService::new(registry());
    svc1.boot(dir.path().to_path_buf()).await.unwrap();
    svc1.save(&snap_path).await.unwrap();

    fs::remove_file(&removed).unwrap();

    let svc2 = KgService::new(registry());
    let summary = svc2
        .boot_with_snapshot(dir.path().to_path_buf(), &snap_path)
        .await
        .unwrap();
    assert_eq!(summary.files_indexed, 1, "only the deletion should reconcile");

    let gone = svc2
        .lookup(LookupParams {
            file: "src/removed.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids;
    assert!(gone.is_empty(), "deleted file should have no nodes");
}

#[tokio::test]
async fn boot_with_snapshot_rejects_mismatched_rig_root_and_falls_back() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    write(&dir_a, "src/a.rs", "pub fn from_a() {}\n");
    write(&dir_b, "src/b.rs", "pub fn from_b() {}\n");

    // Save a snapshot from rig A.
    let snap_path = dir_a.path().join(".yah/cache/snapshot.bin");
    let svc1 = KgService::new(registry());
    svc1.boot(dir_a.path().to_path_buf()).await.unwrap();
    svc1.save(&snap_path).await.unwrap();

    // Boot a new service against rig B but pointed at A's snapshot. The
    // rig_root mismatch should trigger a full boot of B, not a restore
    // of A's state.
    let svc2 = KgService::new(registry());
    svc2.boot_with_snapshot(dir_b.path().to_path_buf(), &snap_path)
        .await
        .unwrap();
    let from_a = svc2
        .lookup(LookupParams {
            file: "src/a.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids;
    assert!(
        from_a.is_empty(),
        "snapshot from rig A must not leak into rig B"
    );
    let from_b = svc2
        .lookup(LookupParams {
            file: "src/b.rs".into(),
            line: None,
            col: None,
        })
        .await
        .ids;
    assert!(!from_b.is_empty(), "rig B should be freshly walked");
}

#[tokio::test]
async fn move_ticket_rewrites_status_in_source_and_emits_agent_edit_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        &dir,
        "src/lib.rs",
        "//! @yah:ticket(R900-T1, \"x\")\n//! @yah:status(open)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let mut rx = svc.subscribe();
    let result = svc
        .move_ticket(MoveTicketParams {
            id: "R900-T1".into(),
            to_bucket: "active".into(),
        })
        .await
        .unwrap();
    assert_eq!(result.from_status, "open");
    assert_eq!(result.to_status, "in-progress");
    assert_eq!(result.file, "src/lib.rs");

    let body = fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("@yah:status(in-progress)"),
        "rewritten file should carry the new status, got:\n{body}"
    );
    assert!(
        !body.contains("@yah:status(open)"),
        "rewritten file should not still hold the old status"
    );

    // The reindex_path call inside move_ticket should fan out an
    // IndexStarted event with reason=AgentEdit.
    let ev = wait_for(&mut rx, |e| {
        matches!(
            e,
            ArchEvent::IndexStarted {
                reason: IndexReason::AgentEdit,
                ..
            }
        )
    })
    .await;
    assert!(matches!(
        ev,
        ArchEvent::IndexStarted {
            reason: IndexReason::AgentEdit,
            ..
        }
    ));
}

#[tokio::test]
async fn move_ticket_rejects_disallowed_transition() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:ticket(R901-T1, \"x\")\n//! @yah:status(open)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // open → review is not on the matrix (only open → active is).
    let err = svc
        .move_ticket(MoveTicketParams {
            id: "R901-T1".into(),
            to_bucket: "review".into(),
        })
        .await
        .unwrap_err();
    assert!(
        matches!(err, DaemonError::Conflict(ref m) if m.contains("not allowed")),
        "expected a transition Conflict, got {err:?}"
    );
}

#[tokio::test]
async fn move_ticket_rejects_unknown_bucket_and_missing_id() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:ticket(R902-T1, \"x\")\n//! @yah:status(open)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let err = svc
        .move_ticket(MoveTicketParams {
            id: "R902-T1".into(),
            to_bucket: "shipped".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, DaemonError::Conflict(ref m) if m.contains("unknown target column")));

    let err = svc
        .move_ticket(MoveTicketParams {
            id: "R-DOES-NOT-EXIST".into(),
            to_bucket: "active".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, DaemonError::Conflict(ref m) if m.contains("not on the board")));
}

#[tokio::test]
async fn move_ticket_refuses_to_mutate_an_epic() {
    let dir = tempfile::tempdir().unwrap();
    // Parent relay declared as an epic, with one child relay so the
    // board derives epic-ness even without the explicit kind tag.
    write(
        &dir,
        "src/parent.rs",
        "//! @yah:relay(R903, \"parent\")\n//! @yah:kind(epic)\n//! @yah:status(in-progress)\n",
    );
    write(
        &dir,
        "src/child.rs",
        "//! @yah:relay(R904, \"child\")\n//! @yah:status(in-progress)\n//! @yah:parent(R903)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let err = svc
        .move_ticket(MoveTicketParams {
            id: "R903".into(),
            to_bucket: "review".into(),
        })
        .await
        .unwrap_err();
    assert!(
        matches!(err, DaemonError::Conflict(ref m) if m.contains("epic")),
        "expected an epic Conflict, got {err:?}"
    );
}

#[tokio::test]
async fn move_ticket_writes_yah_at_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        &dir,
        "src/lib.rs",
        "//! @yah:ticket(R905-T1, \"x\")\n//! @yah:status(open)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    svc.move_ticket(MoveTicketParams {
        id: "R905-T1".into(),
        to_bucket: "active".into(),
    })
    .await
    .unwrap();

    let body = fs::read_to_string(&path).unwrap();
    let at_line = body
        .lines()
        .find(|l| l.contains("@yah:at("))
        .expect("status change should stamp @yah:at, got:\n{body}");
    // Extract the value between parens and assert it parses as RFC 3339 UTC.
    let lparen = at_line.find('(').unwrap();
    let rparen = at_line.rfind(')').unwrap();
    let val = &at_line[lparen + 1..rparen];
    assert!(
        yah_kg::timefmt::parse_rfc3339(val).is_some(),
        "@yah:at value {val:?} should parse as RFC 3339"
    );

    // Re-running with the same target column is a no-op and must NOT
    // bump the timestamp — would otherwise churn on every UI re-render.
    svc.move_ticket(MoveTicketParams {
        id: "R905-T1".into(),
        to_bucket: "active".into(),
    })
    .await
    .unwrap();
    let body2 = fs::read_to_string(&path).unwrap();
    assert_eq!(body, body2, "no-op move should not rewrite source");
}

#[tokio::test]
async fn archive_ticket_strips_yah_lines_and_writes_event_shard() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R901, \"to-archive\")\n\
         //! @yah:status(review)\n\
         //! @arch:see(architecture/foo.md)\n\
         //! preserved doc text\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc
        .archive_ticket(ArchiveTicketParams { id: "R901".into() })
        .await
        .unwrap();
    assert_eq!(result.id, "R901");
    assert_eq!(result.file, "src/lib.rs");
    assert_eq!(result.removed_lines, 2);

    // Source: only @yah lines stripped; @arch and plain doc preserved.
    let body = fs::read_to_string(&path).unwrap();
    assert!(!body.contains("@yah:"), "all @yah lines should be stripped, got:\n{body}");
    assert!(body.contains("@arch:see"), "non-@yah doc lines should survive, got:\n{body}");
    assert!(body.contains("preserved doc text"));

    // Event shard: one archived event with full snapshot + sourceLines.
    let shard = dir.path().join(".yah").join("events").join("R901.jsonl");
    let raw = fs::read_to_string(&shard).expect("event shard should exist");
    let line = raw.lines().next().expect("at least one event line");
    let event: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(event["type"], "archived");
    assert_eq!(event["id"], "R901");
    assert!(event["t"].as_u64().unwrap() > 0);
    assert_eq!(event["file"], "src/lib.rs");
    assert!(event["ticket"]["id"] == "R901");
    let lines = event["sourceLines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].as_str().unwrap().contains("@yah:relay(R901"));
}

#[tokio::test]
async fn archive_ticket_routes_subticket_to_parent_shard() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R902, \"parent\")\n\
         //! @yah:status(in-progress)\n\
         \n\
         /// @yah:ticket(R902-T1, \"subtask\")\n\
         /// @yah:status(review)\n\
         /// @yah:parent(R902)\n\
         pub fn placeholder() {}\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    svc.archive_ticket(ArchiveTicketParams { id: "R902-T1".into() })
        .await
        .unwrap();

    // Sub-ticket events live in the parent's shard, not their own.
    let parent_shard = dir.path().join(".yah").join("events").join("R902.jsonl");
    assert!(parent_shard.exists(), "parent shard should hold the sub-ticket archive");
    let own_shard = dir.path().join(".yah").join("events").join("R902-T1.jsonl");
    assert!(!own_shard.exists(), "sub-ticket should not write to its own shard");
}

#[tokio::test]
async fn archive_ticket_rejects_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:ticket(R900-T2, \"x\")\n//! @yah:status(in-progress)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let err = svc
        .archive_ticket(ArchiveTicketParams { id: "R900-T2".into() })
        .await
        .unwrap_err();
    assert!(
        matches!(err, DaemonError::Conflict(ref m) if m.contains("in-progress")),
        "expected in-progress Conflict, got {err:?}"
    );
}

#[tokio::test]
async fn archive_ticket_rejects_epic_with_live_children() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/parent.rs",
        "//! @yah:relay(R903, \"epic\")\n//! @yah:kind(epic)\n//! @yah:status(review)\n",
    );
    write(
        &dir,
        "src/child.rs",
        "//! @yah:relay(R904, \"child\")\n//! @yah:status(review)\n//! @yah:parent(R903)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let err = svc
        .archive_ticket(ArchiveTicketParams { id: "R903".into() })
        .await
        .unwrap_err();
    assert!(
        matches!(err, DaemonError::Conflict(ref m) if m.contains("child")),
        "expected epic-with-children Conflict, got {err:?}"
    );
}

/// R017-T7 verify gate: cold boot via snapshot beats a full reindex on a
/// real-sized rig (rs-hack itself).
///
/// `#[ignore]` because it walks the whole workspace — slow for default
/// `cargo test` runs and brittle in CI sandboxes that don't ship the
/// full source tree. Run on demand with:
///
/// ```ignore
/// cargo test -p yah-kg-daemon --test e2e --release \
///     boot_with_snapshot_is_faster_than_full_boot -- --ignored --nocapture
/// ```
///
/// **Measured against rs-hack post-T7 (335 indexable / 1899 walked, release):**
/// snapshot replay should comfortably beat full boot. Phase breakdown
/// under `YAH_SNAPSHOT_DEBUG=1`: postcard deserialize, `Store::restore`
/// (bulk `rebuild_from_parts` — pre-allocated, no per-node dedupe),
/// fingerprint walk + diff. v3 swapped from rmp-serde named to postcard
/// + snapshot-only wire types that re-derive serde without
/// `skip_serializing_if` and with externally-tagged enums; the previous
/// ~70ms parse phase is the target this test sentinels.
///
/// We assert only `snap_ms < full_ms` here so the test acts as a
/// regression sentinel — sharper thresholds belong on a follow-up.
#[tokio::test]
#[ignore]
async fn boot_with_snapshot_is_faster_than_full_boot() {
    let workspace = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .expect("CARGO_MANIFEST_DIR is set under cargo test")
        .parent()
        .expect("yah-kg-daemon lives one level under the workspace root")
        .to_path_buf();

    // Persist the snapshot in a tempdir so we don't pollute the rig's
    // own .yah/cache between runs.
    let snap_dir = tempfile::tempdir().unwrap();
    let snap_path = snap_dir.path().join("snapshot.bin");

    // Pass 1 — full boot, save snapshot.
    let svc = KgService::new(registry());
    let t0 = std::time::Instant::now();
    let summary = svc.boot(workspace.clone()).await.unwrap();
    let full_ms = t0.elapsed().as_millis();
    svc.save(&snap_path).await.unwrap();
    eprintln!(
        "full boot: files_seen={} files_indexed={} duration={}ms",
        summary.files_seen, summary.files_indexed, full_ms
    );

    // Pass 2 — fresh service, replay from snapshot.
    let svc2 = KgService::new(registry());
    let t1 = std::time::Instant::now();
    let summary2 = svc2
        .boot_with_snapshot(workspace.clone(), &snap_path)
        .await
        .unwrap();
    let snap_ms = t1.elapsed().as_millis();
    eprintln!(
        "snapshot boot: files_seen={} files_indexed={} duration={}ms",
        summary2.files_seen, summary2.files_indexed, snap_ms
    );

    // The whole point: replay should reindex zero files (nothing
    // changed between the two passes in this same process).
    assert_eq!(
        summary2.files_indexed, 0,
        "clean replay should not reindex anything; saw {} files reindexed",
        summary2.files_indexed
    );

    // Regression sentinel: snapshot replay must be strictly faster
    // than a full reindex. Sharper bounds wait on the postcard /
    // streaming-restore follow-up.
    assert!(
        snap_ms < full_ms,
        "snapshot boot {}ms should be faster than full boot {}ms",
        snap_ms,
        full_ms
    );
    eprintln!(
        "speedup: {:.2}x (full {}ms vs snapshot {}ms)",
        full_ms as f64 / snap_ms.max(1) as f64,
        full_ms,
        snap_ms
    );

    // Sanity: default_snapshot_path lands inside the rig's .yah/cache,
    // matching what the Tauri host wires in arch_open_rig.
    let default = default_snapshot_path(&workspace);
    assert!(
        default.ends_with(".yah/cache/snapshot.bin"),
        "default_snapshot_path should mirror the rig-relative cache layout, got {}",
        default.display()
    );
}

#[tokio::test]
async fn list_authored_files_returns_empty_when_directory_missing() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc
        .list_authored_files(ListAuthoredFilesParams::default())
        .await
        .expect("missing directory is a normal empty state, not an error");
    assert!(result.files.is_empty());
}

#[tokio::test]
async fn list_authored_files_lists_mmd_and_md_and_round_trips_via_read() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        ".yah/arch/authored/topology.mmd",
        "flowchart LR\n  A --> B\n",
    );
    write(
        &dir,
        ".yah/arch/authored/notes.md",
        "# arch notes\n\n```mermaid\ngraph LR; A-->B\n```\n",
    );
    write(
        &dir,
        ".yah/arch/authored/skip.txt",
        "not a renderer artifact\n",
    );
    write(
        &dir,
        ".yah/arch/authored/sub/nested.mmd",
        "graph TB\n  X --> Y\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let listing = svc
        .list_authored_files(ListAuthoredFilesParams::default())
        .await
        .unwrap();
    let names: Vec<&str> = listing.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["notes", "sub/nested", "topology"],
        ".mmd and .md surface together, sorted by name; nested subdirs fold into the name; .txt skipped"
    );

    // Round-trip: pick the .mmd rel_path the daemon handed back and read it.
    let topology = listing
        .files
        .iter()
        .find(|f| f.name == "topology")
        .unwrap();
    let read = svc
        .read_authored_file(ReadAuthoredFileParams {
            rel_path: topology.rel_path.clone(),
        })
        .await
        .unwrap();
    assert!(read.content.starts_with("flowchart LR"));
    assert_eq!(read.bytes, read.content.len() as u64);

    // Round-trip the .md too — the renderer is what specializes mermaid
    // fences, but the daemon must hand the raw text back unchanged.
    let notes = listing.files.iter().find(|f| f.name == "notes").unwrap();
    let md = svc
        .read_authored_file(ReadAuthoredFileParams {
            rel_path: notes.rel_path.clone(),
        })
        .await
        .unwrap();
    assert!(md.content.contains("```mermaid"));
}

#[tokio::test]
async fn list_relays_derives_status_from_children() {
    // R017 has @yah:status(open) but a child sub-ticket in-progress —
    // the derived status should be in-progress, mirroring the desktop UI.
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R017, \"parent\")\n\
         //! @yah:status(open)\n\n\
         /// @yah:ticket(R017-T1, \"child\")\n\
         /// @yah:status(in-progress)\n\
         /// @yah:parent(R017)\n\
         pub fn child() {}\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_relays(ListRelaysParams::default()).await;
    let r017 = result
        .relays
        .iter()
        .find(|r| r.id == "R017")
        .expect("R017 in list_relays");
    assert_eq!(
        r017.anno.status,
        Some(TicketStatus::InProgress),
        "child in-progress should bubble up; got {:?}",
        r017.anno.status
    );
}

#[tokio::test]
async fn list_relays_preserves_source_status_when_no_children() {
    // A childless relay's source-authored status should round-trip.
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R042, \"alone\")\n//! @yah:status(handoff)\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_relays(ListRelaysParams::default()).await;
    let r042 = result
        .relays
        .iter()
        .find(|r| r.id == "R042")
        .expect("R042 in list_relays");
    assert_eq!(r042.anno.status, Some(TicketStatus::Handoff));
}

#[tokio::test]
async fn list_relays_rolls_up_last_modified_ts_across_descendants() {
    // R042 has no shard, so its own ts falls back to mtime. R042-T1 has
    // an event in shard R042.jsonl with a far-future timestamp — the
    // rolled-up relay ts should pick it up.
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R042, \"parent\")\n\
         //! @yah:status(open)\n\n\
         /// @yah:ticket(R042-T1, \"child\")\n\
         /// @yah:status(open)\n\
         /// @yah:parent(R042)\n\
         pub fn child() {}\n",
    );
    let shard = dir.path().join(".yah").join("events");
    fs::create_dir_all(&shard).unwrap();
    fs::write(
        shard.join("R042.jsonl"),
        "{\"t\":1900000000,\"type\":\"scan\",\"id\":\"R042-T1\"}\n",
    )
    .unwrap();

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let result = svc.list_relays(ListRelaysParams::default()).await;
    let r042 = result
        .relays
        .iter()
        .find(|r| r.id == "R042")
        .expect("R042 in list_relays");
    assert_eq!(
        r042.last_modified_ts, 1900000000,
        "child's newer event should bubble up to the relay"
    );
}

#[tokio::test]
async fn move_ticket_refuses_relay_with_compound_subticket() {
    // Non-epic relay with a compound sub-ticket — direct mutation of its
    // status would silently revert because list_relays now derives.
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "src/lib.rs",
        "//! @yah:relay(R905, \"parent\")\n\
         //! @yah:status(open)\n\n\
         /// @yah:ticket(R905-T1, \"child\")\n\
         /// @yah:status(open)\n\
         /// @yah:parent(R905)\n\
         pub fn child() {}\n",
    );

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let err = svc
        .move_ticket(MoveTicketParams {
            id: "R905".into(),
            to_bucket: "active".into(),
        })
        .await
        .unwrap_err();
    assert!(
        matches!(err, DaemonError::Conflict(ref m) if m.contains("relay with children")),
        "expected relay-with-children Conflict, got {err:?}"
    );
}

#[tokio::test]
async fn read_authored_file_rejects_paths_outside_the_sandbox() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        ".yah/arch/authored/ok.mmd",
        "flowchart LR\n  A --> B\n",
    );
    write(&dir, "src/secret.rs", "// not for the renderer\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // `..` traversal out of the sandbox.
    let escape = svc
        .read_authored_file(ReadAuthoredFileParams {
            rel_path: ".yah/arch/authored/../../../src/secret.rs".into(),
        })
        .await;
    assert!(matches!(escape, Err(DaemonError::Conflict(_))));

    // Inside the sandbox but neither a .mmd nor .md extension.
    write(&dir, ".yah/arch/authored/note.txt", "plain\n");
    let wrong_ext = svc
        .read_authored_file(ReadAuthoredFileParams {
            rel_path: ".yah/arch/authored/note.txt".into(),
        })
        .await;
    assert!(matches!(wrong_ext, Err(DaemonError::Conflict(_))));

    // .md is accepted alongside .mmd.
    write(&dir, ".yah/arch/authored/notes.md", "# ok\n");
    let md = svc
        .read_authored_file(ReadAuthoredFileParams {
            rel_path: ".yah/arch/authored/notes.md".into(),
        })
        .await
        .expect(".md must round-trip through the same sandboxed reader");
    assert_eq!(md.content, "# ok\n");
}

#[tokio::test]
async fn file_read_returns_utf8_content_for_text_file() {
    let dir = tempfile::tempdir().unwrap();
    let body = "fn main() {\n    println!(\"hi\");\n}\n";
    write(&dir, "src/lib.rs", body);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_read(FileReadParams {
            path: "src/lib.rs".into(),
            range: None,
        })
        .await
        .unwrap();
    assert_eq!(r.encoding, FileEncoding::Utf8);
    assert_eq!(r.content, body);
    assert_eq!(r.offset, 0);
    assert_eq!(r.bytes as usize, body.len());
    assert_eq!(r.total_bytes, body.len() as u64);
    assert!(r.eof);
    assert!(!r.truncated);
}

#[tokio::test]
async fn file_read_falls_back_to_base64_for_binary() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = [0xFFu8, 0xFE, 0x00, 0x01, 0x80, 0x90];
    let abs = dir.path().join("blob.bin");
    fs::write(&abs, bytes).unwrap();

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_read(FileReadParams {
            path: "blob.bin".into(),
            range: None,
        })
        .await
        .unwrap();
    assert_eq!(r.encoding, FileEncoding::Base64);
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&r.content)
        .unwrap();
    assert_eq!(decoded, bytes);
    assert_eq!(r.bytes, bytes.len() as u32);
}

#[tokio::test]
async fn file_read_paged_range_returns_slice_without_truncating() {
    let dir = tempfile::tempdir().unwrap();
    let body = "0123456789ABCDEF";
    write(&dir, "data.txt", body);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_read(FileReadParams {
            path: "data.txt".into(),
            range: Some(FileReadRange { offset: 4, len: 6 }),
        })
        .await
        .unwrap();
    assert_eq!(r.content, "456789");
    assert_eq!(r.offset, 4);
    assert_eq!(r.bytes, 6);
    assert_eq!(r.total_bytes, body.len() as u64);
    assert!(!r.eof);
    assert!(!r.truncated);

    let tail = svc
        .file_read(FileReadParams {
            path: "data.txt".into(),
            range: Some(FileReadRange { offset: 10, len: 100 }),
        })
        .await
        .unwrap();
    assert_eq!(tail.content, "ABCDEF");
    assert!(tail.eof);
}

#[tokio::test]
async fn file_read_clips_at_soft_cap_when_no_range_supplied() {
    let dir = tempfile::tempdir().unwrap();
    // 6MB of 'a' — over the 5MB soft cap.
    let body = "a".repeat(6 * 1024 * 1024);
    write(&dir, "big.txt", &body);

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_read(FileReadParams {
            path: "big.txt".into(),
            range: None,
        })
        .await
        .unwrap();
    assert_eq!(r.bytes, 5 * 1024 * 1024);
    assert_eq!(r.total_bytes, body.len() as u64);
    assert!(r.truncated, "soft cap should clip an unbounded read");
    assert!(!r.eof);

    // Paging past the cap with an explicit range should bypass it.
    let paged = svc
        .file_read(FileReadParams {
            path: "big.txt".into(),
            range: Some(FileReadRange {
                offset: 5 * 1024 * 1024,
                len: 1024 * 1024,
            }),
        })
        .await
        .unwrap();
    assert_eq!(paged.bytes, 1024 * 1024);
    assert!(paged.eof);
    assert!(!paged.truncated);
}

#[tokio::test]
async fn file_read_rejects_paths_outside_the_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "ok.txt", "inside\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let escape = svc
        .file_read(FileReadParams {
            path: "../escape.txt".into(),
            range: None,
        })
        .await;
    assert!(
        matches!(escape, Err(DaemonError::Conflict(_)) | Err(DaemonError::Io(_))),
        "expected Conflict or Io rejection for path escape, got {escape:?}"
    );

    let missing = svc
        .file_read(FileReadParams {
            path: "does-not-exist".into(),
            range: None,
        })
        .await;
    assert!(matches!(
        missing,
        Err(DaemonError::Conflict(_)) | Err(DaemonError::Io(_))
    ));
}

#[tokio::test]
async fn file_write_creates_new_file_when_expected_mtime_absent() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_write(FileWriteParams {
            path: "notes/hello.txt".into(),
            content: "hello\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: None,
        })
        .await;
    // Parent doesn't exist yet — daemon refuses to mkdir -p.
    assert!(matches!(r, Err(DaemonError::Conflict(_))), "got {r:?}");

    fs::create_dir_all(dir.path().join("notes")).unwrap();
    let r = svc
        .file_write(FileWriteParams {
            path: "notes/hello.txt".into(),
            content: "hello\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: None,
        })
        .await
        .unwrap();
    assert!(r.created);
    assert_eq!(r.bytes, 6);
    assert!(r.mtime_ms.is_some());
    let on_disk = fs::read_to_string(dir.path().join("notes/hello.txt")).unwrap();
    assert_eq!(on_disk, "hello\n");
}

#[tokio::test]
async fn file_write_rejects_create_when_target_exists() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "exists.txt", "old\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_write(FileWriteParams {
            path: "exists.txt".into(),
            content: "new\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: None,
        })
        .await;
    assert!(matches!(r, Err(DaemonError::Conflict(_))), "got {r:?}");
    // File should be untouched.
    assert_eq!(fs::read_to_string(dir.path().join("exists.txt")).unwrap(), "old\n");
}

#[tokio::test]
async fn file_write_overwrites_when_expected_mtime_matches() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "doc.txt", "v1\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let read = svc
        .file_read(FileReadParams { path: "doc.txt".into(), range: None })
        .await
        .unwrap();
    let abs = dir.path().join("doc.txt");
    let mtime = fs::metadata(&abs)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert_eq!(read.content, "v1\n");

    let r = svc
        .file_write(FileWriteParams {
            path: "doc.txt".into(),
            content: "v2\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: Some(mtime),
        })
        .await
        .unwrap();
    assert!(!r.created);
    assert_eq!(r.bytes, 3);
    assert_eq!(fs::read_to_string(&abs).unwrap(), "v2\n");
    assert!(r.mtime_ms.is_some());
}

#[tokio::test]
async fn file_write_rejects_when_mtime_drifted() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "doc.txt", "v1\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    // An mtime the file definitely doesn't have.
    let r = svc
        .file_write(FileWriteParams {
            path: "doc.txt".into(),
            content: "v2\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: Some(0),
        })
        .await;
    match r {
        Err(DaemonError::Conflict(msg)) => {
            assert!(
                msg.contains("mtime mismatch"),
                "expected mtime mismatch in conflict, got: {msg}"
            );
        }
        other => panic!("expected Conflict(mtime mismatch), got {other:?}"),
    }
    assert_eq!(fs::read_to_string(dir.path().join("doc.txt")).unwrap(), "v1\n");
}

#[tokio::test]
async fn file_write_rejects_update_when_target_missing() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_write(FileWriteParams {
            path: "ghost.txt".into(),
            content: "x\n".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: Some(123),
        })
        .await;
    assert!(matches!(r, Err(DaemonError::Conflict(_))), "got {r:?}");
}

#[tokio::test]
async fn file_write_round_trips_base64_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let raw = [0xFFu8, 0xFE, 0x00, 0x01, 0x80, 0x90];
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw);

    let r = svc
        .file_write(FileWriteParams {
            path: "blob.bin".into(),
            content: encoded,
            encoding: FileEncoding::Base64,
            expected_mtime_ms: None,
        })
        .await
        .unwrap();
    assert!(r.created);
    assert_eq!(r.bytes, raw.len() as u64);
    assert_eq!(fs::read(dir.path().join("blob.bin")).unwrap(), raw);
}

#[tokio::test]
async fn file_write_rejects_paths_outside_the_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .file_write(FileWriteParams {
            path: "../escape.txt".into(),
            content: "no".into(),
            encoding: FileEncoding::Utf8,
            expected_mtime_ms: None,
        })
        .await;
    assert!(
        matches!(r, Err(DaemonError::Conflict(_))),
        "expected Conflict for escape, got {r:?}"
    );
}

#[tokio::test]
async fn dir_list_returns_immediate_children_sorted_dirs_first() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/lib.rs", "pub fn x() {}\n");
    write(&dir, "src/sub/mod.rs", "// nested\n");
    write(&dir, "Cargo.toml", "[package]\n");
    write(&dir, "README.md", "# rig\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .dir_list(DirListParams { path: String::new() })
        .await
        .unwrap();
    assert_eq!(r.path, "");
    let names: Vec<&str> = r.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["src", "Cargo.toml", "README.md"]);
    assert_eq!(r.entries[0].kind, DirEntryKind::Dir);
    assert_eq!(r.entries[1].kind, DirEntryKind::File);
    assert!(r.entries[1].size > 0);
    assert!(r.entries[1].mtime_ms.is_some());
    assert!(!r.entries[1].is_symlink);

    // Listing a sub-directory by rel-path.
    let sub = svc
        .dir_list(DirListParams {
            path: "src".into(),
        })
        .await
        .unwrap();
    assert_eq!(sub.path, "src");
    let sub_names: Vec<&str> = sub.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(sub_names, vec!["sub", "lib.rs"]);
}

#[tokio::test]
async fn dir_list_treats_dot_and_empty_path_as_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "a.txt", "a");
    write(&dir, "b.txt", "b");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r1 = svc
        .dir_list(DirListParams { path: String::new() })
        .await
        .unwrap();
    let r2 = svc
        .dir_list(DirListParams { path: ".".into() })
        .await
        .unwrap();
    let r3 = svc
        .dir_list(DirListParams { path: "/".into() })
        .await
        .unwrap();
    let names = |r: &yah_kg::rpc::DirListResult| {
        r.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&r1), names(&r2));
    assert_eq!(names(&r1), names(&r3));
    assert_eq!(r1.path, "");
    assert_eq!(r2.path, "");
    assert_eq!(r3.path, "");
}

#[tokio::test]
async fn dir_list_rejects_paths_outside_the_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "ok.txt", "inside\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let escape = svc
        .dir_list(DirListParams {
            path: "..".into(),
        })
        .await;
    assert!(
        matches!(escape, Err(DaemonError::Conflict(_)) | Err(DaemonError::Io(_))),
        "expected escape rejection, got {escape:?}"
    );

    let missing = svc
        .dir_list(DirListParams {
            path: "no-such-dir".into(),
        })
        .await;
    assert!(matches!(
        missing,
        Err(DaemonError::Conflict(_)) | Err(DaemonError::Io(_))
    ));
}

#[tokio::test]
async fn dir_list_rejects_a_file_path() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "file.txt", "x");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();

    let r = svc
        .dir_list(DirListParams {
            path: "file.txt".into(),
        })
        .await;
    assert!(matches!(r, Err(DaemonError::Conflict(_))));
}

async fn wait_for_file_event<F: FnMut(&FileEvent) -> bool>(
    rx: &mut tokio::sync::broadcast::Receiver<FileEvent>,
    mut pred: F,
) -> FileEvent {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let now = tokio::time::Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            panic!("timed out waiting for file event");
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(ev)) if pred(&ev) => return ev,
            Ok(Ok(_)) => continue,
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(RecvError::Closed)) | Err(_) => {
                panic!("file_events channel closed before matching event");
            }
        }
    }
}

#[tokio::test]
async fn file_watch_emits_modified_events_for_the_target_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "src/lib.rs", "pub fn first() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    svc.start_watching().await.unwrap();

    let mut rx = svc.subscribe_file_events();
    let WatchResult { id } = svc
        .watch_file(FileWatchParams {
            path: "src/lib.rs".into(),
        })
        .await
        .unwrap();
    assert!(id > 0);

    tokio::time::sleep(Duration::from_millis(100)).await;
    fs::write(&path, "pub fn second() {}\n").unwrap();

    let ev = wait_for_file_event(&mut rx, |e| e.watch_id == id).await;
    assert_eq!(ev.kind, FileEventKind::Modified);
    assert_eq!(ev.path, "src/lib.rs");
    assert!(ev.mtime_ms.is_some());

    svc.stop_watching().await;
}

#[tokio::test]
async fn dir_watch_emits_recursive_events_under_the_dir() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "src/a.rs", "pub fn a() {}\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    svc.start_watching().await.unwrap();

    let mut rx = svc.subscribe_file_events();
    let WatchResult { id } = svc
        .watch_dir(DirWatchParams {
            path: "src".into(),
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    let nested = write(&dir, "src/sub/b.rs", "pub fn b() {}\n");

    let ev = wait_for_file_event(&mut rx, |e| {
        e.watch_id == id && e.path.starts_with("src/")
    })
    .await;
    assert!(matches!(ev.kind, FileEventKind::Modified));
    assert!(ev.path == "src/sub" || ev.path == "src/sub/b.rs");

    let _ = nested;
    svc.stop_watching().await;
}

#[tokio::test]
async fn unwatch_stops_further_events_for_the_handle() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, "watched.txt", "v1\n");

    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    svc.start_watching().await.unwrap();

    let mut rx = svc.subscribe_file_events();
    let WatchResult { id } = svc
        .watch_file(FileWatchParams {
            path: "watched.txt".into(),
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    fs::write(&path, "v2\n").unwrap();
    let _ = wait_for_file_event(&mut rx, |e| e.watch_id == id).await;

    svc.unwatch(UnwatchParams { id }).await.unwrap();

    // Drain anything queued before the unwatch took effect.
    while rx.try_recv().is_ok() {}

    fs::write(&path, "v3\n").unwrap();
    // No event should arrive within a generous window.
    let res = timeout(Duration::from_millis(400), rx.recv()).await;
    assert!(
        matches!(res, Err(_)),
        "expected timeout after unwatch, got {res:?}"
    );

    svc.stop_watching().await;
}

#[tokio::test]
async fn unwatch_unknown_id_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    let r = svc.unwatch(UnwatchParams { id: 99_999 }).await;
    assert!(r.is_ok());
}

#[tokio::test]
async fn watch_file_rejects_paths_outside_the_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "ok.txt", "x\n");
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    let r = svc
        .watch_file(FileWatchParams {
            path: "../escape.txt".into(),
        })
        .await;
    assert!(matches!(
        r,
        Err(DaemonError::Conflict(_)) | Err(DaemonError::Io(_))
    ));
}

#[tokio::test]
async fn watch_dir_with_empty_path_watches_the_rig_root() {
    let dir = tempfile::tempdir().unwrap();
    let svc = KgService::new(registry());
    svc.boot(dir.path().to_path_buf()).await.unwrap();
    let r = svc.watch_dir(DirWatchParams { path: String::new() }).await.unwrap();
    assert!(r.id > 0);
}
