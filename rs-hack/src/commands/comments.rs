//! `comments` command: list every comment span in a set of files, apply a hash-guarded
//! batch of delete/replace edits to them, and verify that code is unchanged once comments
//! are ignored.
//!
//! syn drops `//` and `/* */` comments and turns `///` / `//!` into `#[doc]` attributes, so
//! it cannot answer "where are the comments". This module runs a small lexer over the raw
//! source that understands strings, raw strings, char literals vs lifetimes and nested block
//! comments, and uses syn only for the questions syn is good at: which item a comment is
//! attached to, whether it sits inside a fn body, and whether two files are the same code.
//!
//! Every `apply` is gated by the same `verify` check before anything is written, and records
//! a whole-file backup under one run_id so `rs-hack revert <run_id>` undoes the batch.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use proc_macro2::{Delimiter, LineColumn, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::visit::Visit;

use crate::files::collect_rust_files_with_exclusions;
use crate::operations::{BackupNode, NodeLocation};
use crate::state::{
    FileModification, RunMetadata, RunStatus, generate_run_id, hash_file, load_run_metadata,
    save_backup_nodes, save_run_metadata,
};

/// Node type recorded on the whole-file backup an `apply` run saves; `restore_from_nodes`
/// writes `original_content` straight back for it.
pub const FILE_BACKUP_NODE_TYPE: &str = "file";

// ---------------------------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommentKind {
    /// `// ...` (and `//// ...`, which rustc does not treat as doc)
    Line,
    /// `/* ... */` (and `/*** ... */`, `/**/`)
    Block,
    /// `/// ...` or `/** ... */` — becomes an outer `#[doc]` attribute
    Doc,
    /// `//! ...` or `/*! ... */` — becomes an inner `#![doc]` attribute
    InnerDoc,
}

/// One comment token as the lexer sees it: byte range `[start, end)`, never including the
/// trailing newline of a line comment.
#[derive(Debug, Clone, Copy)]
struct RawComment {
    start: usize,
    end: usize,
    kind: CommentKind,
    is_block: bool,
}

const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn classify_line(text: &[u8]) -> CommentKind {
    if text.starts_with(b"//!") {
        CommentKind::InnerDoc
    } else if text.starts_with(b"///") && !text.starts_with(b"////") {
        CommentKind::Doc
    } else {
        CommentKind::Line
    }
}

fn classify_block(text: &[u8]) -> CommentKind {
    if text.starts_with(b"/*!") {
        CommentKind::InnerDoc
    } else if text.starts_with(b"/**") && !text.starts_with(b"/***") && text != b"/**/" {
        CommentKind::Doc
    } else {
        CommentKind::Block
    }
}

