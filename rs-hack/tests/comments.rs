//! `rs-hack comments` list / apply / verify, driven through the lib API.

use std::path::{Path, PathBuf};

use rs_hack::commands::comments::{
    ApplyArgs, CommentAction, CommentKind, CommentOp, CommentSpan, ListArgs, VerifyArgs, apply,
    compare_sources, list, verify,
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

fn op(s: &CommentSpan, action: CommentAction) -> CommentOp {
    CommentOp {
        file: s.file.clone(),
        span: s.span,
        hash: s.hash.clone(),
        action,
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
    assert_eq!(find(&sp, "Inner block doc").kind, CommentKind::InnerDoc);

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

    assert_eq!(find(&sp, "four slashes").kind, CommentKind::Line);
    assert_eq!(find(&sp, "three stars").kind, CommentKind::Block);

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
    assert_eq!(find(&sp, "after tricky quotes").kind, CommentKind::Line);

    // Spans are exact slices of the source, and hashes are stable.
    for s in &sp {
        assert_eq!(&FIXTURE[s.span[0]..s.span[1]], s.text);
    }
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
fn apply_dry_run_then_write_then_revert() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let ops = vec![
        op(find(&sp, "body comment one"), CommentAction::Delete),
        op(find(&sp, "trailing line comment"), CommentAction::Delete),
        op(
            find(&sp, "Doc on a struct"),
            CommentAction::Replace {
                text: "/// Widget.".to_string(),
            },
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
fn stale_hash_is_refused() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let target = find(&sp, "body comment one").clone();
    // The file moves under the batch: same span bytes, different text.
    std::fs::write(
        &sc.file,
        FIXTURE.replace("body comment one", "body comment ONE"),
    )
    .unwrap();

    let report = run_apply(&sc, vec![op(&target, CommentAction::Delete)], true, false);
    assert!(report.accepted.is_empty());
    assert_eq!(report.refused.len(), 1);
    assert!(
        report.refused[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("stale span")
    );
    assert!(report.run_id.is_none());
    assert!(
        std::fs::read_to_string(&sc.file)
            .unwrap()
            .contains("body comment ONE")
    );
}

#[test]
fn span_shifted_by_an_insert_is_refused() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let target = find(&sp, "body comment one").clone();
    std::fs::write(&sc.file, format!("// new header\n{FIXTURE}")).unwrap();
    let report = run_apply(&sc, vec![op(&target, CommentAction::Delete)], true, false);
    assert!(report.accepted.is_empty(), "{report:?}");
}

#[test]
fn annotation_span_is_refused_without_opt_in() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let anno = find(&sp, "@yah:ticket");

    let refused = run_apply(&sc, vec![op(anno, CommentAction::Delete)], true, false);
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
            CommentAction::Replace {
                text: "// @arch:layer(x)".to_string(),
            },
        )],
        false,
        false,
    );
    assert_eq!(sneaky.refused.len(), 1);

    let allowed = run_apply(&sc, vec![op(anno, CommentAction::Delete)], true, true);
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
            CommentAction::Replace {
                text: "let sneaky = 1;".to_string(),
            },
        )],
        true,
        false,
    );
    assert_eq!(report.refused.len(), 1);
    assert_eq!(std::fs::read_to_string(&sc.file).unwrap(), FIXTURE);
}

#[test]
fn span_that_covers_code_is_refused() {
    let sc = scratch(FIXTURE);
    let sp = spans(&sc.file);
    let a = find(&sp, "Doc on a struct");
    let b = find(&sp, "Doc on a field");
    let span = [a.span[0], b.span[1]];
    let text = &FIXTURE[span[0]..span[1]];
    let report = run_apply(
        &sc,
        vec![CommentOp {
            file: sc.file.clone(),
            span,
            hash: blake3::hash(text.as_bytes()).to_hex().as_str()[..16].to_string(),
            action: CommentAction::Delete,
        }],
        true,
        false,
    );
    assert!(
        report.refused[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("covers code")
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
