//! @arch:layer(kg_lang)
//! @arch:role(traverse)
//!
//! JSON / YAML value walker. Operates on a `serde_json::Value` tree
//! (YAML is converted upstream by [`yaml_to_json`]) and pushes the
//! resulting Document / Property / SchemaRef / Anchor nodes into the
//! indexer's `IndexSink`.
//!
//! Spans flow in via a [`crate::spans::SpanLookup`] keyed by
//! JSON-Pointer-style path. JSON wires
//! [`crate::spans::extract_json_spans`] (tree-sitter-json) so every
//! Property node lands on the exact `(line, col)` of its `key: value`
//! pair. YAML and TOML still pass [`crate::spans::NoSpans`] in v1 —
//! they fall back to the file-wide span until the per-format span
//! sources land (yaml-rust2 events, `toml_edit`).

use std::collections::HashMap;
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::indexer::IndexSink;
use yah_kg::kind::{CommonKind, DocKind, Lang, NodeKind};

use crate::spans::{NoSpans, SpanLookup};

/// YAML-only side info captured at parse time.
///
/// `serde_yaml::Value` resolves `&anchor` / `*alias` away — by the time
/// we hold a `Value` they're indistinguishable from inline scalars/maps.
/// We recover the structural info from the `yaml-rust2` token stream
/// (the same scanner libyaml uses) and replay it as Anchor / SchemaRef
/// nodes in the walker. Token-level scanning also gives accurate
/// source-position markers, eliminating the false positives a regex
/// pass would hit (quoted strings, arithmetic `2 * 3`, etc.).
#[derive(Debug, Default, Clone)]
pub struct YamlExtras {
    /// `name → 1-based source line` for every `&name` declaration.
    pub anchors: HashMap<String, u32>,
    /// `(name, line)` for every `*name` alias usage, in source order.
    pub aliases: Vec<(String, u32)>,
}

impl YamlExtras {
    /// Walk the YAML token stream and pull out `&name` / `*name`
    /// occurrences with their source line. Scan errors collapse to
    /// empty extras — the subsequent `serde_yaml` parse will surface
    /// the same syntax issue and the indexer reports it as a parse
    /// failure.
    pub fn scan(src: &str) -> Self {
        use yaml_rust2::scanner::{Scanner, TokenType};
        let mut anchors = HashMap::new();
        let mut aliases = Vec::new();
        let scanner = Scanner::new(src.chars());
        for token in scanner {
            let line = token.0.line() as u32;
            match token.1 {
                TokenType::Anchor(name) => {
                    anchors.entry(name).or_insert(line);
                }
                TokenType::Alias(name) => {
                    aliases.push((name, line));
                }
                _ => {}
            }
        }
        Self { anchors, aliases }
    }
}

/// Convert a `serde_yaml::Value` to a `serde_json::Value` so the walker
/// only knows one shape. YAML-specific bits (anchors, tagged variants)
/// are handled via [`YamlExtras`] / dropped — non-string keys are
/// stringified through their Display form because the JSON/architecture
/// model requires string-keyed maps.
pub fn yaml_to_json(v: &serde_yaml::Value) -> serde_json::Value {
    use serde_json::{Map, Number, Value as J};
    match v {
        serde_yaml::Value::Null => J::Null,
        serde_yaml::Value::Bool(b) => J::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                J::Number(Number::from(i))
            } else if let Some(u) = n.as_u64() {
                J::Number(Number::from(u))
            } else if let Some(f) = n.as_f64() {
                Number::from_f64(f).map(J::Number).unwrap_or(J::Null)
            } else {
                J::Null
            }
        }
        serde_yaml::Value::String(s) => J::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            J::Array(seq.iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, val) in map {
                let key = yaml_key_to_string(k);
                out.insert(key, yaml_to_json(val));
            }
            J::Object(out)
        }
        // Tagged scalars (`!Type value`) drop the tag; the value still
        // surfaces. The graph doesn't model YAML tags in v1.
        serde_yaml::Value::Tagged(t) => yaml_to_json(&t.value),
    }
}

