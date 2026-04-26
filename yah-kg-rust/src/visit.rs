//! @arch:layer(kg)
//! @arch:role(traverse)
//!
//! Syn AST walker. Pure within-file structural extraction.
//!
//! Walks the parsed `syn::File` once, maintaining a module-path stack
//! and a parent-id stack so every emitted node gets a `Contains` edge
//! from its enclosing item. Also emits Rust-specific edges:
//! `ImplFor`, `ImplOfTrait`, `Defines`, plus derive/attribute info
//! recorded as properties on the type (cross-file macro resolution is
//! deferred to the daemon's Pass 3).

use proc_macro2::LineColumn;
use syn::spanned::Spanned;
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::indexer::IndexSink;
use yah_kg::kind::{CommonKind, Lang, MacroFlavor, NodeKind, RustKind};

const LANG: Lang = Lang::Rust;

pub struct Walker<'a> {
    file: String,
    file_id: NodeId,
    mod_path: Vec<String>,
    parents: Vec<NodeId>,
    sink: &'a mut dyn IndexSink,
}

impl<'a> Walker<'a> {
    pub fn new(file: &str, sink: &'a mut dyn IndexSink) -> Self {
        let file = file.replace('\\', "/");
        let file_id = NodeId::compute(LANG, &file, &file);
        Self {
            file,
            file_id,
            mod_path: Vec::new(),
            parents: Vec::new(),
            sink,
        }
    }

