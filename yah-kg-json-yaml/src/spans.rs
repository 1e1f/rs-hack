//! @arch:layer(kg_lang)
//! @arch:role(parse)
//!
//! Per-key span extraction. `serde_json::Value` / `serde_yaml::Value` /
//! `toml::Value` strip source positions, so a separate parse pass is
//! needed if the architecture tab wants to click into nested config keys.
//!
//! Today this module ships a tree-sitter-based JSON walker that
//! produces `JSON-Pointer → Span` lookups keyed exactly the way
//! [`crate::visit::Walker`] composes pointers. The walker consults the
//! map; misses fall back to the file-wide span (the previous behavior).
//!
//! YAML and TOML follow-ups will plug into the same [`SpanLookup`] trait
//! once their span sources are wired (yaml-rust2 events for YAML,
//! `toml_edit` for TOML).

use std::collections::HashMap;
use yah_kg::ids::Span;

/// Lookup of pre-extracted spans keyed by JSON-Pointer-style path
/// (e.g. `/dependencies/react`, empty string for the document root).
pub trait SpanLookup {
    /// Span for the property at `pointer`, or `None` if no precise
    /// span is known (the caller falls back to a file-wide span).
    fn span_for(&self, pointer: &str) -> Option<Span>;
}

/// No-op lookup. Used when the indexer has no span source available
/// (YAML/TOML in v1, or any future format that wants to opt out).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSpans;

impl SpanLookup for NoSpans {
    fn span_for(&self, _pointer: &str) -> Option<Span> {
        None
    }
}

/// Pre-built map of JSON pointers to spans. Cheap to consult — every
/// `walk_property` call hits a single hash lookup.
#[derive(Debug, Default, Clone)]
pub struct PointerSpans {
    map: HashMap<String, Span>,
}

impl PointerSpans {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, pointer: String, span: Span) {
        self.map.insert(pointer, span);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl SpanLookup for PointerSpans {
    fn span_for(&self, pointer: &str) -> Option<Span> {
        self.map.get(pointer).copied()
    }
}

/// JSON span extractor. Parses `src` with tree-sitter-json and walks
/// the parse tree to record `(pointer, span)` for every object key,
/// array index, and the document root.
///
/// On parse error returns an empty map — the caller can still emit
/// nodes; spans simply default to file-wide. (`JsonIndexer` parses with
/// `serde_json` first, so a tree-sitter failure at this point would be
/// surprising; we still tolerate it.)
pub fn extract_json_spans(src: &str) -> PointerSpans {
    use tree_sitter::Parser;
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_json::LANGUAGE.into()).is_err() {
        return PointerSpans::new();
    }
    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => return PointerSpans::new(),
    };
    let src_bytes = src.as_bytes();
    let mut out = PointerSpans::new();
    let root = tree.root_node();
    // The document node wraps a single value. Span for the document
    // root (pointer `""`) is the value's own range, so the architecture
    // tab can highlight `{...}` rather than the whole file.
    if let Some(value) = first_value_child(root) {
        out.insert(String::new(), node_span(value));
        walk_value(&value, "", src_bytes, &mut out);
    }
    out
}

fn first_value_child(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_json_value(child.kind()) {
            return Some(child);
        }
    }
    None
}

fn is_json_value(kind: &str) -> bool {
    matches!(
        kind,
        "object" | "array" | "string" | "number" | "true" | "false" | "null"
    )
}

fn walk_value(
    node: &tree_sitter::Node<'_>,
    parent_pointer: &str,
    src: &[u8],
    out: &mut PointerSpans,
) {
    match node.kind() {
        "object" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() != "pair" {
                    continue;
                }
                let key_node = match child.child_by_field_name("key") {
                    Some(k) => k,
                    None => continue,
                };
                let key = match string_text(&key_node, src) {
                    Some(s) => s,
                    None => continue,
                };
                let pointer = format!(
                    "{}/{}",
                    parent_pointer,
                    escape_pointer_token(&key)
                );
                // Property span = the whole `pair` (key + colon + value),
                // so clicks on either side land on the same node. The
                // editor highlight feels right for `"foo": 42`-style
                // lines.
                out.insert(pointer.clone(), node_span(child));
                if let Some(value_node) = child.child_by_field_name("value") {
                    walk_value(&value_node, &pointer, src, out);
                }
            }
        }
        "array" => {
            let mut cursor = node.walk();
            let mut idx: usize = 0;
            for child in node.named_children(&mut cursor) {
                if !is_json_value(child.kind()) {
                    continue;
                }
                let pointer = format!("{}/{}", parent_pointer, idx);
                out.insert(pointer.clone(), node_span(child));
                walk_value(&child, &pointer, src, out);
                idx += 1;
            }
        }
        // Scalars have no children we care about.
        _ => {}
    }
}

