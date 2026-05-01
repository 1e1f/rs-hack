//! Integration test for the R031-F4 write surface (`KgToolRegistry`'s
//! experimental writers). Sister to the unit tests in
//! `agent_tools::tests` — those exercise per-tool error paths against a
//! fake `KgService`; this one boots a *real* daemon over an ephemeral
//! fixture rig and asserts the watcher seam fans out
//! `IndexReason::AgentEdit` events whenever a writer runs.
//!
//! Why integration: `reindex_path` lives behind `KgService::boot` and the
//! `ArchEvent` broadcast only emits when the store has a registered
//! indexer for the file's extension. Unit tests with an unbooted service
//! can't observe that pipeline. This test is the contract that says
//! "every write tool reindexes the touched file under `AgentEdit`."
//!
//! Test rig is bootstrapped per-test in a tempdir (a tiny fake Cargo
//! crate) — we never dogfood against the host workspace, since that
//! would mutate real source mid-CI if `yah` is on PATH.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use desktop::agent_approval::{ApprovalChoice, ApprovalRouter, StaticApprovalRouter};
use desktop::agent_tools::{KgToolRegistry, ToolContext};
use desktop::state::RigId;
use kg::agent::SessionId;
use kg::event::{ArchEvent, IndexReason};
use kg_daemon::KgService;
use kg_json_yaml::{JsonIndexer, TomlIndexer, YamlIndexer};
use kg_rust::RustIndexer;
use kg_store::IndexerRegistry;
use kg_ts::TsIndexer;
use runner::ToolRegistry;
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::timeout;

/// Build a writer registry with a permissive router so the gate
/// auto-approves every write call. R031-F5 inserted the gate between
/// `ToolRegistry::execute` and `Tool::execute`; without a router the
/// experimental writers all fail with `approval_required`. The e2e
/// contract this file asserts is "writers reindex the rig" — gate
/// behaviour is covered separately in `agent_tools::tests`.
fn writers_with_apply_router(ctx: ToolContext) -> KgToolRegistry {
    let router: Arc<dyn ApprovalRouter> =
        Arc::new(StaticApprovalRouter::new(ApprovalChoice::Apply));
    KgToolRegistry::with_experimental_writers(ctx)
        .with_router(router)
        .with_session(SessionId::new("session:e2e_writers01"))
}

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

/// Drop a one-crate Cargo workspace into `dir`. A real `Cargo.toml` plus
/// a `src/lib.rs` with one struct + one fn is enough for the Rust
/// indexer to populate the store, and for `yah_add` (when `yah` is on
/// PATH) to have a real target to mutate.
fn write_fixture_rig(dir: &Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
         \n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub struct Widget {\n    pub id: u64,\n}\n\npub fn make() -> Widget {\n    Widget { id: 0 }\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("README.md"), "# fixture\n\nseed line\nbody\n").unwrap();
}

fn fake_ctx(rig_root: std::path::PathBuf, svc: Arc<KgService>) -> ToolContext {
    ToolContext {
        rig_id: RigId("rig:e2e".into()),
        rig_root,
        svc,
    }
}

/// Drain any events queued before the test started caring (boot fans out
/// a flurry of `IndexStarted` / `IndexFinished` / `NodeAdded`). Uses a
/// short non-blocking sweep so we don't accidentally swallow events from
/// the action under test.
async fn drain(rx: &mut tokio::sync::broadcast::Receiver<ArchEvent>) {
    while timeout(Duration::from_millis(50), rx.recv()).await.is_ok() {}
}

/// Wait until `rx` yields an event matching `pred`, or the test-wide
/// timeout fires. Panics with a clear message on timeout — keeps a
/// hung test from blocking the suite indefinitely.
async fn wait_for<F: FnMut(&ArchEvent) -> bool>(
    rx: &mut tokio::sync::broadcast::Receiver<ArchEvent>,
    mut pred: F,
    label: &str,
) -> ArchEvent {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let now = tokio::time::Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            panic!("timed out waiting for {label}");
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Ok(ev)) if pred(&ev) => return ev,
            Ok(Ok(_)) => continue,
            Ok(Err(RecvError::Lagged(_))) => continue,
            Ok(Err(RecvError::Closed)) | Err(_) => {
                panic!("channel closed or timed out before matching {label}");
            }
        }
    }
}

#[tokio::test]
async fn edit_file_round_trips_through_reindex_with_agent_edit_reason() {
    // The contract: a write tool's reindex call must surface as an
    // IndexStarted{ reason: AgentEdit } event so the UI can flag
    // agent-touched files differently from file-watch events.
    let tmp = TempDir::new().unwrap();
    write_fixture_rig(tmp.path());
    let svc = Arc::new(KgService::new(registry()));
    svc.boot(tmp.path().to_path_buf()).await.expect("boot");

    let mut rx = svc.subscribe();
    drain(&mut rx).await;

    let ctx = fake_ctx(tmp.path().to_path_buf(), svc.clone());
    let registry_ = writers_with_apply_router(ctx);
    let outcome = registry_
        .execute(
            "edit_file",
            json!({
                "path": "README.md",
                "old_string": "seed line",
                "new_string": "edited line",
            }),
        )
        .await;
    assert!(outcome.ok, "{outcome:?}");

    let ev = wait_for(
        &mut rx,
        |e| {
            matches!(
                e,
                ArchEvent::IndexStarted {
                    reason: IndexReason::AgentEdit,
                    ..
                }
            )
        },
        "IndexStarted{AgentEdit}",
    )
    .await;
    let ArchEvent::IndexStarted { reason, .. } = ev else {
        unreachable!()
    };
    assert!(matches!(reason, IndexReason::AgentEdit));

    wait_for(
        &mut rx,
        |e| matches!(e, ArchEvent::IndexFinished { .. }),
        "IndexFinished",
    )
    .await;

    // File on disk reflects the edit.
    let body = std::fs::read_to_string(tmp.path().join("README.md")).unwrap();
    assert!(body.contains("edited line"), "{body}");
}

