//! `rs-hack comments` list / apply / verify, driven through the lib API.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rs_hack::commands::comments::{
    apply, compare_sources, list, parse_line_range, to_batch, verify, ApplyArgs, CommentKind,
    CommentOp, CommentSpan, ListArgs, OpKind, VerifyArgs,
};

const FIXTURE: &str = include_str!("fixtures/comments/all_kinds.rs");

struct Scratch {
    _dir: tempfile::TempDir,
    file: PathBuf,
    state: PathBuf,
}

fn scratch(src: &str) -> Scratch {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("all_kinds.rs");
    std::fs::write(&file, src).unwrap();
    let state = dir.path().join("state");
    Scratch {
        _dir: dir,
        file,
        state,
    }
}

fn spans(file: &Path) -> Vec<CommentSpan> {
    list(&ListArgs {
        paths: vec![file.to_path_buf()],
        exclude: Vec::new(),
        exclude_annotations: false,
        lines: None,
    })
    .unwrap()
    .spans
}

fn find<'a>(spans: &'a [CommentSpan], needle: &str) -> &'a CommentSpan {
    spans
        .iter()
        .find(|s| s.text.contains(needle))
        .unwrap_or_else(|| panic!("no span containing {needle:?}"))
}

fn op(s: &CommentSpan, kind: OpKind, text: Option<String>) -> CommentOp {
    CommentOp {
        file: s.file.clone(),
        hash: s.hash.clone(),
        op: kind,
        text,
        lines: None,
    }
}

fn run_apply(
    sc: &Scratch,
    ops: Vec<CommentOp>,
    write: bool,
    allow: bool,
) -> rs_hack::commands::comments::ApplyReport {
    apply(&ApplyArgs {
        ops,
        apply: write,
        allow_annotations: allow,
        state_dir: sc.state.clone(),
        command_line: "test".to_string(),
    })
    .unwrap()
}

#[test]
fn list_covers_every_kind() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);

    let inner = find(&sp, "Module docs");
    assert_eq!(inner.kind, CommentKind::InnerDoc);
    assert_eq!(
        inner.lines,
        [1, 2],
        "consecutive //! lines merge into one span"
    );
    assert_eq!(
        find(&sp, "Inner block doc").kind,
        CommentKind::InnerDoc
    );

    let doc = find(&sp, "Doc on a struct");
    assert_eq!(doc.kind, CommentKind::Doc);
    let item = doc.attached_item.as_ref().unwrap();
    assert_eq!(
        (item.node_type.as_str(), item.name.as_str()),
        ("struct", "Widget")
    );
    assert_eq!(item.visibility, "pub");

    let field = find(&sp, "Doc on a field").attached_item.clone().unwrap();
    assert_eq!(
        (field.node_type.as_str(), field.name.as_str()),
        ("field", "Widget::size")
    );

    let trailing = find(&sp, "trailing line comment");
    assert_eq!(trailing.kind, CommentKind::Line);
    assert!(trailing.attached_item.is_none());

    let block_doc = find(&sp, "Block doc on a fn");
    assert_eq!(block_doc.kind, CommentKind::Doc);
    assert_eq!(block_doc.attached_item.as_ref().unwrap().name, "run");

    let body = find(&sp, "body comment one");
    assert!(body.in_body);
    assert_eq!(body.kind, CommentKind::Line);
    assert!(
        body.text.contains("body comment two"),
        "adjacent // lines merge"
    );

    let nested = find(&sp, "nested");
    assert_eq!(nested.kind, CommentKind::Block);
    assert!(nested.in_body);
    assert!(
        nested.text.ends_with("still block */"),
        "nested block comment lexed whole"
    );

    assert_eq!(
        find(&sp, "four slashes").kind,
        CommentKind::Line
    );
    assert_eq!(
        find(&sp, "three stars").kind,
        CommentKind::Block
    );

    let method = find(&sp, "attached to a method")
        .attached_item
        .clone()
        .unwrap();
    assert_eq!(
        (method.node_type.as_str(), method.name.as_str()),
        ("impl-method", "Widget::helper")
    );

    // Strings, raw strings and char literals are not comments.
    assert!(sp.iter().all(|s| !s.text.contains("not // a comment")));
    assert!(sp.iter().all(|s| !s.text.contains("nor /* this")));
    assert_eq!(
        find(&sp, "after tricky quotes").kind,
        CommentKind::Line
    );

    // Spans are exact slices of the source, and hashes are stable.
    for s in &sp {
        assert_eq!(&FIXTURE[s.span[0]..s.span[1]], s.text);
    }

    // A hash identifies exactly one span in its file (AC1 of R917-F20).
    let hashes: HashSet<&str> = sp.iter().map(|s| s.hash.as_str()).collect();
    assert_eq!(hashes.len(), sp.len(), "every span's hash must be unique");
}