/// Skip a quoted string body starting just after the opening `"`; returns the offset just past
/// the closing quote.
const fn skip_string(b: &[u8], mut i: usize) -> usize {
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Skip a raw string: `i` points at the first `#` or `"` after the `r`. Returns the offset past
/// the terminator, or None if this is not a raw string (e.g. a raw identifier `r#foo`).
fn skip_raw_string(b: &[u8], mut i: usize) -> Option<usize> {
    let mut hashes = 0;
    while i < b.len() && b[i] == b'#' {
        hashes += 1;
        i += 1;
    }
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    i += 1;
    while i < b.len() {
        if b[i] == b'"'
            && b[i + 1..]
                .iter()
                .take(hashes)
                .filter(|c| **c == b'#')
                .count()
                == hashes
        {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(b.len())
}

/// Lex every comment in `src`, in source order.
fn lex_comments(src: &str) -> Vec<RawComment> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    // A shebang line is not a comment, but `#![attr]` on line 1 is not a shebang either.
    let mut i = if b.starts_with(b"#!") && !src[2..].trim_start().starts_with('[') {
        src.find('\n').unwrap_or(b.len())
    } else {
        0
    };
    while i < b.len() {
        let c = b[i];
        match c {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = src[i..].find('\n').map_or(b.len(), |n| i + n);
                // `\r\n` endings: keep the `\r` out of the comment text.
                let end = if end > i && b[end - 1] == b'\r' {
                    end - 1
                } else {
                    end
                };
                out.push(RawComment {
                    start: i,
                    end,
                    kind: classify_line(&b[i..end]),
                    is_block: false,
                });
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                let mut j = i;
                while j < b.len() {
                    if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                        depth += 1;
                        j += 2;
                    } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                        depth -= 1;
                        j += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        j += 1;
                    }
                }
                let end = j.min(b.len());
                out.push(RawComment {
                    start: i,
                    end,
                    kind: classify_block(&b[i..end]),
                    is_block: true,
                });
                i = end;
            }
            b'"' => i = skip_string(b, i + 1),
            b'\'' => {
                // Char literal vs lifetime/label.
                if b.get(i + 1) == Some(&b'\\') {
                    let mut j = i + 1;
                    while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
                        j += if b[j] == b'\\' { 2 } else { 1 };
                    }
                    i = j + 1;
                } else if let Some(ch) = src[i + 1..].chars().next() {
                    let after = i + 1 + ch.len_utf8();
                    if b.get(after) == Some(&b'\'') {
                        i = after + 1;
                    } else {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            _ if is_ident_char(c) => {
                let prev_is_ident = i > 0 && is_ident_char(b[i - 1]);
                if !prev_is_ident {
                    // String-literal prefixes: r, b, c, br, cr.
                    let (prefix_len, raw) = match (c, b.get(i + 1).copied()) {
                        (b'r', Some(b'"' | b'#')) => (1, true),
                        (b'b' | b'c', Some(b'r')) if matches!(b.get(i + 2), Some(b'"' | b'#')) => {
                            (2, true)
                        }
                        (b'b' | b'c', Some(b'"')) => (1, false),
                        (b'b', Some(b'\'')) => {
                            // byte char literal b'x' / b'\n'
                            let mut j = i + 2;
                            while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
                                j += if b[j] == b'\\' { 2 } else { 1 };
                            }
                            i = j + 1;
                            continue;
                        }
                        _ => (0, false),
                    };
                    if prefix_len > 0 {
                        if raw {
                            if let Some(end) = skip_raw_string(b, i + prefix_len) {
                                i = end;
                                continue;
                            }
                        } else {
                            i = skip_string(b, i + prefix_len + 1);
                            continue;
                        }
                    }
                }
                i += 1;
                while i < b.len() && is_ident_char(b[i]) {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Line/offset helpers
// ---------------------------------------------------------------------------------------------

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(src: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { starts }
    }

    /// 1-based line of a byte offset.
    fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }

    /// proc-macro2 fallback spans count columns in chars; convert to a byte offset.
    fn offset_of(&self, src: &str, lc: LineColumn) -> Option<usize> {
        let start = *self.starts.get(lc.line.checked_sub(1)?)?;
        let rest = &src[start..];
        let delta = rest
            .char_indices()
            .nth(lc.column)
            .map_or(rest.len(), |(i, _)| i);
        Some(start + delta)
    }
}

fn short_hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().as_str()[..16].to_string()
}

fn is_ws(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

// ---------------------------------------------------------------------------------------------
// Annotation detection
// ---------------------------------------------------------------------------------------------

/// Annotation markers the board indexes; a span holding one is refused by `apply` unless the
/// caller opts in, and flagged by `list` so callers can exclude it.
pub const ANNOTATION_MARKERS: [&str; 2] = ["@yah:", "@arch:"];

/// Paren/quote state of an annotation value that has not closed yet, carried across comment
/// lines so a `@yah:next("...` wrapped onto the next `//!` line keeps the continuation flagged.
#[derive(Default, Clone, Copy)]
struct AnnoState {
    depth: usize,
    in_str: bool,
}

impl AnnoState {
    const fn open(self) -> bool {
        self.depth > 0
    }

    /// Scan `text` starting inside an open value (or at a marker), return the state at the end.
    fn scan(mut self, text: &str) -> Self {
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if self.in_str {
                match ch {
                    '\\' => {
                        chars.next();
                    }
                    '"' => self.in_str = false,
                    _ => {}
                }
                continue;
            }
            match ch {
                '"' if self.depth > 0 => self.in_str = true,
                '(' => self.depth += 1,
                ')' if self.depth > 0 => {
                    self.depth -= 1;
                    if self.depth == 0 {
                        // A later marker on the same line starts fresh.
                        return Self::default().scan_from_marker(chars.as_str());
                    }
                }
                _ => {}
            }
        }
        self
    }

    /// Find the first marker in `text` and scan its value.
    fn scan_from_marker(self, text: &str) -> Self {
        let Some(pos) = ANNOTATION_MARKERS.iter().filter_map(|m| text.find(m)).min() else {
            return self;
        };
        let after = &text[pos..];
        let name_end = after.find(':').map_or(after.len(), |i| i + 1);
        let rest = &after[name_end..];
        let ident_len = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let tail = &rest[ident_len..];
        if tail.starts_with('(') {
            Self::default().scan(tail)
        } else {
            Self::default().scan_from_marker(tail)
        }
    }
}

fn has_marker(text: &str) -> bool {
    ANNOTATION_MARKERS.iter().any(|m| text.contains(m))
}

/// Per-comment-token annotation flag, with wrapped-value continuation.
fn annotation_flags(src: &str, raws: &[RawComment], lines: &LineIndex) -> Vec<bool> {
    let mut flags = vec![false; raws.len()];
    let mut state = AnnoState::default();
    let mut prev_line: Option<(usize, bool)> = None; // (line, is_line_comment)
    for (idx, rc) in raws.iter().enumerate() {
        let text = &src[rc.start..rc.end];
        let line = lines.line_of(rc.start);
        let continues =
            state.open() && !rc.is_block && matches!(prev_line, Some((l, true)) if l + 1 == line);
        if !continues {
            state = AnnoState::default();
        }
        if rc.is_block {
            flags[idx] = has_marker(text);
            state = AnnoState::default();
        } else if continues {
            flags[idx] = true;
            state = state.scan(text);
        } else if has_marker(text) {
            flags[idx] = true;
            state = AnnoState::default().scan_from_marker(text);
        }
        prev_line = Some((lines.line_of(rc.end), !rc.is_block));
    }
    flags
}

// ---------------------------------------------------------------------------------------------
// Item attachment (syn)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachedItem {
    pub node_type: String,
    pub name: String,
    pub visibility: String,
}

#[derive(Default)]
struct ItemMap {
    /// byte offset of an item's first non-doc token -> item
    heads: HashMap<usize, AttachedItem>,
    /// `[open_brace, close_brace]` byte offsets of every fn body
    bodies: Vec<(usize, usize)>,
}

fn vis_str(vis: &syn::Visibility) -> String {
    match vis {
        syn::Visibility::Inherited => "private".to_string(),
        other => other
            .to_token_stream()
            .to_string()
            .replace(" (", "(")
            .replace("( ", "(")
            .replace(" )", ")"),
    }
}

/// True if a `#`-led token run at `tts[i..]` is a doc attribute (`#[doc = ...]` or
/// `#![doc = ...]`); returns how many token trees it spans.
fn doc_attr_len(tts: &[TokenTree], i: usize) -> Option<usize> {
    let TokenTree::Punct(p) = &tts[i] else {
        return None;
    };
    if p.as_char() != '#' {
        return None;
    }
    let mut j = i + 1;
    if let Some(TokenTree::Punct(bang)) = tts.get(j)
        && bang.as_char() == '!'
    {
        j += 1;
    }
    let TokenTree::Group(g) = tts.get(j)? else {
        return None;
    };
    if g.delimiter() != Delimiter::Bracket {
        return None;
    }
    let mut inner = g.stream().into_iter();
    let is_doc = matches!(inner.next(), Some(TokenTree::Ident(id)) if id == "doc")
        && matches!(inner.next(), Some(TokenTree::Punct(eq)) if eq.as_char() == '=');
    is_doc.then_some(j + 1 - i)
}

struct ItemCollector<'a> {
    src: &'a str,
    lines: &'a LineIndex,
    map: ItemMap,
}