/// Convert a `toml::Value` to a `serde_json::Value` so the walker only
/// sees one shape. TOML datetimes are stringified via Display — the
/// architecture model doesn't track temporal types in v1.
pub fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    use serde_json::{Map, Number, Value as J};
    match v {
        toml::Value::String(s) => J::String(s.clone()),
        toml::Value::Integer(i) => J::Number(Number::from(*i)),
        toml::Value::Float(f) => Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        toml::Value::Boolean(b) => J::Bool(*b),
        toml::Value::Datetime(dt) => J::String(dt.to_string()),
        toml::Value::Array(arr) => J::Array(arr.iter().map(toml_to_json).collect()),
        toml::Value::Table(tbl) => {
            let mut out = Map::with_capacity(tbl.len());
            for (k, val) in tbl {
                out.insert(k.clone(), toml_to_json(val));
            }
            J::Object(out)
        }
    }
}

fn yaml_key_to_string(k: &serde_yaml::Value) -> String {
    match k {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        // Sequences/mappings as keys are legal YAML but rare; render
        // them through the YAML serializer so we still produce a stable
        // string. Failure → opaque sentinel.
        other => serde_yaml::to_string(other)
            .unwrap_or_else(|_| "<unrenderable-key>".to_string())
            .trim()
            .to_string(),
    }
}

pub struct Walker<'a> {
    lang: Lang,
    file: String,
    file_id: NodeId,
    file_span: Span,
    sink: &'a mut dyn IndexSink,
    extras: YamlExtras,
    /// Per-key span lookup. JSON wires this from
    /// [`crate::spans::extract_json_spans`]; YAML/TOML pass [`NoSpans`]
    /// for v1 and rely on the file-wide fallback.
    spans: Box<dyn SpanLookup>,
    /// Anchor node ids keyed by anchor name, populated from
    /// [`YamlExtras::anchors`] up front so alias `*name` lookups can
    /// find them.
    anchor_ids: HashMap<String, NodeId>,
}

impl<'a> Walker<'a> {
    pub fn new(
        lang: Lang,
        file: &str,
        src: &str,
        sink: &'a mut dyn IndexSink,
        extras: YamlExtras,
    ) -> Self {
        Self::with_spans(lang, file, src, sink, extras, Box::new(NoSpans))
    }

    pub fn with_spans(
        lang: Lang,
        file: &str,
        src: &str,
        sink: &'a mut dyn IndexSink,
        extras: YamlExtras,
        spans: Box<dyn SpanLookup>,
    ) -> Self {
        let file = file.replace('\\', "/");
        let file_id = NodeId::compute(lang, &file, &file);
        let line_count = src.lines().count().max(1) as u32;
        let file_span = Span {
            start_line: 1,
            start_col: 1,
            end_line: line_count,
            end_col: 1,
        };
        Self {
            lang,
            file,
            file_id,
            file_span,
            sink,
            extras,
            spans,
            anchor_ids: HashMap::new(),
        }
    }

    pub fn run(&mut self, root: &serde_json::Value) {
        // File node.
        let label = self
            .file
            .rsplit_once('/')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| self.file.clone());
        self.emit_node(NodeRef {
            id: self.file_id,
            lang: self.lang,
            kind: NodeKind::Common(CommonKind::File),
            label,
            qualified: self.file.clone(),
            file: self.file.clone(),
            span: self.file_span,
            synthetic: false,
        });

        // Document node — qualified is `<file>#`. The root pointer
        // (empty string) gets a precise span when JSON spans are
        // wired; otherwise the file-wide fallback stands in.
        let doc_qualified = format!("{}#", self.file);
        let doc_id = self.make_id(&doc_qualified);
        let doc_span = self.spans.span_for("").unwrap_or(self.file_span);
        self.emit_node(NodeRef {
            id: doc_id,
            lang: self.lang,
            kind: NodeKind::Common(CommonKind::Document),
            label: "<root>".to_string(),
            qualified: doc_qualified,
            file: self.file.clone(),
            span: doc_span,
            synthetic: false,
        });
        self.emit_edge(self.file_id, doc_id, EdgeKind::Contains);
        self.sink
            .push_property(doc_id, "value_kind", value_kind(root));

        // Pre-emit Anchor nodes so alias-resolution finds them.
        let anchors = std::mem::take(&mut self.extras.anchors);
        for (name, line) in &anchors {
            let qualified = format!("{}#&{}", self.file, name);
            let id = self.make_id(&qualified);
            let span = Span::point(*line, 1);
            self.emit_node(NodeRef {
                id,
                lang: self.lang,
                kind: NodeKind::Doc(DocKind::Anchor),
                label: name.clone(),
                qualified,
                file: self.file.clone(),
                span,
                synthetic: false,
            });
            self.emit_edge(doc_id, id, EdgeKind::Contains);
            self.anchor_ids.insert(name.clone(), id);
        }
        self.extras.anchors = anchors;