#[test]
fn identical_comment_text_gets_distinct_hashes_in_one_file() {
    // Two non-adjacent spans with byte-identical text: adjacent same-kind own-line comments
    // merge into one span, so these are kept apart by real code in between.
    let dup = "fn a() {}\n// dup\nfn b() {}\n// dup\nfn c() {}\n";
    let sc = scratch(dup);
    let sp = spans(&sc.file);

    let dups: Vec<&CommentSpan> = sp.iter().filter(|s| s.text == "// dup").collect();
    assert_eq!(dups.len(), 2, "{sp:?}");
    assert_ne!(
        dups[0].hash, dups[1].hash,
        "two spans with identical text must still hash apart"
    );

    let hashes: HashSet<&str> = sp.iter().map(|s| s.hash.as_str()).collect();
    assert_eq!(hashes.len(), sp.len(), "list must never emit a duplicate hash");
}

#[test]
fn annotation_flag_covers_wrapped_continuation_and_splits_prose() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let anno = find(&sp, "@yah:ticket");
    assert!(anno.annotation);
    assert_eq!(
        anno.lines,
        [6, 8],
        "wrapped @yah:next continuation stays in the annotation span"
    );
    let prose = find(&sp, "plain prose after");
    assert!(
        !prose.annotation,
        "prose after an annotation is its own editable span"
    );
    assert_eq!(prose.lines, [9, 9]);
    assert_eq!(sp.iter().filter(|s| s.annotation).count(), 1);
}

#[test]
fn lines_filter_keeps_only_spans_inside_the_range() {
    let sc = scratch(FIXTURE);
    let report = list(&ListArgs {
        paths: vec![sc.file],
        exclude: Vec::new(),
        exclude_annotations: false,
        lines: Some([11, 17]),
    })
    .unwrap();

    assert!(report.spans.iter().any(|s| s.text.contains("Doc on a struct")));
    assert!(report.spans.iter().any(|s| s.text.contains("Doc on a field")));
    assert!(report
        .spans
        .iter()
        .any(|s| s.text.contains("trailing line comment")));
    assert!(!report.spans.iter().any(|s| s.text.contains("Module docs")));
    assert!(!report.spans.iter().any(|s| s.text.contains("body comment one")));
    for s in &report.spans {
        assert!(
            s.lines[0] >= 11 && s.lines[1] <= 17,
            "span {s:?} falls outside the requested 11-17 range"
        );
    }
}

#[test]
fn lines_filter_applies_to_the_batch_format_too() {
    let sc = scratch(FIXTURE);
    let all = list(&ListArgs {
        paths: vec![sc.file.clone()],
        exclude: Vec::new(),
        exclude_annotations: false,
        lines: None,
    })
    .unwrap();
    let sliced = list(&ListArgs {
        paths: vec![sc.file],
        exclude: Vec::new(),
        exclude_annotations: false,
        lines: Some([11, 17]),
    })
    .unwrap();

    assert!(to_batch(&sliced).len() < to_batch(&all).len());
    assert!(
        to_batch(&sliced)
            .iter()
            .all(|o| o.lines.is_some_and(|[a, b]| a >= 11 && b <= 17))
    );
}

#[test]
fn parse_line_range_rejects_bad_input() {
    assert!(parse_line_range("10-20").is_ok());
    assert!(parse_line_range("0-20").is_err(), "not 1-indexed");
    assert!(parse_line_range("20-10").is_err(), "inverted");
    assert!(parse_line_range("nope").is_err());
}

#[test]
fn apply_dry_run_then_write_then_revert() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let ops = vec![
        op(find(&sp, "body comment one"), OpKind::Delete, None),
        op(find(&sp, "trailing line comment"), OpKind::Delete, None),
        op(
            find(&sp, "Doc on a struct"),
            OpKind::Replace,
            Some("/// Widget.".to_string()),
        ),
    ];

    let dry = run_apply(&sc, ops.clone(), false, false);
    assert!(dry.dry_run && dry.run_id.is_none());
    assert_eq!(dry.accepted.len(), 3, "refused: {:?}", dry.refused);
    assert_eq!(
        std::fs::read_to_string(&sc.file).unwrap(),
        FIXTURE,
        "dry run writes nothing"
    );

    let wet = run_apply(&sc, ops, true, false);
    let run_id = wet.run_id.expect("run id");
    let after = std::fs::read_to_string(&sc.file).unwrap();
    assert!(!after.contains("body comment"));
    assert!(after.contains("    let s = \"not // a comment\";"));
    assert!(
        after.contains("pub size: u32,\n"),
        "trailing comment and its padding removed"
    );
    assert!(after.contains("/// Widget.\n#[derive(Debug)]"));
    assert!(
        !after.contains("\n\n    let s"),
        "own-line delete takes the whole lines"
    );

    let v = verify(&VerifyArgs::RunId {
        run_id: run_id.clone(),
        state_dir: sc.state.clone(),
    })
    .unwrap();
    assert!(v.equal, "{v:?}");

    rs_hack::state::revert_run(&run_id, false, &sc.state).unwrap();
    assert_eq!(std::fs::read_to_string(&sc.file).unwrap(), FIXTURE);
}