impl ItemCollector<'_> {
    fn head_offset<T: ToTokens>(&self, node: &T) -> Option<usize> {
        let tts: Vec<TokenTree> = node.to_token_stream().into_iter().collect();
        let mut i = 0;
        while i < tts.len() {
            if let Some(n) = doc_attr_len(&tts, i) {
                i += n;
                continue;
            }
            return self.lines.offset_of(self.src, tts[i].span().start());
        }
        None
    }

    fn record<T: ToTokens>(&mut self, node: &T, node_type: &str, name: String, vis: String) {
        if let Some(off) = self.head_offset(node) {
            self.map.heads.entry(off).or_insert_with(|| AttachedItem {
                node_type: node_type.to_string(),
                name,
                visibility: vis,
            });
        }
    }

    fn record_body(&mut self, block: &syn::Block) {
        let open = self
            .lines
            .offset_of(self.src, block.brace_token.span.open().start());
        let close = self
            .lines
            .offset_of(self.src, block.brace_token.span.close().start());
        if let (Some(o), Some(c)) = (open, close) {
            self.map.bodies.push((o, c));
        }
    }
}

impl<'ast> Visit<'ast> for ItemCollector<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        use syn::Item;
        let info: Option<(&str, String, String)> = match item {
            Item::Fn(i) => Some(("function", i.sig.ident.to_string(), vis_str(&i.vis))),
            Item::Struct(i) => Some(("struct", i.ident.to_string(), vis_str(&i.vis))),
            Item::Enum(i) => Some(("enum", i.ident.to_string(), vis_str(&i.vis))),
            Item::Union(i) => Some(("union", i.ident.to_string(), vis_str(&i.vis))),
            Item::Trait(i) => Some(("trait", i.ident.to_string(), vis_str(&i.vis))),
            Item::TraitAlias(i) => Some(("trait-alias", i.ident.to_string(), vis_str(&i.vis))),
            Item::Mod(i) => Some(("mod", i.ident.to_string(), vis_str(&i.vis))),
            Item::Const(i) => Some(("const", i.ident.to_string(), vis_str(&i.vis))),
            Item::Static(i) => Some(("static", i.ident.to_string(), vis_str(&i.vis))),
            Item::Type(i) => Some(("type-alias", i.ident.to_string(), vis_str(&i.vis))),
            Item::ExternCrate(i) => Some(("extern-crate", i.ident.to_string(), vis_str(&i.vis))),
            Item::Use(i) => Some((
                "use",
                i.tree.to_token_stream().to_string().replace(' ', ""),
                vis_str(&i.vis),
            )),
            Item::Impl(i) => Some(("impl", impl_name(i), "private".to_string())),
            Item::Macro(i) => Some((
                "macro",
                i.ident
                    .as_ref()
                    .map_or_else(|| path_str(&i.mac.path), ToString::to_string),
                "private".to_string(),
            )),
            Item::ForeignMod(_) => Some(("extern-block", String::new(), "private".to_string())),
            _ => None,
        };
        if let Some((kind, name, vis)) = info {
            self.record(item, kind, name, vis);
        }
        if let Item::Fn(f) = item {
            self.record_body(&f.block);
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_item_impl(&mut self, imp: &'ast syn::ItemImpl) {
        let owner = impl_name(imp);
        for it in &imp.items {
            match it {
                syn::ImplItem::Fn(f) => {
                    self.record(
                        f,
                        "impl-method",
                        format!("{owner}::{}", f.sig.ident),
                        vis_str(&f.vis),
                    );
                    self.record_body(&f.block);
                }
                syn::ImplItem::Const(c) => self.record(
                    c,
                    "impl-const",
                    format!("{owner}::{}", c.ident),
                    vis_str(&c.vis),
                ),
                syn::ImplItem::Type(t) => self.record(
                    t,
                    "impl-type",
                    format!("{owner}::{}", t.ident),
                    vis_str(&t.vis),
                ),
                _ => {}
            }
        }
        syn::visit::visit_item_impl(self, imp);
    }

    fn visit_item_trait(&mut self, tr: &'ast syn::ItemTrait) {
        for it in &tr.items {
            let name = |id: &syn::Ident| format!("{}::{id}", tr.ident);
            match it {
                syn::TraitItem::Fn(f) => {
                    self.record(f, "trait-method", name(&f.sig.ident), "private".into());
                    if let Some(b) = &f.default {
                        self.record_body(b);
                    }
                }
                syn::TraitItem::Const(c) => {
                    self.record(c, "trait-const", name(&c.ident), "private".into());
                }
                syn::TraitItem::Type(t) => {
                    self.record(t, "trait-type", name(&t.ident), "private".into());
                }
                _ => {}
            }
        }
        syn::visit::visit_item_trait(self, tr);
    }

    fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
        for (idx, f) in s.fields.iter().enumerate() {
            let fname = f
                .ident
                .as_ref()
                .map_or_else(|| idx.to_string(), ToString::to_string);
            self.record(f, "field", format!("{}::{fname}", s.ident), vis_str(&f.vis));
        }
        syn::visit::visit_item_struct(self, s);
    }

    fn visit_item_enum(&mut self, e: &'ast syn::ItemEnum) {
        for v in &e.variants {
            self.record(
                v,
                "variant",
                format!("{}::{}", e.ident, v.ident),
                "private".into(),
            );
            for (idx, f) in v.fields.iter().enumerate() {
                let fname = f
                    .ident
                    .as_ref()
                    .map_or_else(|| idx.to_string(), ToString::to_string);
                self.record(
                    f,
                    "field",
                    format!("{}::{}::{fname}", e.ident, v.ident),
                    "private".into(),
                );
            }
        }
        syn::visit::visit_item_enum(self, e);
    }
}

