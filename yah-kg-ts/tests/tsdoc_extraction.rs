//! TSDoc / `///` / `//!` doc-comment extraction (R016-F1).
//!
//! Without this, `/** @yah:tag(...) */` and `//! @yah:ticket(...)` blocks
//! authored above TypeScript items were silently ignored — yah-kg-anno had
//! no doc to scan. These tests pin the contract so future indexer work
//! doesn't quietly regress it.

use std::path::Path;
use yah_kg::indexer::LanguageIndexer;
use yah_kg_store::{Store, StoreSink};
use yah_kg_ts::TsIndexer;

fn build(path: &str, src: &str) -> Store {
    let mut store = Store::new();
    {
        let mut sink = StoreSink::new(&mut store);
        TsIndexer::new()
            .index_file(Path::new(path), src, &mut sink)
            .expect("indexer should accept the fixture");
    }
    store
}

fn doc_of(store: &Store, file: &str, label: &str) -> Option<String> {
    store
        .lookup(file, None)
        .into_iter()
        .find_map(|id| {
            let n = store.node_ref(id)?;
            if n.label != label {
                return None;
            }
            store.node_full(id).and_then(|n| n.doc)
        })
}

const FIXTURE: &str = r#"//! @yah:ticket(R999, "fixture file-level doc")
//! Second inner-line.

/** Class-level TSDoc.
 *  @yah:tag(audio)
 */
export class Foo {
  /** Method-level doc */
  bar(): void {}

  /** Field-level doc */
  count: number = 0;
}

/** Interface doc */
export interface IBar {
  /** Property doc */
  name: string;
}

/** Enum doc */
export enum Color {
  /** Variant doc */
  Red,
  Green,
}

/** Type alias doc */
export type Id = string;

/// Triple-slash item doc
function helperFn(): void {}

/** Const doc */
export const FIXED = 42;

/** Namespace doc */
namespace inner {
  export const X = 1;
}
"#;

#[test]
fn block_tsdoc_attaches_to_export_wrapped_class() {
    let store = build("src/fixture.ts", FIXTURE);
    let doc = doc_of(&store, "src/fixture.ts", "Foo").expect("class doc");
    assert!(
        doc.contains("Class-level TSDoc.") && doc.contains("@yah:tag(audio)"),
        "class doc should retain both prose and the @yah:tag directive; got {doc:?}"
    );
}

#[test]
fn block_tsdoc_attaches_to_methods_and_fields() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "bar").as_deref(),
        Some("Method-level doc")
    );
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "count").as_deref(),
        Some("Field-level doc")
    );
}

#[test]
fn block_tsdoc_attaches_to_interface_and_property() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "IBar").as_deref(),
        Some("Interface doc")
    );
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "name").as_deref(),
        Some("Property doc")
    );
}

#[test]
fn block_tsdoc_attaches_to_enum_and_variants() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "Color").as_deref(),
        Some("Enum doc")
    );
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "Red").as_deref(),
        Some("Variant doc")
    );
    // Variants without a doc should not get one from a sibling.
    assert_eq!(doc_of(&store, "src/fixture.ts", "Green"), None);
}

#[test]
fn block_tsdoc_attaches_to_type_alias_and_const() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "Id").as_deref(),
        Some("Type alias doc")
    );
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "FIXED").as_deref(),
        Some("Const doc")
    );
}

#[test]
fn triple_slash_attaches_as_outer_doc() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "helperFn").as_deref(),
        Some("Triple-slash item doc")
    );
}

#[test]
fn namespace_gets_block_doc() {
    let store = build("src/fixture.ts", FIXTURE);
    assert_eq!(
        doc_of(&store, "src/fixture.ts", "inner").as_deref(),
        Some("Namespace doc")
    );
}

#[test]
fn inner_line_comments_attach_to_file_node() {
    let store = build("src/fixture.ts", FIXTURE);
    let doc = doc_of(&store, "src/fixture.ts", "fixture.ts").expect("file doc");
    assert!(
        doc.contains("@yah:ticket(R999")
            && doc.contains("fixture file-level doc")
            && doc.contains("Second inner-line."),
        "file doc should concatenate `//!` lines including the @yah:ticket header; got {doc:?}"
    );
}

#[test]
fn plain_line_comments_are_not_docs() {
    let src = r#"// just a header
function visible(): void {}

// another non-doc
export const ALSO = 1;
"#;
    let store = build("src/plain.ts", src);
    assert_eq!(doc_of(&store, "src/plain.ts", "visible"), None);
    assert_eq!(doc_of(&store, "src/plain.ts", "ALSO"), None);
}

#[test]
fn divider_quadruple_slash_is_not_a_doc() {
    let src = r#"//// ============ visual divider ============
function alpha(): void {}
"#;
    let store = build("src/divider.ts", src);
    assert_eq!(doc_of(&store, "src/divider.ts", "alpha"), None);
}

#[test]
fn single_star_block_comments_are_not_docs() {
    let src = r#"/* not a TSDoc */
function beta(): void {}
"#;
    let store = build("src/single.ts", src);
    assert_eq!(doc_of(&store, "src/single.ts", "beta"), None);
}

#[test]
fn doc_survives_blank_line_inside_block() {
    let src = r#"/**
 * line one
 *
 * line three
 */
export function gamma(): void {}
"#;
    let store = build("src/gamma.ts", src);
    let doc = doc_of(&store, "src/gamma.ts", "gamma").expect("gamma doc");
    assert!(doc.contains("line one") && doc.contains("line three"));
}

#[test]
fn tsx_file_doc_extraction_works_too() {
    let src = r#"//! @yah:ticket(R000, "tsx fixture")

/** Component docs */
export function Widget() {
  return <div />;
}
"#;
    let store = build("src/Widget.tsx", src);
    let file_doc = doc_of(&store, "src/Widget.tsx", "Widget.tsx").expect("file doc");
    assert!(file_doc.contains("@yah:ticket(R000"));
    assert_eq!(
        doc_of(&store, "src/Widget.tsx", "Widget").as_deref(),
        Some("Component docs")
    );
}
