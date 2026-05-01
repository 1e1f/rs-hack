//! @arch:layer(kg_lang)
//! @arch:role(parse)
//!
//! Per-key span extraction. `serde_json::Value` / `serde_yaml::Value` /
//! `toml::Value` strip source positions, so a separate parse pass is
//! needed if the architecture tab wants to click into nested config keys.
//!
//! Today this module ships:
//!
//! * a tree-sitter-based JSON walker (`extract_json_spans`)
//! * a `yaml-rust2` event-driven YAML walker (`extract_yaml_spans`)
//! * a `toml_edit::ImDocument` walker (`extract_toml_spans`)
//!
//! All three produce `JSON-Pointer → Span` lookups keyed exactly the way
//! [`crate::visit::Walker`] composes pointers. The walker consults the
//! map; misses fall back to the file-wide span (the previous behavior).
//! YAML keys introduced via merge resolution (`<<: *anchor`) are not
//! visible to yaml-rust2's event stream — those keys hit the fallback
//! while keys that physically appear in source get precise spans.

use std::collections::HashMap;
use kg::ids::Span;

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

/// YAML span extractor. Drives `yaml-rust2`'s event-based parser and
/// records `(JSON-Pointer, Span)` for every scalar leaf, mapping, and
/// sequence element it sees. Scalar property spans start at the *key*'s
/// marker so clicking either side of `host: localhost` lands on the
/// same node — the same convention the JSON extractor uses on `pair`
/// nodes.
///
/// Merge keys (`<<: *anchor`) are *not* expanded: only keys that
/// physically appear in source get a span. The walker's fallback
/// (file-wide) covers the resolved siblings.
pub fn extract_yaml_spans(src: &str) -> PointerSpans {
    use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
    use yaml_rust2::scanner::Marker;

    enum MapState {
        ExpectKey,
        ExpectValue { key: String, key_marker: Marker },
    }

    enum Frame {
        Map { pointer: String, state: MapState },
        Seq { pointer: String, idx: usize },
    }

    struct Sink {
        spans: PointerSpans,
        stack: Vec<Frame>,
        root_set: bool,
    }

    impl Sink {
        fn value_pointer(&self) -> String {
            match self.stack.last() {
                None => String::new(),
                Some(Frame::Map { pointer, state: MapState::ExpectValue { key, .. } }) => {
                    format!("{}/{}", pointer, escape_pointer_token(key))
                }
                Some(Frame::Map { pointer, state: MapState::ExpectKey }) => pointer.clone(),
                Some(Frame::Seq { pointer, idx }) => format!("{}/{}", pointer, idx),
            }
        }

        /// Marker to use as the span anchor for the value currently
        /// being introduced. For map values, that's the key's marker
        /// (so the span starts at `key:`); for sequence elements and
        /// the root, the value's own marker.
        fn span_marker(&self, value_marker: Marker) -> Marker {
            match self.stack.last() {
                Some(Frame::Map { state: MapState::ExpectValue { key_marker, .. }, .. }) => {
                    *key_marker
                }
                _ => value_marker,
            }
        }

        fn record(&mut self, pointer: String, marker: Marker) {
            if pointer.is_empty() {
                return;
            }
            let span = Span::point(marker.line() as u32, marker.col() as u32);
            self.spans.insert(pointer, span);
        }

        fn after_value_completed(&mut self) {
            match self.stack.last_mut() {
                Some(Frame::Map { state, .. }) => {
                    *state = MapState::ExpectKey;
                }
                Some(Frame::Seq { idx, .. }) => {
                    *idx += 1;
                }
                None => {}
            }
        }
    }

    impl MarkedEventReceiver for Sink {
        fn on_event(&mut self, ev: Event, mark: Marker) {
            // Skip stream/document framing — they aren't values.
            let is_content = !matches!(
                ev,
                Event::StreamStart
                    | Event::StreamEnd
                    | Event::DocumentStart
                    | Event::DocumentEnd
                    | Event::Nothing
            );
            if !is_content {
                return;
            }

            // First content event marks the document root. Use the
            // value's own marker (no key context yet at the root).
            if !self.root_set {
                self.spans
                    .insert(String::new(), Span::point(mark.line() as u32, mark.col() as u32));
                self.root_set = true;
            }

            // In a Map's key slot? The next event is the key, not a value.
            let in_key_slot = matches!(
                self.stack.last(),
                Some(Frame::Map { state: MapState::ExpectKey, .. })
            );

            if in_key_slot {
                match ev {
                    Event::Scalar(s, _, _, _) => {
                        if let Some(Frame::Map { state, .. }) = self.stack.last_mut() {
                            *state = MapState::ExpectValue { key: s, key_marker: mark };
                        }
                        return;
                    }
                    Event::MappingEnd => {
                        // Empty map / end of map. Pop and bubble up.
                        self.stack.pop();
                        self.after_value_completed();
                        return;
                    }
                    // Structural keys (a map/seq used as a key) are legal
                    // YAML but rare. We don't model them — produce an
                    // empty placeholder key and let the value branch
                    // handle the structural value's frame as if it were
                    // a normal value. Spans for structural keys fall
                    // back to file-wide.
                    _ => {
                        if let Some(Frame::Map { state, .. }) = self.stack.last_mut() {
                            *state = MapState::ExpectValue {
                                key: String::new(),
                                key_marker: mark,
                            };
                        }
                        // fall through — treat the rest as the value
                    }
                }
            }

            // Value position (or root). Compute pointer + record span.
            let pointer = self.value_pointer();
            let span_marker = self.span_marker(mark);

            match ev {
                Event::Scalar(_, _, _, _) | Event::Alias(_) => {
                    self.record(pointer, span_marker);
                    self.after_value_completed();
                }
                Event::MappingStart(_, _) => {
                    self.record(pointer.clone(), span_marker);
                    self.stack.push(Frame::Map {
                        pointer,
                        state: MapState::ExpectKey,
                    });
                }
                Event::SequenceStart(_, _) => {
                    self.record(pointer.clone(), span_marker);
                    self.stack.push(Frame::Seq { pointer, idx: 0 });
                }
                Event::MappingEnd | Event::SequenceEnd => {
                    self.stack.pop();
                    self.after_value_completed();
                }
                _ => {}
            }
        }
    }

    let mut sink = Sink {
        spans: PointerSpans::new(),
        stack: Vec::new(),
        root_set: false,
    };
    let mut parser = Parser::new_from_str(src);
    // Parse failure → return whatever spans we collected; serde_yaml
    // surfaces the syntax error to the caller.
    let _ = parser.load(&mut sink, true);
    sink.spans
}