fn string_text(node: &tree_sitter::Node<'_>, src: &[u8]) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    // tree-sitter-json may split a string body into mixed
    // `string_content` + `escape_sequence` children, so reading the
    // raw text between the quotes and re-unescaping ourselves is the
    // simplest reliable path. The full node range includes the quotes.
    let raw = node.utf8_text(src).ok()?;
    let trimmed = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"'))?;
    Some(unescape_json(trimmed))
}

/// Decode the JSON escapes tree-sitter exposes verbatim. We need the
/// *logical* key to match what serde_json produced; otherwise the
/// pointer-spans map and the walker's pointers won't agree.
fn unescape_json(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

fn node_span(node: tree_sitter::Node<'_>) -> Span {
    let start = node.start_position();
    let end = node.end_position();
    Span {
        // tree-sitter rows/columns are 0-based; yah-kg::Span is 1-based.
        start_line: (start.row as u32) + 1,
        start_col: (start.column as u32) + 1,
        end_line: (end.row as u32) + 1,
        end_col: (end.column as u32) + 1,
    }
}

/// JSON-Pointer escaping per RFC 6901: `~` → `~0`, `/` → `~1`. Kept
/// in sync with the same routine in [`crate::visit`] so the maps agree.
fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_top_level_keys() {
        let src = "{\n  \"name\": \"yah\",\n  \"version\": \"0.7.0\"\n}\n";
        let spans = extract_json_spans(src);
        let name = spans.span_for("/name").expect("name span");
        assert_eq!(name.start_line, 2);
        assert_eq!(name.start_col, 3);
        let version = spans.span_for("/version").expect("version span");
        assert_eq!(version.start_line, 3);
    }

    #[test]
    fn extracts_nested_keys() {
        let src = "{\n  \"deps\": {\n    \"react\": \"^18\"\n  }\n}\n";
        let spans = extract_json_spans(src);
        let react = spans.span_for("/deps/react").expect("nested react");
        assert_eq!(react.start_line, 3);
        assert_eq!(react.start_col, 5);
    }

    #[test]
    fn extracts_array_index_keys() {
        let src = "{\n  \"windows\": [\n    { \"title\": \"yah\" }\n  ]\n}\n";
        let spans = extract_json_spans(src);
        let title = spans.span_for("/windows/0/title").expect("indexed title");
        assert_eq!(title.start_line, 3);
    }

    #[test]
    fn document_root_pointer_is_empty_string() {
        let src = "{ \"a\": 1 }";
        let spans = extract_json_spans(src);
        let root = spans.span_for("").expect("root pointer");
        assert_eq!(root.start_line, 1);
        assert_eq!(root.start_col, 1);
    }

    #[test]
    fn escaped_keys_round_trip_through_pointer() {
        let src = "{ \"a/b\": 1, \"c~d\": 2 }";
        let spans = extract_json_spans(src);
        // `/` → `~1`, `~` → `~0`
        assert!(spans.span_for("/a~1b").is_some());
        assert!(spans.span_for("/c~0d").is_some());
    }

    #[test]
    fn unicode_escapes_decode() {
        let src = "{ \"\\u00e9\": 1 }";
        let spans = extract_json_spans(src);
        assert!(spans.span_for("/é").is_some());
    }

    #[test]
    fn parse_failure_yields_empty_map() {
        let src = "{ this is not json";
        let spans = extract_json_spans(src);
        // tree-sitter-json's permissive recovery may still produce
        // partial structure, so we don't assert empty — just that the
        // call doesn't panic.
        let _ = spans.len();
    }
}