    pub fn run(&mut self, file: &syn::File) {
        // Emit the File node so the daemon doesn't need to know whether
        // the indexer or the walker is responsible. `upsert` dedupes.
        let label = self
            .file
            .rsplit_once('/')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| self.file.clone());
        let span = whole_file_span(file);
        self.sink.push_node(NodeRef {
            id: self.file_id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::File),
            label,
            qualified: self.file.clone(),
            file: self.file.clone(),
            span,
            synthetic: false,
        });
        if let Some(d) = doc_of_attrs(&file.attrs) {
            self.sink.push_doc(self.file_id, &d);
        }
        self.parents.push(self.file_id);
        for item in &file.items {
            self.walk_item(item);
        }
        self.parents.pop();
    }

    fn current_parent(&self) -> NodeId {
        *self.parents.last().expect("parent stack invariant")
    }

    fn qualify(&self, name: &str) -> String {
        if self.mod_path.is_empty() {
            format!("{}::{}", self.file, name)
        } else {
            format!("{}::{}::{}", self.file, self.mod_path.join("::"), name)
        }
    }

    fn make_id(&self, qualified: &str) -> NodeId {
        NodeId::compute(LANG, qualified, &self.file)
    }

    fn emit_node(&mut self, node: NodeRef) {
        self.sink.push_node(node);
    }

    fn emit_contains(&mut self, parent: NodeId, child: NodeId) {
        self.sink.push_edge(EdgeOut {
            id: EdgeId::compute(parent, child, &EdgeKind::Contains),
            from: parent,
            to: child,
            kind: EdgeKind::Contains,
            annotations: vec![],
        });
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

    fn walk_item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Mod(m) => self.walk_mod(m),
            syn::Item::Struct(s) => self.walk_struct(s),
            syn::Item::Enum(e) => self.walk_enum(e),
            syn::Item::Union(u) => self.walk_union(u),
            syn::Item::Type(t) => self.walk_type_alias(t),
            syn::Item::Fn(f) => self.walk_fn(f),
            syn::Item::Trait(t) => self.walk_trait(t),
            syn::Item::Impl(i) => self.walk_impl(i),
            syn::Item::Const(c) => self.walk_const(&c.ident, &c.attrs, c.span()),
            syn::Item::Static(s) => self.walk_const(&s.ident, &s.attrs, s.span()),
            syn::Item::Macro(m) => self.walk_macro_decl(m),
            // Use, ExternCrate, ForeignMod, TraitAlias, Verbatim — Pass 3 work.
            _ => {}
        }
    }

    fn walk_mod(&mut self, m: &syn::ItemMod) {
        let name = m.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Module),
            label: name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(m),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        if let Some(d) = doc_of_attrs(&m.attrs) {
            self.sink.push_doc(id, &d);
        }

        if let Some((_, items)) = &m.content {
            self.mod_path.push(name);
            self.parents.push(id);
            for item in items {
                self.walk_item(item);
            }
            self.parents.pop();
            self.mod_path.pop();
        }
    }

    fn walk_struct(&mut self, s: &syn::ItemStruct) {
        let name = s.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Type),
            label: name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(s),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.record_type_kind(id, "struct");
        self.record_attrs(id, &s.attrs);
        if let Some(d) = doc_of_attrs(&s.attrs) {
            self.sink.push_doc(id, &d);
        }

        // Fields
        let qualified_struct = self.qualify(&name);
        for field in &s.fields {
            self.walk_field(&qualified_struct, id, field);
        }
    }

    fn walk_union(&mut self, u: &syn::ItemUnion) {
        let name = u.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Type),
            label: name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(u),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.record_type_kind(id, "union");
        self.record_attrs(id, &u.attrs);
        if let Some(d) = doc_of_attrs(&u.attrs) {
            self.sink.push_doc(id, &d);
        }
        let qualified_union = self.qualify(&name);
        for field in &u.fields.named {
            self.walk_field(&qualified_union, id, field);
        }
    }

    fn walk_field(&mut self, parent_qualified: &str, parent_id: NodeId, field: &syn::Field) {
        let Some(ident) = field.ident.as_ref() else {
            // Tuple-struct fields: skip; they don't have stable ident-based ids.
            return;
        };
        let name = ident.to_string();
        let qualified = format!("{}::{}", parent_qualified, name);
        let id = self.make_id(&qualified);
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Field),
            label: name,
            qualified,
            file: self.file.clone(),
            span: span_of(field),
            synthetic: false,
        });
        self.emit_contains(parent_id, id);
        if let Some(d) = doc_of_attrs(&field.attrs) {
            self.sink.push_doc(id, &d);
        }
    }

    fn walk_enum(&mut self, e: &syn::ItemEnum) {
        let name = e.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Type),
            label: name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(e),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.record_type_kind(id, "enum");
        self.record_attrs(id, &e.attrs);
        if let Some(d) = doc_of_attrs(&e.attrs) {
            self.sink.push_doc(id, &d);
        }

        let qualified_enum = self.qualify(&name);
        for variant in &e.variants {
            let v_name = variant.ident.to_string();
            let v_qualified = format!("{}::{}", qualified_enum, v_name);
            let v_id = self.make_id(&v_qualified);
            self.emit_node(NodeRef {
                id: v_id,
                lang: LANG,
                kind: NodeKind::Common(CommonKind::Variant),
                label: v_name,
                qualified: v_qualified,
                file: self.file.clone(),
                span: span_of(variant),
                synthetic: false,
            });
            self.emit_contains(id, v_id);
            if let Some(d) = doc_of_attrs(&variant.attrs) {
                self.sink.push_doc(v_id, &d);
            }
        }
    }

    fn walk_type_alias(&mut self, t: &syn::ItemType) {
        let name = t.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Type),
            label: name,
            qualified,
            file: self.file.clone(),
            span: span_of(t),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.record_type_kind(id, "alias");
        if let Some(d) = doc_of_attrs(&t.attrs) {
            self.sink.push_doc(id, &d);
        }
    }

    fn walk_fn(&mut self, f: &syn::ItemFn) {
        let name = f.sig.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Function),
            label: name,
            qualified,
            file: self.file.clone(),
            span: span_of(f),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        if f.sig.asyncness.is_some() {
            self.sink.push_property(id, "async", "true");
        }
        if f.sig.unsafety.is_some() {
            self.sink.push_property(id, "unsafe", "true");
        }
        if let Some(d) = doc_of_attrs(&f.attrs) {
            self.sink.push_doc(id, &d);
        }
    }

    fn walk_const(&mut self, ident: &syn::Ident, attrs: &[syn::Attribute], sp: proc_macro2::Span) {
        let name = ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Constant),
            label: name,
            qualified,
            file: self.file.clone(),
            span: span_from(sp),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        if let Some(d) = doc_of_attrs(attrs) {
            self.sink.push_doc(id, &d);
        }
    }

    fn walk_trait(&mut self, t: &syn::ItemTrait) {
        let name = t.ident.to_string();
        let qualified = self.qualify(&name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Rust(RustKind::Trait),
            label: name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(t),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        if t.auto_token.is_some() {
            self.sink.push_property(id, "auto", "true");
        }
        if t.unsafety.is_some() {
            self.sink.push_property(id, "unsafe", "true");
        }
        if let Some(d) = doc_of_attrs(&t.attrs) {
            self.sink.push_doc(id, &d);
        }

        let qualified_trait = self.qualify(&name);
        for item in &t.items {
            self.walk_trait_item(&qualified_trait, id, item);
        }
    }

    fn walk_trait_item(
        &mut self,
        parent_qualified: &str,
        parent_id: NodeId,
        item: &syn::TraitItem,
    ) {
        match item {
            syn::TraitItem::Fn(f) => {
                let name = f.sig.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Common(CommonKind::Method),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(f),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
                if f.default.is_none() {
                    self.sink.push_property(id, "required", "true");
                }
                if let Some(d) = doc_of_attrs(&f.attrs) {
                    self.sink.push_doc(id, &d);
                }
            }
            syn::TraitItem::Type(t) => {
                let name = t.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Rust(RustKind::AssocType),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(t),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
            }
            syn::TraitItem::Const(c) => {
                let name = c.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Rust(RustKind::AssocConst),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(c),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
            }
            _ => {}
        }
    }

    fn walk_impl(&mut self, i: &syn::ItemImpl) {
        // Synthesize a stable name. Include the impl's start line so multiple
        // impls of the same trait/type within one file get distinct ids.
        let self_ty_name = path_of_type(&i.self_ty).unwrap_or_else(|| "?".to_string());
        let trait_name = i
            .trait_
            .as_ref()
            .map(|(_, p, _)| path_of_path(p))
            .unwrap_or_default();
        let line = i.span().start().line;
        let synthetic_name = if trait_name.is_empty() {
            format!("impl_for_{}@{}", self_ty_name, line)
        } else {
            format!("impl_{}_for_{}@{}", trait_name, self_ty_name, line)
        };
        let qualified = self.qualify(&synthetic_name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.emit_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Rust(RustKind::Impl),
            label: synthetic_name.clone(),
            qualified,
            file: self.file.clone(),
            span: span_of(i),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        if let Some(d) = doc_of_attrs(&i.attrs) {
            self.sink.push_doc(id, &d);
        }

        // ImplFor: link impl → type (if resolvable to an in-scope simple ident).
        if let Some(ty_ident) = simple_ident(&self_ty_name) {
            let target_qualified = self.qualify(ty_ident);
            let target_id = self.make_id(&target_qualified);
            self.emit_edge(id, target_id, EdgeKind::ImplFor);
            // Materialized convenience edge: type → trait.
            if !trait_name.is_empty() {
                if let Some(tr_ident) = simple_ident(&trait_name) {
                    let trait_qualified = self.qualify(tr_ident);
                    let trait_id = self.make_id(&trait_qualified);
                    self.emit_edge(target_id, trait_id, EdgeKind::Implements);
                }
            }
        }
        // ImplOfTrait: link impl → trait (simple-ident only for v1).
        if !trait_name.is_empty() {
            if let Some(tr_ident) = simple_ident(&trait_name) {
                let trait_qualified = self.qualify(tr_ident);
                let trait_id = self.make_id(&trait_qualified);
                self.emit_edge(id, trait_id, EdgeKind::ImplOfTrait);
            }
        }

        // Walk impl items.
        let qualified_impl = self.qualify(&synthetic_name);
        for item in &i.items {
            self.walk_impl_item(&qualified_impl, id, item);
        }
    }

    fn walk_impl_item(
        &mut self,
        parent_qualified: &str,
        parent_id: NodeId,
        item: &syn::ImplItem,
    ) {
        match item {
            syn::ImplItem::Fn(f) => {
                let name = f.sig.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Common(CommonKind::Method),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(f),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
                if f.sig.asyncness.is_some() {
                    self.sink.push_property(id, "async", "true");
                }
                if f.sig.unsafety.is_some() {
                    self.sink.push_property(id, "unsafe", "true");
                }
                if let Some(d) = doc_of_attrs(&f.attrs) {
                    self.sink.push_doc(id, &d);
                }
            }
            syn::ImplItem::Type(t) => {
                let name = t.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Rust(RustKind::AssocType),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(t),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
            }
            syn::ImplItem::Const(c) => {
                let name = c.ident.to_string();
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.emit_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Rust(RustKind::AssocConst),
                    label: name,
                    qualified,
                    file: self.file.clone(),
                    span: span_of(c),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
            }
            _ => {}
        }
    }

    fn walk_macro_decl(&mut self, m: &syn::ItemMacro) {
        // `macro_rules! foo { ... }` is the only form syn surfaces here as
        // Item::Macro with an ident. Top-level macro *invocations* (like
        // `lazy_static!{}` at module scope) also parse as Item::Macro; we
        // only treat it as a declaration when the path is `macro_rules`.
        let path_str = path_of_path(&m.mac.path);
        if path_str == "macro_rules" {
            let Some(name) = m.ident.as_ref().map(|i| i.to_string()) else {
                return;
            };
            let qualified = self.qualify(&name);
            let id = self.make_id(&qualified);
            let parent = self.current_parent();
            self.emit_node(NodeRef {
                id,
                lang: LANG,
                kind: NodeKind::Rust(RustKind::MacroDecl(MacroFlavor::Rules)),
                label: name,
                qualified,
                file: self.file.clone(),
                span: span_of(m),
                synthetic: false,
            });
            self.emit_contains(parent, id);
            if let Some(d) = doc_of_attrs(&m.attrs) {
                self.sink.push_doc(id, &d);
            }
        }
    }

    fn record_type_kind(&mut self, id: NodeId, kind: &str) {
        self.sink.push_property(id, "type_kind", kind);
    }

    fn record_attrs(&mut self, id: NodeId, attrs: &[syn::Attribute]) {
        let mut derives: Vec<String> = Vec::new();
        let mut macro_attrs: Vec<String> = Vec::new();
        for attr in attrs {
            if attr.path().is_ident("derive") {
                let _ = attr.parse_nested_meta(|meta| {
                    if let Some(ident) = meta.path.get_ident() {
                        derives.push(ident.to_string());
                    }
                    Ok(())
                });
            } else if !is_doc_attr(attr) && !is_known_builtin_attr(attr) {
                if let Some(name) = attr.path().get_ident().map(|i| i.to_string()) {
                    macro_attrs.push(name);
                }
            }
        }
        if !derives.is_empty() {
            self.sink.push_property(id, "derives", &derives.join(","));
        }
        if !macro_attrs.is_empty() {
            self.sink
                .push_property(id, "attribute_macros", &macro_attrs.join(","));
        }
    }
}

fn span_of<S: Spanned>(node: &S) -> Span {
    span_from(node.span())
}

fn span_from(span: proc_macro2::Span) -> Span {
    let LineColumn {
        line: sl,
        column: sc,
    } = span.start();
    let LineColumn {
        line: el,
        column: ec,
    } = span.end();
    Span {
        start_line: sl as u32,
        start_col: (sc + 1) as u32,
        end_line: el as u32,
        end_col: (ec + 1) as u32,
    }
}

fn whole_file_span(file: &syn::File) -> Span {
    if let (Some(first), Some(last)) = (file.items.first(), file.items.last()) {
        let s = span_of(first);
        let e = span_of(last);
        Span {
            start_line: 1,
            start_col: 1,
            end_line: e.end_line.max(s.end_line),
            end_col: 1,
        }
    } else {
        Span::point(1, 1)
    }
}

fn doc_of_attrs(attrs: &[syn::Attribute]) -> Option<String> {
    let mut out = String::new();
    for attr in attrs {
        if !is_doc_attr(attr) {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                if !out.is_empty() {
                    out.push('\n');
                }
                let raw = s.value();
                out.push_str(raw.strip_prefix(' ').unwrap_or(&raw));
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn is_doc_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("doc")
}

fn is_known_builtin_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    [
        "doc",
        "derive",
        "cfg",
        "cfg_attr",
        "allow",
        "warn",
        "deny",
        "forbid",
        "must_use",
        "inline",
        "non_exhaustive",
        "repr",
        "test",
        "ignore",
        "should_panic",
        "bench",
    ]
    .iter()
    .any(|s| path.is_ident(s))
}

fn path_of_type(ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(tp) = ty {
        Some(path_of_path(&tp.path))
    } else {
        None
    }
}

fn path_of_path(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// If `s` parses as a simple `ident` (no `::`), return that ident; else
/// `None` so callers know they cannot resolve cross-module within v1.
fn simple_ident(s: &str) -> Option<&str> {
    if s.contains("::") || s.is_empty() {
        None
    } else {
        Some(s)
    }
}