fn path_str(p: &syn::Path) -> String {
    p.to_token_stream().to_string().replace(' ', "")
}

fn impl_name(imp: &syn::ItemImpl) -> String {
    let ty = imp.self_ty.to_token_stream().to_string().replace(' ', "");
    match &imp.trait_ {
        Some((bang, path, _)) => format!(
            "{}{} for {ty}",
            if bang.is_some() { "!" } else { "" },
            path_str(path)
        ),
        None => ty,
    }
}

// ---------------------------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentSpan {
    pub file: PathBuf,
    /// Byte range `[start, end)` in the file. Pass it back verbatim to `apply`.
    pub span: [usize; 2],
    /// 1-based inclusive line range.
    pub lines: [usize; 2],
    pub kind: CommentKind,
    /// Raw source text of the span, comment markers included.
    pub text: String,
    /// Guard for `apply`: blake3 of `text`, first 16 hex chars.
    pub hash: String,
    pub attached_item: Option<AttachedItem>,
    pub in_body: bool,
    /// True if the span holds an `@yah:` / `@arch:` annotation (or its wrapped continuation).
    pub annotation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListReport {
    pub files_scanned: usize,
    pub span_count: usize,
    pub annotation_span_count: usize,
    /// Comment lines inside annotation spans; comparable to `grep -c '@yah:\|@arch:'` plus
    /// wrapped continuation lines.
    pub annotation_line_count: usize,
    /// Files syn could not parse: their spans are still listed, without attachment/in_body.
    pub parse_errors: Vec<(PathBuf, String)>,
    pub spans: Vec<CommentSpan>,
}

pub struct ListArgs {
    pub paths: Vec<PathBuf>,
    pub exclude: Vec<String>,
    /// Drop annotation spans from the output (still counted).
    pub exclude_annotations: bool,
}

/// Spans of one file. Consecutive own-line `//`-style comments of the same kind on adjacent
/// lines merge into one span; a run splits where the annotation flag flips, so the prose
/// around an annotation block stays editable while the annotation itself stays guarded.
fn file_spans(path: &Path, src: &str) -> (Vec<CommentSpan>, Option<String>) {
    let lines = LineIndex::new(src);
    let raws = lex_comments(src);
    let flags = annotation_flags(src, &raws, &lines);

    let (map, parse_err) = match syn::parse_file(src) {
        Ok(file) => {
            let mut c = ItemCollector {
                src,
                lines: &lines,
                map: ItemMap::default(),
            };
            c.visit_file(&file);
            (c.map, None)
        }
        Err(e) => (ItemMap::default(), Some(e.to_string())),
    };

    let own_line = |rc: &RawComment| {
        let ls = lines.starts[lines.line_of(rc.start) - 1];
        is_ws(&src[ls..rc.start])
    };

    let mut groups: Vec<(usize, usize)> = Vec::new(); // inclusive raw index ranges
    for (idx, rc) in raws.iter().enumerate() {
        if let Some((gs, ge)) = groups.last_mut() {
            let prev = &raws[*ge];
            let joinable = !rc.is_block
                && !prev.is_block
                && prev.kind == rc.kind
                && flags[*ge] == flags[idx]
                && own_line(rc)
                && own_line(&raws[*gs])
                && lines.line_of(rc.start) == lines.line_of(prev.end) + 1
                && is_ws(&src[prev.end..rc.start]);
            if joinable {
                *ge = idx;
                continue;
            }
        }
        groups.push((idx, idx));
    }

    let spans = groups
        .into_iter()
        .map(|(gs, ge)| {
            let start = raws[gs].start;
            let end = raws[ge].end;
            let kind = raws[gs].kind;
            let text = src[start..end].to_string();
            let attached_item = if kind == CommentKind::InnerDoc {
                None
            } else {
                next_code_offset(src, &raws, ge).and_then(|p| map.heads.get(&p).cloned())
            };
            CommentSpan {
                file: path.to_path_buf(),
                span: [start, end],
                lines: [lines.line_of(start), lines.line_of(end.max(start + 1) - 1)],
                kind,
                hash: short_hash(&text),
                text,
                attached_item,
                in_body: map.bodies.iter().any(|(o, c)| *o < start && start < *c),
                annotation: flags[gs..=ge].iter().any(|f| *f),
            }
        })
        .collect();
    (spans, parse_err)
}