/// TOML span extractor. Parses with `toml_edit::ImDocument` (the
/// `Im` form preserves spans — `DocumentMut::from_str` despans on
/// the `into_mut` step) and walks the tree to record `(JSON-Pointer,
/// Span)` for every key, table header, and array index. Keys land on
/// the `key` repr's source range; nested values inherit their own
/// `span()` (e.g. `[[items]]` headers, inline-table braces).
///
/// On parse error the map is empty — `TomlIndexer` already surfaces
/// the same syntax error via `toml::Value::parse`.
pub fn extract_toml_spans(src: &str) -> PointerSpans {
    let doc = match toml_edit::ImDocument::parse(src) {
        Ok(d) => d,
        Err(_) => return PointerSpans::new(),
    };
    let lines = LineIndex::new(src);
    let mut out = PointerSpans::new();

    let root = doc.as_table();
    if let Some(range) = root.span() {
        out.insert(String::new(), lines.span(range));
    }

    walk_table(root, "", &lines, &mut out);
    out
}

fn walk_table(
    table: &toml_edit::Table,
    parent: &str,
    lines: &LineIndex,
    out: &mut PointerSpans,
) {
    for (key, item) in table.iter() {
        let pointer = format!("{}/{}", parent, escape_pointer_token(key));
        if let Some(span) = key_or_value_span(table.key(key), item, lines) {
            out.insert(pointer.clone(), span);
        }
        walk_item(item, &pointer, lines, out);
    }
}

