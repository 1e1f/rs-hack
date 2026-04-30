//! @arch:layer(kg_store)
//! @arch:role(extract)
//!
//! `yah-kg-anno-wasm` — wasm-bindgen target on top of [`yah_kg_anno::parser`].
//!
//! The Files-tab KG-overlay extension (see
//! `architecture/yah-files-tab.md`) loads this module to surface live
//! `@yah:` diagnostics in Monaco as the user types — without round-tripping
//! to `yah serve`. The parser itself is the same code that runs server-side
//! during Pass 4 of the indexer; compiling it to wasm is the cheapest way
//! to keep the editor and the daemon agreeing on what is well-formed.
//!
//! Wire shape: [`parse_doc`] takes the doc-comment text (markers like
//! `///`/`//!`/`*` already stripped by the caller — same contract as
//! [`yah_kg_anno::parse_doc`]) and returns a JS object with two arrays:
//!
//! ```text
//! { annotations: ParsedAnnotation[], errors: WireError[] }
//! ```
//!
//! `ParsedAnnotation` is the `Serialize`-derived form of
//! [`yah_kg_anno::ParsedAnnotation`]; `WireError` is a thin DTO around
//! [`yah_kg_anno::ParseError`] that flattens the enum into named fields
//! so the TS side gets `{kind, line, message}` without a serde tag.
//!
//! @arch:see(architecture/yah-files-tab.md)

use serde::Serialize;
use wasm_bindgen::prelude::*;
use yah_kg_anno::{parse_doc as parse_doc_native, ParseError, ParsedAnnotation};

/// Wire form of [`yah_kg_anno::ParseError`]. The native enum is
/// `thiserror`-shaped (one variant carries `kind`/`line`/`message`); the
/// TS side wants plain fields, not a serde-tagged enum.
#[derive(Serialize)]
struct WireError {
    /// Directive kind that failed to parse (`"status"`, `"flow"`, …).
    kind: String,
    /// 1-based line within the input doc string.
    line: u32,
    /// Human-readable diagnostic — safe to render verbatim in a tooltip.
    message: String,
}

impl From<&ParseError> for WireError {
    fn from(e: &ParseError) -> Self {
        match e {
            ParseError::Malformed { kind, line, message } => Self {
                kind: kind.clone(),
                line: *line,
                message: message.clone(),
            },
        }
    }
}

#[derive(Serialize)]
struct ParseResult<'a> {
    annotations: &'a [ParsedAnnotation],
    errors: Vec<WireError>,
}

/// Parse a doc string for `@yah:` directives. See the module-level docs
/// for the wire shape; semantics match [`yah_kg_anno::parse_doc`] exactly
/// (same input contract, same fenced-block / inline-backtick rules).
#[wasm_bindgen(js_name = parseDoc)]
pub fn parse_doc(doc: &str) -> Result<JsValue, JsValue> {
    let (annotations, errors) = parse_doc_native(doc);
    let wire_errors: Vec<WireError> = errors.iter().map(WireError::from).collect();
    let result = ParseResult {
        annotations: &annotations,
        errors: wire_errors,
    };
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yah_kg_anno::{RawAnnotation, WorkItemType};

    #[test]
    fn parse_doc_native_round_trip() {
        let (out, errs) = parse_doc_native(
            "@yah:relay(R001, \"Demo\")\n@yah:status(in-progress)\n@yah:next(\"step\")",
        );
        assert!(errs.is_empty());
        assert_eq!(out.len(), 1);
        let RawAnnotation::WorkItem { item_type, anno } = &out[0].anno else {
            panic!("expected WorkItem, got {:?}", out[0].anno);
        };
        assert_eq!(*item_type, WorkItemType::Relay);
        assert_eq!(anno.id, "R001");
        assert_eq!(anno.next_steps, vec!["step".to_string()]);
    }

    #[test]
    fn wire_error_carries_line_and_message() {
        let (_, errs) = parse_doc_native("@yah:status(open)");
        assert_eq!(errs.len(), 1);
        let wire: WireError = (&errs[0]).into();
        assert_eq!(wire.kind, "status");
        assert_eq!(wire.line, 1);
        assert!(wire.message.contains("modifier without preceding"));
    }
}
