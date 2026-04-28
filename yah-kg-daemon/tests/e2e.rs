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
use yah_kg::prompt::PromptMode;
use yah_kg::rpc::{
    Direction, GetTicketParams, ListRelaysParams, ListTicketsParams, LookupParams, NeighborsParams,
    RootsParams, SubgraphParams, TicketPromptParams,
};
use yah_kg_daemon::{DaemonError, KgService};
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
    let snap_path = dir.path().join(".yah/cache/snapshot.json");

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
    let snap_path = dir.path().join(".yah/cache/snapshot.json");
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
    let snap_path = dir.path().join(".yah/cache/snapshot.json");

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
async fn boot_with_snapshot_drops_files_deleted_offline() {
    let dir = tempfile::tempdir().unwrap();
    let _kept = write(&dir, "src/kept.rs", "pub fn kept() {}\n");
    let removed = write(&dir, "src/removed.rs", "pub fn going() {}\n");
    let snap_path = dir.path().join(".yah/cache/snapshot.json");

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
    let snap_path = dir_a.path().join(".yah/cache/snapshot.json");
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