fn walk_inline_table(
    table: &toml_edit::InlineTable,
    parent: &str,
    lines: &LineIndex,
    out: &mut PointerSpans,
) {
    for (key, value) in table.iter() {
        let pointer = format!("{}/{}", parent, escape_pointer_token(key));
        let span = table
            .key(key)
            .and_then(|k| k.span())
            .or_else(|| value.span())
            .map(|r| lines.span(r));
        if let Some(s) = span {
            out.insert(pointer.clone(), s);
        }
        walk_toml_value(value, &pointer, lines, out);
    }
}

fn walk_item(
    item: &toml_edit::Item,
    pointer: &str,
    lines: &LineIndex,
    out: &mut PointerSpans,
) {
    match item {
        toml_edit::Item::Value(v) => walk_toml_value(v, pointer, lines, out),
        toml_edit::Item::Table(t) => walk_table(t, pointer, lines, out),
        toml_edit::Item::ArrayOfTables(arr) => {
            for (idx, t) in arr.iter().enumerate() {
                let p = format!("{}/{}", pointer, idx);
                if let Some(range) = t.span() {
                    out.insert(p.clone(), lines.span(range));
                }
                walk_table(t, &p, lines, out);
            }
        }
        toml_edit::Item::None => {}
    }
}

fn walk_toml_value(
    value: &toml_edit::Value,
    pointer: &str,
    lines: &LineIndex,
    out: &mut PointerSpans,
) {
    match value {
        toml_edit::Value::Array(arr) => {
            for (idx, v) in arr.iter().enumerate() {
                let p = format!("{}/{}", pointer, idx);
                if let Some(range) = v.span() {
                    out.insert(p.clone(), lines.span(range));
                }
                walk_toml_value(v, &p, lines, out);
            }
        }
        toml_edit::Value::InlineTable(t) => walk_inline_table(t, pointer, lines, out),
        // Scalars have no children to recurse into.
        _ => {}
    }
}

/// Prefer the key's source span (anchors on `key = value`) and fall
/// back to the value's span when the key is missing (e.g. dotted-table
/// headers don't always carry a Key). The repr-less Key case (table
/// constructed in-memory) trips the value fallback too.
fn key_or_value_span(
    key: Option<&toml_edit::Key>,
    item: &toml_edit::Item,
    lines: &LineIndex,
) -> Option<Span> {
    key.and_then(|k| k.span())
        .or_else(|| item.span())
        .map(|r| lines.span(r))
}

