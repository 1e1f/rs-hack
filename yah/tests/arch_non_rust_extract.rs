//! Coverage for `line_extract` — the per-language scanner that picks up
//! `@yah:` / `@arch:` annotations from non-Rust files (TS, MD, TOML, YAML).
//! Each test writes a fixture into a temp dir, runs the workspace
//! extractor, and asserts the resulting tickets / annotation targets.

use yah::arch::annotation::{AnnotationTarget, ArchKind};
use yah::arch::extract::extract_from_workspace;
use yah::arch::ticket::TicketBoard;
use std::fs;
use std::path::PathBuf;

/// Create a fresh temp dir under `target/test-fixtures/` so the workspace
/// walker has a clean root to scan. Returns its path.
fn temp_workspace(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn ts_file_module_block_extracts_ticket() {
    let dir = temp_workspace("ts_module_block");
    fs::write(
        dir.join("server.ts"),
        r#"//! @yah:ticket(T01, "ts ticket")
//! @yah:status(open)
//! @yah:assignee(agent:claude)

export function main() {}
"#,
    )
    .unwrap();

    let anns = extract_from_workspace(&dir).unwrap();
    assert_eq!(anns.len(), 3, "should extract 3 annotations from TS module block");

    // All three share the same anchor (line 1) → one ticket on the board.
    let board = TicketBoard::from_annotations(&anns);
    let t = board.get("T01").expect("T01 should appear on the board");
    assert_eq!(t.title, "ts ticket");
    assert_eq!(t.assignee.as_deref(), Some("agent:claude"));
}

#[test]
fn ts_double_slash_comment_extracts() {
    let dir = temp_workspace("ts_double_slash");
    fs::write(
        dir.join("api.ts"),
        r#"// @yah:ticket(T02, "double slash works")
// @yah:status(open)

const x = 1;
"#,
    )
    .unwrap();

    let board = TicketBoard::from_annotations(&extract_from_workspace(&dir).unwrap());
    assert!(board.get("T02").is_some(), "// comments should also be scanned");
}

#[test]
fn ts_jsdoc_star_continuation_extracts() {
    let dir = temp_workspace("ts_jsdoc");
    fs::write(
        dir.join("util.ts"),
        r#"/**
 * @yah:ticket(T03, "jsdoc body")
 * @yah:status(open)
 */
export const x = 1;
"#,
    )
    .unwrap();

    let board = TicketBoard::from_annotations(&extract_from_workspace(&dir).unwrap());
    assert!(board.get("T03").is_some(), "* prefix inside /** */ should be scanned");
}

#[test]
fn md_plain_lines_extract_outside_fence() {
    let dir = temp_workspace("md_basic");
    fs::write(
        dir.join("notes.md"),
        r#"# Plan

@yah:ticket(T04, "md ticket")
@yah:status(open)

Some prose follows.
"#,
    )
    .unwrap();

    let board = TicketBoard::from_annotations(&extract_from_workspace(&dir).unwrap());
    let t = board.get("T04").expect("plain-line MD annotation should extract");
    assert_eq!(t.title, "md ticket");
}

#[test]
fn md_fenced_code_block_is_ignored() {
    let dir = temp_workspace("md_fenced");
    fs::write(
        dir.join("doc.md"),
        r#"# Doc

Outside fence:
@yah:ticket(T05, "real ticket")

```
@yah:ticket(T99, "should NOT extract — inside fence")
```

After fence.
"#,
    )
    .unwrap();

    let anns = extract_from_workspace(&dir).unwrap();
    let board = TicketBoard::from_annotations(&anns);
    assert!(board.get("T05").is_some(), "outside-fence ticket should extract");
    assert!(
        board.get("T99").is_none(),
        "inside-fence annotation should be skipped — got {:?}",
        anns.iter().map(|a| (&a.line, &a.kind)).collect::<Vec<_>>()
    );
}

#[test]
fn toml_hash_prefix_extracts() {
    let dir = temp_workspace("toml_basic");
    fs::write(
        dir.join("Config.toml"),
        r#"# @yah:ticket(T06, "toml ticket")
# @yah:status(open)

[package]
name = "example"
"#,
    )
    .unwrap();

    let board = TicketBoard::from_annotations(&extract_from_workspace(&dir).unwrap());
    assert!(board.get("T06").is_some(), "TOML #-prefix annotation should extract");
}

#[test]
fn yaml_hash_prefix_extracts() {
    let dir = temp_workspace("yaml_basic");
    fs::write(
        dir.join("ci.yml"),
        r#"# @yah:ticket(T07, "yaml ticket")
# @yah:status(open)
name: ci
"#,
    )
    .unwrap();

    let board = TicketBoard::from_annotations(&extract_from_workspace(&dir).unwrap());
    assert!(board.get("T07").is_some(), ".yml hash-prefix annotation should extract");
}

#[test]
fn md_blank_line_breaks_block_so_anchors_differ() {
    // Two separate annotation blocks separated by a blank line should
    // become two separate ticket buckets via distinct anchors.
    let dir = temp_workspace("md_two_blocks");
    fs::write(
        dir.join("blocks.md"),
        r#"@yah:ticket(T08, "first")

@yah:ticket(T09, "second")
"#,
    )
    .unwrap();

    let anns = extract_from_workspace(&dir).unwrap();
    let anchors: Vec<usize> = anns
        .iter()
        .filter_map(|a| match &a.target {
            AnnotationTarget::File { anchor, .. } => Some(*anchor),
            _ => None,
        })
        .collect();
    assert_eq!(anchors.len(), 2);
    assert_ne!(
        anchors[0], anchors[1],
        "blank line should break the block so each ticket gets its own anchor"
    );

    let board = TicketBoard::from_annotations(&anns);
    assert!(board.get("T08").is_some());
    assert!(board.get("T09").is_some());
}

#[test]
fn file_target_id_includes_anchor() {
    let target = AnnotationTarget::File {
        path: PathBuf::from("foo/bar.md"),
        anchor: 42,
    };
    assert_eq!(target.id(), "file:foo/bar.md#42");
}

#[test]
fn parse_arch_layer_on_md_works() {
    // @arch: annotations should also flow through line_extract — even
    // though the arch graph doesn't yet treat File targets as first-class
    // nodes (P4 in the design doc), the extraction itself shouldn't drop
    // them.
    let dir = temp_workspace("md_arch");
    fs::write(
        dir.join("arch.md"),
        r#"@arch:layer(docs)
@arch:see(architecture/foo.md)
"#,
    )
    .unwrap();

    let anns = extract_from_workspace(&dir).unwrap();
    assert!(anns.iter().any(|a| matches!(&a.kind, ArchKind::Layer(l) if l == "docs")));
    assert!(anns.iter().any(|a| matches!(&a.kind, ArchKind::See(s) if s.contains("foo.md"))));
}