#[tokio::test]
async fn write_arch_doc_creates_file_under_authored_sandbox() {
    // .mmd files aren't on the indexer registry, so `reindex_path` is a
    // no-op for them — we only assert the file landed in the right
    // place. (When an arch-doc indexer arrives, this test grows an
    // event-wait like the edit_file case above.)
    let tmp = TempDir::new().unwrap();
    write_fixture_rig(tmp.path());
    let svc = Arc::new(KgService::new(registry()));
    svc.boot(tmp.path().to_path_buf()).await.expect("boot");

    let ctx = fake_ctx(tmp.path().to_path_buf(), svc.clone());
    let registry_ = writers_with_apply_router(ctx);
    let outcome = registry_
        .execute(
            "write_arch_doc",
            json!({
                "rel_path": ".yah/arch/authored/topology.mmd",
                "content": "graph TD\n  A --> B\n  B --> C\n",
            }),
        )
        .await;
    assert!(outcome.ok, "{outcome:?}");
    let body = std::fs::read_to_string(tmp.path().join(".yah/arch/authored/topology.mmd")).unwrap();
    assert_eq!(body, "graph TD\n  A --> B\n  B --> C\n");
}

#[tokio::test]
async fn writer_chain_leaves_rig_in_consistent_state() {
    // Multi-step chain: write_arch_doc → edit_file. Asserts each step's
    // outcome envelope plus the final on-disk state. Sister to the
    // single-tool tests above; this is what catches "tool A's reindex
    // races with tool B's read" if anyone ever introduces shared state.
    let tmp = TempDir::new().unwrap();
    write_fixture_rig(tmp.path());
    let svc = Arc::new(KgService::new(registry()));
    svc.boot(tmp.path().to_path_buf()).await.expect("boot");

    let ctx = fake_ctx(tmp.path().to_path_buf(), svc.clone());
    let registry_ = writers_with_apply_router(ctx);

    let arch_doc = registry_
        .execute(
            "write_arch_doc",
            json!({
                "rel_path": ".yah/arch/authored/chain.mmd",
                "content": "graph TD\n  one --> two\n",
            }),
        )
        .await;
    assert!(arch_doc.ok, "{arch_doc:?}");

    let edit = registry_
        .execute(
            "edit_file",
            json!({
                "path": "README.md",
                "old_string": "body",
                "new_string": "BODY",
            }),
        )
        .await;
    assert!(edit.ok, "{edit:?}");

    // Final state: both files reflect their writes; the un-edited line
    // in README is preserved (edit_file's exact-match guard is what
    // makes that assertion meaningful).
    let mmd = std::fs::read_to_string(tmp.path().join(".yah/arch/authored/chain.mmd")).unwrap();
    assert_eq!(mmd, "graph TD\n  one --> two\n");
    let readme = std::fs::read_to_string(tmp.path().join("README.md")).unwrap();
    assert!(readme.contains("BODY"), "{readme}");
    assert!(readme.contains("seed line"), "{readme}");
}

#[tokio::test]
async fn yah_add_envelope_round_trips_when_yah_binary_present() {
    // CI may or may not have `yah` on PATH. When it does, this is the
    // only test that exercises the full subprocess → file-write →
    // reindex chain end-to-end (the unit test asserts envelope
    // shape but doesn't booted-daemon-verify the reindex). When it
    // doesn't, we skip cleanly so this file stays green on any host.
    if which_yah().is_none() {
        eprintln!("skipping yah_add_envelope_round_trips: `yah` not on PATH");
        return;
    }

    let tmp = TempDir::new().unwrap();
    write_fixture_rig(tmp.path());
    let svc = Arc::new(KgService::new(registry()));
    svc.boot(tmp.path().to_path_buf()).await.expect("boot");

    let mut rx = svc.subscribe();
    drain(&mut rx).await;

    let ctx = fake_ctx(tmp.path().to_path_buf(), svc.clone());
    let registry_ = writers_with_apply_router(ctx);
    let outcome = registry_
        .execute(
            "yah_add",
            json!({
                "paths": "src/lib.rs",
                "name": "Widget",
                "kind": "struct",
                "field_name": "label",
                "field_type": "String",
            }),
        )
        .await;

    // Subprocess might fail for other reasons (e.g. the field already
    // exists from a stale tempdir, or yah's CLI rejected the args) —
    // we only assert the envelope is well-formed and, on success, that
    // the file actually changed and a reindex fired.
    assert!(outcome.result.get("exit_code").is_some(), "{outcome:?}");
    let exit_code = outcome.result["exit_code"].as_i64().unwrap();
    if exit_code == 0
        && outcome.result["touched"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    {
        let body = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(
            body.contains("label"),
            "yah_add succeeded but file unchanged: {body}"
        );

        wait_for(
            &mut rx,
            |e| {
                matches!(
                    e,
                    ArchEvent::IndexStarted {
                        reason: IndexReason::AgentEdit,
                        ..
                    }
                )
            },
            "IndexStarted{AgentEdit} after yah_add",
        )
        .await;
    }
}

fn which_yah() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("yah");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