/// Offset of the first code byte after raw comment `idx`, skipping whitespace and comments.
fn next_code_offset(src: &str, raws: &[RawComment], idx: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut pos = raws[idx].end;
    let mut next = idx + 1;
    loop {
        while pos < b.len() && b[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if next < raws.len() && raws[next].start == pos {
            pos = raws[next].end;
            next += 1;
            continue;
        }
        return (pos < b.len()).then_some(pos);
    }
}

pub fn list(args: &ListArgs) -> Result<ListReport> {
    let files = collect_rust_files_with_exclusions(&args.paths, &args.exclude)?;
    let mut report = ListReport {
        files_scanned: files.len(),
        ..Default::default()
    };
    for path in files {
        let src = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let (spans, err) = file_spans(&path, &src);
        if let Some(e) = err {
            report.parse_errors.push((path.clone(), e));
        }
        for s in spans {
            report.span_count += 1;
            if s.annotation {
                report.annotation_span_count += 1;
                report.annotation_line_count += s.lines[1] - s.lines[0] + 1;
                if args.exclude_annotations {
                    continue;
                }
            }
            report.spans.push(s);
        }
    }
    Ok(report)
}

pub fn render_list(report: &ListReport) {
    for s in &report.spans {
        let attached = s
            .attached_item
            .as_ref()
            .map(|a| format!(" -> {} {}", a.node_type, a.name))
            .unwrap_or_default();
        println!(
            "{}:{}-{} [{}..{}] {:?}{}{}{} {}",
            s.file.display(),
            s.lines[0],
            s.lines[1],
            s.span[0],
            s.span[1],
            s.kind,
            if s.in_body { " in-body" } else { "" },
            if s.annotation { " ANNOTATION" } else { "" },
            attached,
            s.hash
        );
    }
    println!(
        "{} span(s) in {} file(s); {} annotation span(s) covering {} line(s)",
        report.span_count,
        report.files_scanned,
        report.annotation_span_count,
        report.annotation_line_count
    );
}

// ---------------------------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------------------------

/// One comparable unit of code: a label naming it and its token string with every comment,
/// every whitespace difference and every `#[doc = ...]` attribute removed.
#[derive(Debug, Clone)]
struct CodeUnit {
    label: String,
    line: usize,
    tokens: String,
    children: Vec<Self>,
}

fn push_tokens(ts: TokenStream, out: &mut String) {
    let tts: Vec<TokenTree> = ts.into_iter().collect();
    let mut i = 0;
    while i < tts.len() {
        if let Some(n) = doc_attr_len(&tts, i) {
            i += n;
            continue;
        }
        match &tts[i] {
            TokenTree::Group(g) => {
                let (open, close) = match g.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("<<", ">>"),
                };
                out.push_str(open);
                out.push(' ');
                push_tokens(g.stream(), out);
                out.push_str(close);
                out.push(' ');
            }
            TokenTree::Punct(p) => {
                out.push(p.as_char());
                if p.spacing() == Spacing::Alone {
                    out.push(' ');
                }
            }
            TokenTree::Ident(id) => {
                out.push_str(&id.to_string());
                out.push(' ');
            }
            TokenTree::Literal(l) => {
                out.push_str(&l.to_string());
                out.push(' ');
            }
        }
        i += 1;
    }
}

fn norm<T: ToTokens>(node: &T) -> String {
    let mut s = String::new();
    push_tokens(node.to_token_stream(), &mut s);
    s
}

fn item_label(item: &syn::Item) -> String {
    use syn::Item;
    match item {
        Item::Fn(i) => format!("fn {}", i.sig.ident),
        Item::Struct(i) => format!("struct {}", i.ident),
        Item::Enum(i) => format!("enum {}", i.ident),
        Item::Union(i) => format!("union {}", i.ident),
        Item::Trait(i) => format!("trait {}", i.ident),
        Item::TraitAlias(i) => format!("trait {}", i.ident),
        Item::Mod(i) => format!("mod {}", i.ident),
        Item::Const(i) => format!("const {}", i.ident),
        Item::Static(i) => format!("static {}", i.ident),
        Item::Type(i) => format!("type {}", i.ident),
        Item::ExternCrate(i) => format!("extern crate {}", i.ident),
        Item::Use(i) => format!(
            "use {}",
            i.tree.to_token_stream().to_string().replace(' ', "")
        ),
        Item::Impl(i) => format!("impl {}", impl_name(i)),
        Item::Macro(i) => i.ident.as_ref().map_or_else(
            || format!("{}!", path_str(&i.mac.path)),
            |id| format!("macro_rules! {id}"),
        ),
        Item::ForeignMod(_) => "extern block".to_string(),
        _ => "item".to_string(),
    }
}

fn unit_line<T: syn::spanned::Spanned>(node: &T) -> usize {
    node.span().start().line
}