#[test]
fn apply_refuses_a_hand_typed_hash_that_never_existed() {
    let sc = scratch(FIXTURE);
    let bogus = CommentOp {
        file: sc.file.clone(),
        hash: "deadbeefdeadbeef".to_string(),
        op: OpKind::Delete,
        text: None,
        lines: None,
    };
    let report = run_apply(&sc, vec![bogus], false, false);
    assert!(report.accepted.is_empty());
    assert_eq!(report.refused.len(), 1);
    let reason = report.refused[0].reason.as_deref().unwrap();
    assert!(reason.contains("unknown hash"), "{reason}");
    assert!(reason.contains("deadbeefdeadbeef"), "{reason}");
    assert_eq!(
        std::fs::read_to_string(&sc.file).unwrap(),
        FIXTURE,
        "a refused op must never write"
    );
}

#[test]
fn a_hash_computed_before_the_text_changed_is_unknown_after() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let target = find(&sp, "body comment one").clone();
    // The file moves under the batch: same bytes it once occupied now hold different text, so
    // the old hash (computed from the old text) can no longer resolve to anything.
    std::fs::write(
        &sc.file,
        FIXTURE.replace("body comment one", "body comment ONE"),
    )
    .unwrap();

    let report = run_apply(&sc, vec![op(&target, OpKind::Delete, None)], true, false);
    assert!(report.accepted.is_empty());
    assert_eq!(report.refused.len(), 1);
    assert!(
        report.refused[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("unknown hash")
    );
    assert!(report.run_id.is_none());
    assert!(
        std::fs::read_to_string(&sc.file)
            .unwrap()
            .contains("body comment ONE")
    );
}

#[test]
fn a_hash_still_resolves_after_the_file_shifts_under_it() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let target = find(&sp, "body comment one").clone();
    // Insert a line above everything: every byte offset below it moves, but the comment's
    // TEXT (and therefore its hash) does not, so resolution by hash alone still finds it.
    std::fs::write(&sc.file, format!("// new header\n{FIXTURE}")).unwrap();
    let report = run_apply(&sc, vec![op(&target, OpKind::Delete, None)], true, false);
    assert_eq!(report.accepted.len(), 1, "{report:?}");
    assert!(
        !std::fs::read_to_string(&sc.file)
            .unwrap()
            .contains("body comment one")
    );
}

#[test]
fn two_batches_from_different_ranges_of_one_file_apply_in_either_order() {
    for reversed in [false, true] {
        let sc = scratch(FIXTURE);
        let sp = spans(&sc.file);
        // Both batches are built against the SAME original listing, so batch B's hash is
        // "stale" relative to whichever batch actually runs second -- and must still resolve.
        let batch_a = vec![op(find(&sp, "body comment one"), OpKind::Delete, None)];
        let batch_b = vec![op(find(&sp, "trailing line comment"), OpKind::Delete, None)];
        let (first, second) = if reversed {
            (batch_b, batch_a)
        } else {
            (batch_a, batch_b)
        };

        let r1 = run_apply(&sc, first, true, false);
        assert_eq!(r1.accepted.len(), 1, "reversed={reversed} r1={r1:?}");
        let r2 = run_apply(&sc, second, true, false);
        assert_eq!(r2.accepted.len(), 1, "reversed={reversed} r2={r2:?}");

        let after = std::fs::read_to_string(&sc.file).unwrap();
        assert!(!after.contains("body comment one"), "reversed={reversed}");
        assert!(!after.contains("trailing line comment"), "reversed={reversed}");
    }
}

#[test]
fn format_batch_round_trip_list_flip_apply_verify() {
    let sc = scratch(FIXTURE);
    let report = list(&ListArgs {
        paths: vec![sc.file.clone()],
        exclude: Vec::new(),
        exclude_annotations: false,
        lines: None,
    })
    .unwrap();
    let mut batch = to_batch(&report);

    // Every stub starts as "keep", and no annotation span is present at all.
    assert!(batch.iter().all(|o| o.op == OpKind::Keep));
    assert!(
        batch
            .iter()
            .all(|o| !o.text.as_deref().unwrap_or_default().contains("@yah:"))
    );

    let target = batch
        .iter_mut()
        .find(|o| {
            o.text
                .as_deref()
                .is_some_and(|t| t.contains("trailing line comment"))
        })
        .expect("a stub for the trailing line comment");
    target.op = OpKind::Delete;

    let dry = run_apply(&sc, batch.clone(), false, false);
    assert_eq!(dry.accepted.len(), 1, "{dry:?}");
    assert!(dry.refused.is_empty(), "{dry:?}");
    assert_eq!(
        std::fs::read_to_string(&sc.file).unwrap(),
        FIXTURE,
        "dry run writes nothing, and every untouched \"keep\" stub is ignored"
    );

    let wet = run_apply(&sc, batch, true, false);
    let run_id = wet.run_id.expect("run id");
    assert!(
        !std::fs::read_to_string(&sc.file)
            .unwrap()
            .contains("trailing line comment")
    );

    let v = verify(&VerifyArgs::RunId {
        run_id,
        state_dir: sc.state,
    })
    .unwrap();
    assert!(v.equal, "{v:?}");
}