        // Walk the value tree.
        self.walk(doc_id, "", root);

        // Emit RefersTo edges for YAML aliases (`*name`). The synthetic
        // SchemaRef node is one-per-alias-use so each call site has its
        // own line.
        let aliases = std::mem::take(&mut self.extras.aliases);
        for (name, line) in aliases {
            let target = match self.anchor_ids.get(&name) {
                Some(t) => *t,
                None => continue,
            };
            let qualified = format!("{}#*{}@{}", self.file, name, line);
            let id = self.make_id(&qualified);
            self.emit_node(NodeRef {
                id,
                lang: self.lang,
                kind: NodeKind::Doc(DocKind::SchemaRef),
                label: format!("*{}", name),
                qualified,
                file: self.file.clone(),
                span: Span::point(line, 1),
                synthetic: false,
            });
            self.emit_edge(doc_id, id, EdgeKind::Contains);
            self.sink.push_property(id, "target", &name);
            self.sink.push_property(id, "ref_kind", "yaml_alias");
            self.emit_edge(id, target, EdgeKind::RefersTo);
        }
    }

    /// Walk one value. `parent` is the id of the enclosing Document or
    /// Property. `pointer` is a JSON-Pointer-style path from the
    /// document root (empty string at the root).
    fn walk(&mut self, parent: NodeId, pointer: &str, value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    self.walk_property(parent, pointer, key, child);
                }
            }
            serde_json::Value::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    let key = idx.to_string();
                    self.walk_property(parent, pointer, &key, item);
                }
            }
            // Scalar leaves at the root level: nothing to recurse into;
            // the document already carries `value_kind`.
            _ => {}
        }
    }

    fn walk_property(
        &mut self,
        parent: NodeId,
        parent_pointer: &str,
        key: &str,
        value: &serde_json::Value,
    ) {
        let pointer = format!("{}/{}", parent_pointer, escape_pointer_token(key));
        let qualified = format!("{}#{}", self.file, pointer);
        let id = self.make_id(&qualified);

        // Treat `$schema` / `$ref` / `extends` as ref-bearing keys at
        // any depth. The document's parent is its `Document` node, so
        // the conforms-to edge originates from the document.
        let is_root_level = parent_pointer.is_empty();
        let kind = if matches!(key, "$ref" | "$schema") || (is_root_level && key == "extends") {
            NodeKind::Doc(DocKind::SchemaRef)
        } else {
            NodeKind::Doc(DocKind::Property)
        };

        let is_ref = matches!(kind, NodeKind::Doc(DocKind::SchemaRef));
        let span = self.spans.span_for(&pointer).unwrap_or(self.file_span);
        self.emit_node(NodeRef {
            id,
            lang: self.lang,
            kind,
            label: key.to_string(),
            qualified,
            file: self.file.clone(),
            span,
            synthetic: false,
        });
        self.emit_edge(parent, id, EdgeKind::Contains);
        self.sink.push_property(id, "value_kind", value_kind(value));
        if let Some(scalar) = scalar_value(value) {
            self.sink.push_property(id, "value", &scalar);
        }

        // Ref-bearing keys: emit the resolution edge.
        if is_ref {
            if let serde_json::Value::String(target) = value {
                self.sink.push_property(id, "target", target);
                self.sink.push_property(id, "ref_kind", key);
                if let Some(target_id) = self.resolve_ref_target(key, target) {
                    let edge = match key {
                        "$schema" => EdgeKind::ConformsTo,
                        _ => EdgeKind::RefersTo,
                    };
                    self.emit_edge(id, target_id, edge);
                }
            }
            // Don't recurse — `$ref`/`$schema` values are leaves.
            return;
        }

        // Recurse into nested objects/arrays.
        self.walk(id, &pointer, value);
    }

    fn resolve_ref_target(&self, _key: &str, target: &str) -> Option<NodeId> {
        // YAML alias-style `*name` (rare in JSON but legal here).
        if let Some(name) = target.strip_prefix('*') {
            return self.anchor_ids.get(name).copied();
        }
        // Same-doc fragment — `#/path` resolves to a property in this doc.
        if let Some(fragment) = target.strip_prefix('#') {
            let qualified = format!("{}#{}", self.file, fragment);
            return Some(NodeId::compute(self.lang, &qualified, &self.file));
        }
        // Cross-file: `path#fragment` or bare path.
        let (path_part, fragment) = match target.split_once('#') {
            Some((p, f)) => (p, format!("#{}", f)),
            None => (target, "#".to_string()),
        };
        if path_part.is_empty() {
            return None;
        }
        let resolved = normalize_relative_path(&self.file, path_part);
        let target_lang = if resolved.ends_with(".yaml") || resolved.ends_with(".yml") {
            Lang::Yaml
        } else if resolved.ends_with(".toml") {
            Lang::Toml
        } else {
            Lang::Json
        };
        let qualified = format!("{}{}", resolved, fragment);
        Some(NodeId::compute(target_lang, &qualified, &resolved))
    }

    fn emit_node(&mut self, node: NodeRef) {
        self.sink.push_node(node);
    }

    fn emit_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        self.sink.push_edge(EdgeOut {
            id: EdgeId::compute(from, to, &kind),
            from,
            to,
            kind,
            annotations: vec![],
        });
    }

    fn make_id(&self, qualified: &str) -> NodeId {
        NodeId::compute(self.lang, qualified, &self.file)
    }
}