fn item_units(items: &[syn::Item], prefix: &str) -> Vec<CodeUnit> {
    items
        .iter()
        .map(|item| {
            let label = format!("{prefix}{}", item_label(item));
            let children = match item {
                syn::Item::Impl(i) => i
                    .items
                    .iter()
                    .map(|it| CodeUnit {
                        label: format!("{label}::{}", impl_item_label(it)),
                        line: unit_line(it),
                        tokens: norm(it),
                        children: Vec::new(),
                    })
                    .collect(),
                syn::Item::Trait(t) => t
                    .items
                    .iter()
                    .map(|it| CodeUnit {
                        label: format!("{label}::{}", trait_item_label(it)),
                        line: unit_line(it),
                        tokens: norm(it),
                        children: Vec::new(),
                    })
                    .collect(),
                syn::Item::Mod(m) => m
                    .content
                    .as_ref()
                    .map(|(_, items)| item_units(items, &format!("{label}::")))
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            CodeUnit {
                label,
                line: unit_line(item),
                tokens: norm(item),
                children,
            }
        })
        .collect()
}

fn impl_item_label(it: &syn::ImplItem) -> String {
    match it {
        syn::ImplItem::Fn(f) => format!("fn {}", f.sig.ident),
        syn::ImplItem::Const(c) => format!("const {}", c.ident),
        syn::ImplItem::Type(t) => format!("type {}", t.ident),
        syn::ImplItem::Macro(m) => format!("{}!", path_str(&m.mac.path)),
        _ => "item".to_string(),
    }
}

fn trait_item_label(it: &syn::TraitItem) -> String {
    match it {
        syn::TraitItem::Fn(f) => format!("fn {}", f.sig.ident),
        syn::TraitItem::Const(c) => format!("const {}", c.ident),
        syn::TraitItem::Type(t) => format!("type {}", t.ident),
        syn::TraitItem::Macro(m) => format!("{}!", path_str(&m.mac.path)),
        _ => "item".to_string(),
    }
}

fn file_units(src: &str) -> Result<Vec<CodeUnit>> {
    let file = syn::parse_file(src).map_err(|e| anyhow!("parse error: {e}"))?;
    let mut crate_attrs = String::new();
    for a in &file.attrs {
        push_tokens(a.to_token_stream(), &mut crate_attrs);
    }
    let mut units = vec![CodeUnit {
        label: "crate attributes".to_string(),
        line: 1,
        tokens: crate_attrs,
        children: Vec::new(),
    }];
    units.extend(item_units(&file.items, ""));
    Ok(units)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Difference {
    /// Item as it was before, or None if the after side has an extra item here.
    pub before_item: Option<String>,
    pub before_line: Option<usize>,
    pub after_item: Option<String>,
    pub after_line: Option<usize>,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let side = |item: &Option<String>, line: &Option<usize>| match (item, line) {
            (Some(i), Some(l)) => format!("`{i}` (line {l})"),
            (Some(i), None) => format!("`{i}`"),
            _ => "<nothing>".to_string(),
        };
        if self.before_item == self.after_item {
            write!(
                f,
                "{} changed: before line {}, after line {}",
                side(&self.before_item, &None),
                self.before_line.unwrap_or(0),
                self.after_line.unwrap_or(0)
            )
        } else {
            write!(
                f,
                "before {} vs after {}",
                side(&self.before_item, &self.before_line),
                side(&self.after_item, &self.after_line)
            )
        }
    }
}

fn first_difference(before: &[CodeUnit], after: &[CodeUnit]) -> Option<Difference> {
    let n = before.len().max(after.len());
    for i in 0..n {
        match (before.get(i), after.get(i)) {
            (Some(b), Some(a)) if b.label == a.label && b.tokens == a.tokens => {}
            (Some(b), Some(a)) => {
                // Same container on both sides: name the member that changed, if any.
                if b.label == a.label
                    && let Some(d) = first_difference(&b.children, &a.children)
                {
                    return Some(d);
                }
                return Some(Difference {
                    before_item: Some(b.label.clone()),
                    before_line: Some(b.line),
                    after_item: Some(a.label.clone()),
                    after_line: Some(a.line),
                });
            }
            (b, a) => {
                return Some(Difference {
                    before_item: b.map(|u| u.label.clone()),
                    before_line: b.map(|u| u.line),
                    after_item: a.map(|u| u.label.clone()),
                    after_line: a.map(|u| u.line),
                });
            }
        }
    }
    None
}

/// Compare two sources as code. `Ok(None)` means equal modulo comments, whitespace and doc
/// attributes; `Ok(Some(d))` names the first item that differs.
pub fn compare_sources(before: &str, after: &str) -> Result<Option<Difference>> {
    let b = file_units(before).context("before")?;
    let a = file_units(after).context("after")?;
    Ok(first_difference(&b, &a))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVerdict {
    pub file: PathBuf,
    pub equal: bool,
    pub difference: Option<Difference>,
    /// Set when either side failed to parse; `equal` is false.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub equal: bool,
    pub files: Vec<FileVerdict>,
}

impl VerifyReport {
    fn from_files(files: Vec<FileVerdict>) -> Self {
        Self {
            equal: files.iter().all(|f| f.equal),
            files,
        }
    }
}

fn verdict(file: PathBuf, before: &str, after: &str) -> FileVerdict {
    match compare_sources(before, after) {
        Ok(d) => FileVerdict {
            file,
            equal: d.is_none(),
            difference: d,
            error: None,
        },
        Err(e) => FileVerdict {
            file,
            equal: false,
            difference: None,
            error: Some(format!("{e:#}")),
        },
    }
}

pub enum VerifyArgs {
    /// `before` is a file path, or a git revision whose copy of `after` is compared.
    Files { before: String, after: PathBuf },
    /// Compare every file a `comments apply` run modified against its backup.
    RunId { run_id: String, state_dir: PathBuf },
}

fn read_before(before: &str, after: &Path) -> Result<String> {
    let p = Path::new(before);
    if p.is_file() {
        return std::fs::read_to_string(p).with_context(|| format!("Failed to read {before}"));
    }
    // Treat as a git revision: `git show <rev>:./<file>` from the after file's directory.
    let dir = after
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = after
        .file_name()
        .ok_or_else(|| anyhow!("--after has no file name"))?
        .to_string_lossy();
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("{before}:./{name}"))
        .output()
        .context("Failed to run git show")?;
    if !out.status.success() {
        bail!(
            "--before `{before}` is neither a file nor a git revision containing {}: {}",
            after.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8(out.stdout)?)
}

pub fn verify(args: &VerifyArgs) -> Result<VerifyReport> {
    match args {
        VerifyArgs::Files { before, after } => {
            let before_src = read_before(before, after)?;
            let after_src = std::fs::read_to_string(after)
                .with_context(|| format!("Failed to read {}", after.display()))?;
            Ok(VerifyReport::from_files(vec![verdict(
                after.clone(),
                &before_src,
                &after_src,
            )]))
        }
        VerifyArgs::RunId { run_id, state_dir } => {
            let run = load_run_metadata(run_id, state_dir)?;
            let mut files = Vec::new();
            for fm in &run.files_modified {
                let Some(backup) = fm
                    .backup_nodes
                    .iter()
                    .find(|n| n.node_type == FILE_BACKUP_NODE_TYPE)
                else {
                    bail!(
                        "run {run_id} has no whole-file backup for {} — only `comments apply` runs can be verified by run id",
                        fm.path.display()
                    );
                };
                let after_src = std::fs::read_to_string(&fm.path)
                    .with_context(|| format!("Failed to read {}", fm.path.display()))?;
                files.push(verdict(
                    fm.path.clone(),
                    &backup.original_content,
                    &after_src,
                ));
            }
            Ok(VerifyReport::from_files(files))
        }
    }
}

pub fn render_verify(report: &VerifyReport) {
    for f in &report.files {
        if f.equal {
            println!(
                "✓ {}: code unchanged (comments/whitespace/doc ignored)",
                f.file.display()
            );
        } else if let Some(e) = &f.error {
            println!("✗ {}: {e}", f.file.display());
        } else if let Some(d) = &f.difference {
            println!("✗ {}: first differing item: {d}", f.file.display());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// apply
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum CommentAction {
    /// Remove the span; an own-line span takes its whole lines with it.
    Delete,
    /// Replace the span's text. `text` must itself be only comments and whitespace.
    Replace { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentOp {
    pub file: PathBuf,
    pub span: [usize; 2],
    /// The `hash` `comments list` reported for this span.
    pub hash: String,
    #[serde(flatten)]
    pub action: CommentAction,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BatchShape {
    Ops(Vec<CommentOp>),
    Wrapped { ops: Vec<CommentOp> },
}

pub fn parse_batch(json: &str) -> Result<Vec<CommentOp>> {
    let shape: BatchShape = serde_json::from_str(json)
        .context("batch must be a JSON array of ops or {\"ops\": [...]}")?;
    Ok(match shape {
        BatchShape::Ops(v) | BatchShape::Wrapped { ops: v } => v,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpOutcome {
    /// Index of the op in the batch.
    pub index: usize,
    pub file: PathBuf,
    pub span: [usize; 2],
    /// Why the op was refused; None for an accepted op.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApplyReport {
    pub dry_run: bool,
    /// Set only when files were written.
    pub run_id: Option<String>,
    pub accepted: Vec<OpOutcome>,
    pub refused: Vec<OpOutcome>,
    pub files_changed: Vec<PathBuf>,
    /// Unified diff of every changed file (dry-run and apply alike).
    pub diff: String,
}

pub struct ApplyArgs {
    pub ops: Vec<CommentOp>,
    pub apply: bool,
    pub allow_annotations: bool,
    pub state_dir: PathBuf,
    /// Recorded on the run for `rs-hack history`.
    pub command_line: String,
}

/// Why `[s, e)` is not an editable comment span of `src`, if it is not.
fn check_span(
    src: &str,
    raws: &[RawComment],
    flags: &[bool],
    [s, e]: [usize; 2],
    allow_annotations: bool,
) -> Option<String> {
    if s >= e || e > src.len() || !src.is_char_boundary(s) || !src.is_char_boundary(e) {
        return Some(format!("span [{s}, {e}) is out of bounds for this file"));
    }
    let Some(first) = raws.iter().position(|r| r.start == s) else {
        return Some("span does not start at a comment".to_string());
    };
    let mut pos = s;
    for (idx, r) in raws.iter().enumerate().skip(first) {
        if r.start >= e {
            break;
        }
        if !is_ws(&src[pos..r.start]) {
            return Some(format!("span covers code at byte {pos}"));
        }
        if r.end > e {
            return Some("span ends inside a comment".to_string());
        }
        if flags[idx] && !allow_annotations {
            return Some(
                "span holds an @yah:/@arch: annotation (pass --allow-annotations to edit it)"
                    .to_string(),
            );
        }
        pos = r.end;
    }
    if pos != e {
        return Some("span does not end at a comment".to_string());
    }
    None
}

/// Resolve a delete to the byte range actually removed: whole lines for an own-line span, the
/// comment plus its leading horizontal whitespace for a trailing one.
fn delete_range(src: &str, [s, e]: [usize; 2]) -> (usize, usize) {
    let ls = src[..s].rfind('\n').map_or(0, |i| i + 1);
    let le = src[e..].find('\n').map_or(src.len(), |i| e + i);
    if is_ws(&src[ls..s]) && is_ws(&src[e..le]) {
        (ls, (le + 1).min(src.len()))
    } else {
        let trimmed = src[..s].trim_end_matches([' ', '\t']).len();
        (trimmed, e)
    }
}

pub fn apply(args: &ApplyArgs) -> Result<ApplyReport> {
    let mut report = ApplyReport {
        dry_run: !args.apply,
        ..Default::default()
    };

    let mut by_file: BTreeMap<PathBuf, Vec<(usize, &CommentOp)>> = BTreeMap::new();
    for (i, op) in args.ops.iter().enumerate() {
        by_file.entry(op.file.clone()).or_default().push((i, op));
    }

    let run_id = generate_run_id();
    let mut modifications = Vec::new();

    for (file, ops) in by_file {
        let refuse = |report: &mut ApplyReport, i: usize, op: &CommentOp, why: String| {
            report.refused.push(OpOutcome {
                index: i,
                file: op.file.clone(),
                span: op.span,
                reason: Some(why),
            });
        };
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                for (i, op) in ops {
                    refuse(&mut report, i, op, format!("cannot read file: {e}"));
                }
                continue;
            }
        };
        let lines = LineIndex::new(&src);
        let raws = lex_comments(&src);
        let flags = annotation_flags(&src, &raws, &lines);

        // (index, op, removed range, replacement)
        let mut edits: Vec<(usize, &CommentOp, (usize, usize), String)> = Vec::new();
        for (i, op) in ops {
            if let Some(why) = check_span(&src, &raws, &flags, op.span, args.allow_annotations) {
                refuse(&mut report, i, op, why);
                continue;
            }
            let current = short_hash(&src[op.span[0]..op.span[1]]);
            if current != op.hash {
                refuse(
                    &mut report,
                    i,
                    op,
                    format!(
                        "stale span: hash is {current}, batch expected {} — re-run `comments list`",
                        op.hash
                    ),
                );
                continue;
            }
            let (range, text) = match &op.action {
                CommentAction::Delete => (delete_range(&src, op.span), String::new()),
                CommentAction::Replace { text } => {
                    let rep = lex_comments(text);
                    let mut pos = 0;
                    let mut only_comments = true;
                    for r in &rep {
                        only_comments &= is_ws(&text[pos..r.start]);
                        pos = r.end;
                    }
                    only_comments &= is_ws(&text[pos..]);
                    if !only_comments {
                        refuse(
                            &mut report,
                            i,
                            op,
                            "replacement text must contain only comments and whitespace"
                                .to_string(),
                        );
                        continue;
                    }
                    if !args.allow_annotations && has_marker(text) {
                        refuse(
                            &mut report,
                            i,
                            op,
                            "replacement text introduces an @yah:/@arch: annotation (pass --allow-annotations)"
                                .to_string(),
                        );
                        continue;
                    }
                    if text.is_empty() {
                        (delete_range(&src, op.span), String::new())
                    } else {
                        ((op.span[0], op.span[1]), text.clone())
                    }
                }
            };
            if let Some((j, other, ..)) = edits
                .iter()
                .find(|(_, _, r, _)| range.0 < r.1 && r.0 < range.1)
            {
                refuse(
                    &mut report,
                    i,
                    op,
                    format!(
                        "overlaps op #{j} span [{}, {})",
                        other.span[0], other.span[1]
                    ),
                );
                continue;
            }
            edits.push((i, op, range, text));
        }
        if edits.is_empty() {
            continue;
        }

        edits.sort_by_key(|(_, _, r, _)| std::cmp::Reverse(r.0));
        let mut new_src = src.clone();
        for (_, _, (s, e), text) in &edits {
            new_src.replace_range(*s..*e, text);
        }

        // The gate: a comment edit must leave the code identical.
        match compare_sources(&src, &new_src) {
            Ok(None) => {}
            other => {
                let why = match other {
                    Ok(Some(d)) => format!("batch would change code in this file: {d}"),
                    Err(e) => format!("batch would leave this file unparseable: {e:#}"),
                    Ok(None) => unreachable!(),
                };
                for (i, op, ..) in edits {
                    refuse(&mut report, i, op, why.clone());
                }
                continue;
            }
        }

        for (i, op, ..) in &edits {
            report.accepted.push(OpOutcome {
                index: *i,
                file: op.file.clone(),
                span: op.span,
                reason: None,
            });
        }
        report
            .diff
            .push_str(&crate::diff::generate_unified_diff(&file, &src, &new_src, 3).0);
        report.files_changed.push(file.clone());

        if args.apply {
            let abs = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
            let backup = BackupNode {
                node_type: FILE_BACKUP_NODE_TYPE.to_string(),
                identifier: abs.display().to_string(),
                original_content: src.clone(),
                location: NodeLocation {
                    line: 1,
                    column: 0,
                    end_line: lines.starts.len(),
                    end_column: 0,
                },
            };
            let hash_before = hash_file(&abs)?;
            save_backup_nodes(
                &abs,
                std::slice::from_ref(&backup),
                &run_id,
                &args.state_dir,
            )?;
            std::fs::write(&abs, &new_src)
                .with_context(|| format!("Failed to write {}", abs.display()))?;
            modifications.push(FileModification {
                path: abs.clone(),
                hash_before,
                hash_after: hash_file(&abs)?,
                backup_nodes: vec![backup],
            });
        }
    }

    report.accepted.sort_by_key(|o| o.index);
    report.refused.sort_by_key(|o| o.index);

    if !modifications.is_empty() {
        save_run_metadata(
            &RunMetadata {
                run_id: run_id.clone(),
                timestamp: chrono::Utc::now(),
                command: args.command_line.clone(),
                operation: "comments-apply".to_string(),
                files_modified: modifications,
                status: RunStatus::Applied,
                can_revert: true,
            },
            &args.state_dir,
        )?;
        report.run_id = Some(run_id);
    }
    Ok(report)
}

pub fn render_apply(report: &ApplyReport) {
    if !report.diff.is_empty() {
        println!("{}", report.diff);
    }
    for r in &report.refused {
        println!(
            "✗ op #{} {} [{}, {}): {}",
            r.index,
            r.file.display(),
            r.span[0],
            r.span[1],
            r.reason.as_deref().unwrap_or("")
        );
    }
    println!(
        "{} op(s) accepted, {} refused, {} file(s) {}",
        report.accepted.len(),
        report.refused.len(),
        report.files_changed.len(),
        if report.dry_run {
            "would change (dry run; pass --apply to write)"
        } else {
            "changed"
        }
    );
    if let Some(id) = &report.run_id {
        println!("run_id: {id} (undo with `rs-hack revert {id}`)");
    }
}