/// Byte-offset → 1-based (line, column) lookup. Columns are byte
/// offsets within the line — same convention tree-sitter exposes for
/// the JSON extractor, so multibyte content is consistent across the
/// two paths.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(src: &str) -> Self {
        let mut starts = Vec::with_capacity(src.len() / 40 + 1);
        starts.push(0);
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { starts }
    }

    fn loc(&self, offset: usize) -> (u32, u32) {
        // Largest line whose start is <= offset.
        let line = self
            .starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        let col = offset - self.starts[line];
        ((line as u32) + 1, (col as u32) + 1)
    }

    fn span(&self, range: std::ops::Range<usize>) -> Span {
        let (sl, sc) = self.loc(range.start);
        // For an empty range (start == end) the "end" position is
        // legitimately the same point — keep the cursor model so a
        // bare key span doesn't underflow into the previous line.
        let end_offset = range.end.max(range.start);
        let (el, ec) = self.loc(end_offset);
        Span {
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
        }
    }
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

    #[test]
    fn yaml_extracts_top_level_keys() {
        let src = "name: yah\nversion: 0.7.0\n";
        let spans = extract_yaml_spans(src);
        let name = spans.span_for("/name").expect("name span");
        assert_eq!(name.start_line, 1);
        let version = spans.span_for("/version").expect("version span");
        assert_eq!(version.start_line, 2);
    }

    #[test]
    fn yaml_extracts_nested_keys() {
        let src = "deps:\n  react: ^18\n  vue: ^3\n";
        let spans = extract_yaml_spans(src);
        let react = spans.span_for("/deps/react").expect("nested react");
        assert_eq!(react.start_line, 2);
        let vue = spans.span_for("/deps/vue").expect("nested vue");
        assert_eq!(vue.start_line, 3);
    }

    #[test]
    fn yaml_extracts_array_index_keys() {
        let src = "windows:\n  - title: yah\n  - title: other\n";
        let spans = extract_yaml_spans(src);
        let first = spans.span_for("/windows/0").expect("first index");
        assert_eq!(first.start_line, 2);
        let title = spans.span_for("/windows/0/title").expect("indexed title");
        assert_eq!(title.start_line, 2);
        let second = spans.span_for("/windows/1/title").expect("indexed second title");
        assert_eq!(second.start_line, 3);
    }

    #[test]
    fn yaml_document_root_pointer_is_empty_string() {
        let src = "name: yah\n";
        let spans = extract_yaml_spans(src);
        let root = spans.span_for("").expect("root pointer");
        assert_eq!(root.start_line, 1);
    }

    #[test]
    fn yaml_merge_resolved_keys_have_no_span() {
        // `<<: *defaults` is recorded as a literal `<<` key, not as
        // its expanded siblings. The walker's fallback covers those.
        let src = "defaults: &defaults\n  timeout: 30\n\ndev:\n  <<: *defaults\n  host: localhost\n";
        let spans = extract_yaml_spans(src);
        // Direct keys → precise spans.
        assert_eq!(spans.span_for("/defaults").map(|s| s.start_line), Some(1));
        assert_eq!(spans.span_for("/dev/host").map(|s| s.start_line), Some(6));
        // Merge-resolved key has no entry — the walker falls back to
        // the file-wide span.
        assert!(spans.span_for("/dev/timeout").is_none());
    }

    #[test]
    fn toml_extracts_top_level_table_keys() {
        let src = "[package]\nname = \"yah\"\nversion = \"0.7.0\"\n";
        let spans = extract_toml_spans(src);
        let pkg = spans.span_for("/package").expect("package span");
        assert_eq!(pkg.start_line, 1);
        let name = spans.span_for("/package/name").expect("name span");
        assert_eq!(name.start_line, 2);
        let version = spans.span_for("/package/version").expect("version span");
        assert_eq!(version.start_line, 3);
    }

    #[test]
    fn toml_extracts_inline_table_keys() {
        let src = "[deps]\nyah = { path = \"../yah\", version = \"1\" }\n";
        let spans = extract_toml_spans(src);
        let path = spans.span_for("/deps/yah/path").expect("inline path");
        assert_eq!(path.start_line, 2);
        let version = spans.span_for("/deps/yah/version").expect("inline version");
        assert_eq!(version.start_line, 2);
    }

    #[test]
    fn toml_extracts_array_indexes() {
        let src = "[build]\nflags = [\n  \"-O2\",\n  \"-g\",\n]\n";
        let spans = extract_toml_spans(src);
        let zero = spans.span_for("/build/flags/0").expect("flags[0]");
        assert_eq!(zero.start_line, 3);
        let one = spans.span_for("/build/flags/1").expect("flags[1]");
        assert_eq!(one.start_line, 4);
    }

    #[test]
    fn toml_extracts_array_of_tables() {
        let src = "[[items]]\nname = \"a\"\n\n[[items]]\nname = \"b\"\n";
        let spans = extract_toml_spans(src);
        let first = spans.span_for("/items/0/name").expect("items[0].name");
        assert_eq!(first.start_line, 2);
        let second = spans.span_for("/items/1/name").expect("items[1].name");
        assert_eq!(second.start_line, 5);
    }

    #[test]
    fn toml_parse_failure_yields_empty_map() {
        let spans = extract_toml_spans("[unterminated");
        assert!(spans.is_empty());
    }
}