#[test]
fn annotation_span_is_refused_without_opt_in() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let anno = find(&sp, "@yah:ticket");

    let refused = run_apply(&sc, vec![op(anno, OpKind::Delete, None)], true, false);
    assert_eq!(refused.refused.len(), 1);
    assert!(
        refused.refused[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("--allow-annotations")
    );
    assert_eq!(std::fs::read_to_string(&sc.file).unwrap(), FIXTURE);

    // Replacing prose with an annotation is refused too.
    let prose = find(&sp, "plain prose after");
    let sneaky = run_apply(
        &sc,
        vec![op(
            prose,
            OpKind::Replace,
            Some("// @arch:layer(x)".to_string()),
        )],
        false,
        false,
    );
    assert_eq!(sneaky.refused.len(), 1);

    let allowed = run_apply(&sc, vec![op(anno, OpKind::Delete, None)], true, true);
    assert_eq!(allowed.accepted.len(), 1);
    assert!(!std::fs::read_to_string(&sc.file).unwrap().contains("@yah:"));
}

#[test]
fn replacement_that_is_code_is_refused() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let report = run_apply(
        &sc,
        vec![op(
            find(&sp, "body comment one"),
            OpKind::Replace,
            Some("let sneaky = 1;".to_string()),
        )],
        true,
        false,
    );
    assert_eq!(report.refused.len(), 1);
    assert_eq!(std::fs::read_to_string(&sc.file).unwrap(), FIXTURE);
}

#[test]
fn replace_op_with_no_text_is_refused() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let report = run_apply(
        &sc,
        vec![op(find(&sp, "body comment one"), OpKind::Replace, None)],
        true,
        false,
    );
    assert_eq!(report.refused.len(), 1);
    assert!(
        report.refused[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("no `text`")
    );
}

#[test]
fn verify_passes_on_pure_comment_delete() {
    let after = FIXTURE
        .replace("    // body comment one\n    // body comment two\n", "")
        .replace("/// Doc on a struct.\n/// Two lines.\n", "")
        .replace("//! Second inner-doc line.\n", "")
        .replace(" // trailing line comment", "")
        .replace("fn run(w: &Widget)", "fn run(\n    w  :  &Widget\n)");
    assert_eq!(compare_sources(FIXTURE, &after).unwrap(), None);

    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.rs");
    let after_path = dir.path().join("after.rs");
    std::fs::write(&before, FIXTURE).unwrap();
    std::fs::write(&after_path, &after).unwrap();
    let report = verify(&VerifyArgs::Files {
        before: before.display().to_string(),
        after: after_path,
    })
    .unwrap();
    assert!(report.equal);
}

#[test]
fn verify_fails_on_renamed_fn_and_names_it() {
    let after = FIXTURE.replace("fn helper(&self)", "fn helper_renamed(&self)");
    let d = compare_sources(FIXTURE, &after)
        .unwrap()
        .expect("difference");
    assert_eq!(d.before_item.as_deref(), Some("impl Widget::fn helper"));
    assert_eq!(
        d.after_item.as_deref(),
        Some("impl Widget::fn helper_renamed")
    );

    let top = FIXTURE.replace("pub fn run(", "pub fn run_renamed(");
    let d = compare_sources(FIXTURE, &top).unwrap().expect("difference");
    assert_eq!(d.before_item.as_deref(), Some("fn run"));
    assert_eq!(d.after_item.as_deref(), Some("fn run_renamed"));
}

#[test]
fn verify_fails_on_changed_string_literal_and_non_doc_attr() {
    let lit = FIXTURE.replace("\"not // a comment\"", "\"not a comment\"");
    assert!(compare_sources(FIXTURE, &lit).unwrap().is_some());
    let attr = FIXTURE.replace("#[derive(Debug)]", "#[derive(Debug, Clone)]");
    let d = compare_sources(FIXTURE, &attr)
        .unwrap()
        .expect("difference");
    assert_eq!(d.before_item.as_deref(), Some("struct Widget"));
}