fn value_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn scalar_value(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Null => Some("null".to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// JSON-Pointer escaping per RFC 6901: `~` → `~0`, `/` → `~1`.
fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Resolve `target` (a relative or absolute reference) against `from`
/// (the file currently being indexed). Output is a forward-slash,
/// rig-relative path with `./` and `../` collapsed. Absolute refs
/// (`/...` or scheme-prefixed) are returned unchanged.
fn normalize_relative_path(from: &str, target: &str) -> String {
    if target.starts_with('/') || target.contains("://") {
        return target.to_string();
    }
    let mut base: Vec<&str> = from.split('/').collect();
    base.pop(); // drop the file name
    let mut parts: Vec<String> = base.iter().map(|s| s.to_string()).collect();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_token_escape() {
        assert_eq!(escape_pointer_token("foo"), "foo");
        assert_eq!(escape_pointer_token("a/b"), "a~1b");
        assert_eq!(escape_pointer_token("a~b"), "a~0b");
    }

    #[test]
    fn relative_path_collapses_dots() {
        assert_eq!(
            normalize_relative_path("a/b/c.json", "./d.json"),
            "a/b/d.json"
        );
        assert_eq!(
            normalize_relative_path("a/b/c.json", "../d.json"),
            "a/d.json"
        );
        assert_eq!(
            normalize_relative_path("a/b/c.json", "../../d/e.json"),
            "d/e.json"
        );
    }

    #[test]
    fn yaml_extras_picks_up_anchors_and_aliases() {
        let src = "defaults: &defaults\n  timeout: 30\ntest:\n  <<: *defaults\n";
        let extras = YamlExtras::scan(src);
        assert_eq!(extras.anchors.get("defaults").copied(), Some(1));
        assert_eq!(extras.aliases.len(), 1);
        assert_eq!(extras.aliases[0].0, "defaults");
        assert_eq!(extras.aliases[0].1, 4);
    }

    #[test]
    fn yaml_extras_event_stream_avoids_false_positives() {
        let src = "# &not_real\nname: 2 * 3\nval: \"&fake\"\nlist:\n  - &real one\n  - *real\n";
        let extras = YamlExtras::scan(src);
        // Comment-only `&not_real` is skipped by the YAML scanner.
        assert!(extras.anchors.get("not_real").is_none());
        // Quoted scalars containing `&` are not anchors.
        assert!(extras.anchors.get("fake").is_none());
        // Arithmetic `2 * 3` is a plain scalar — it must NOT register
        // `3` as an alias. (The legacy regex pass had this bug.)
        assert!(!extras.aliases.iter().any(|(n, _)| n == "3"));
        // Real `&real` declaration + `*real` use are still picked up.
        assert!(extras.anchors.get("real").is_some());
        assert!(extras.aliases.iter().any(|(n, _)| n == "real"));
    }
}
