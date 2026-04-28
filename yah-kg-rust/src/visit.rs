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
    /// Top-level `use` paths collected during the walk. Emitted as the
    /// file node's `imports` property at the end of [`Walker::run`] so
    /// the daemon's Pass 3 cross-file resolver can read them back.
    /// v1 only collects file-level uses (skips `mod foo { use ... }`)
    /// because inline-mod `super::`/`self::` semantics need the inline
    /// path to resolve correctly — out of scope for the first cut.
    imports: Vec<String>,
    /// `Calls` edges deferred until the structural walk finishes. The
    /// store drops edges whose target id is missing, so we wait until
    /// every same-file fn/method node has been emitted before flushing —
    /// otherwise a forward call (`fn a() { b() } fn b() {}`) would race
    /// the target node and silently drop. v1 only resolves simple-ident
    /// callees (`foo()`, not `Foo::method()` or `x.foo()`); ambiguous
    /// or multi-segment call paths are skipped.
    pending_calls: Vec<(NodeId, NodeId)>,
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
            imports: Vec::new(),
            pending_calls: Vec::new(),
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

        // Drain collected use paths onto the file node. Newline-joined
        // mirrors the convention `record_attrs` uses for `derives`.
        if !self.imports.is_empty() {
            let joined = self.imports.join("\n");
            self.sink.push_property(self.file_id, "imports", &joined);
        }

        // Flush deferred Calls edges. The store drops edges whose target
        // id has no node, so unresolved/external call sites disappear.
        let pending = std::mem::take(&mut self.pending_calls);
        for (from, to) in pending {
            self.emit_edge(from, to, EdgeKind::Calls);
        }
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
            syn::Item::Use(u) => {
                if self.mod_path.is_empty() {
                    self.collect_use(u);
                }
            }
            // ExternCrate, ForeignMod, TraitAlias, Verbatim — Pass 3 work.
            _ => {}
        }
    }

    /// Flatten one `use` tree into normalized path strings and stash on
    /// `self.imports`. Globs are kept as `path::*`; renames are stored
    /// under their original target so the resolver doesn't have to guess
    /// what `as Foo` aliased.
    fn collect_use(&mut self, u: &syn::ItemUse) {
        let leading = u.leading_colon.is_some();
        let mut prefix: Vec<String> = if leading {
            // `use ::foo` is absolute (external in 2018+ unless a crate
            // named `foo` is in deps). We don't try to resolve external
            // paths today, so tag the leading `::` and let the resolver
            // skip it.
            vec![String::new()]
        } else {
            Vec::new()
        };
        collect_use_paths(&u.tree, &mut prefix, &mut self.imports);
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
        self.collect_calls_in_block(id, &f.block);
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
                if let Some(default) = &f.default {
                    self.collect_calls_in_block(id, default);
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
                self.collect_calls_in_block(id, &f.block);
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

    /// Scan `block` for call sites and queue a `Calls` edge from `caller`
    /// to each unambiguously-resolvable callee. Resolution is deliberately
    /// narrow: only a single-ident path callee (`foo()`) where the same
    /// module already defines `fn foo` will produce an edge — anything
    /// requiring type inference (`x.foo()`), trait dispatch, or external-
    /// crate path resolution is dropped on the floor by the store when
    /// the target id has no node.
    fn collect_calls_in_block(&mut self, caller: NodeId, block: &syn::Block) {
        let mut names: Vec<String> = Vec::new();
        collect_call_names_in_block(block, &mut names);
        for name in names {
            let qualified = self.qualify(&name);
            let target = self.make_id(&qualified);
            self.pending_calls.push((caller, target));
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

/// Recursively scan every `Stmt`/`Expr` in `block` and append the
/// callee name of each `Expr::Call` whose path is a single ident
/// without generic arguments. Multi-segment paths (`Foo::new()`,
/// `crate::foo()`), method calls (`x.foo()`), macro invocations, and
/// closures-of-closures all fall through silently.
fn collect_call_names_in_block(block: &syn::Block, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        collect_call_names_in_stmt(stmt, out);
    }
}

fn collect_call_names_in_stmt(stmt: &syn::Stmt, out: &mut Vec<String>) {
    match stmt {
        syn::Stmt::Local(l) => {
            if let Some(init) = &l.init {
                collect_call_names_in_expr(&init.expr, out);
                if let Some((_, e)) = &init.diverge {
                    collect_call_names_in_expr(e, out);
                }
            }
        }
        syn::Stmt::Expr(e, _) => collect_call_names_in_expr(e, out),
        // `Stmt::Item` introduces nested items (e.g. nested `fn`); their
        // bodies aren't part of the parent fn's call surface — skip.
        // `Stmt::Macro` (macro statements like `println!()`) we can't
        // resolve without expansion, so skip.
        _ => {}
    }
}

fn collect_call_names_in_expr(expr: &syn::Expr, out: &mut Vec<String>) {
    use syn::Expr;
    match expr {
        Expr::Call(c) => {
            if let Some(name) = simple_call_name(&c.func) {
                out.push(name);
            } else {
                collect_call_names_in_expr(&c.func, out);
            }
            for a in &c.args {
                collect_call_names_in_expr(a, out);
            }
        }
        Expr::MethodCall(m) => {
            collect_call_names_in_expr(&m.receiver, out);
            for a in &m.args {
                collect_call_names_in_expr(a, out);
            }
        }
        Expr::Block(b) => collect_call_names_in_block(&b.block, out),
        Expr::Async(a) => collect_call_names_in_block(&a.block, out),
        Expr::Unsafe(u) => collect_call_names_in_block(&u.block, out),
        Expr::TryBlock(t) => collect_call_names_in_block(&t.block, out),
        Expr::If(i) => {
            collect_call_names_in_expr(&i.cond, out);
            collect_call_names_in_block(&i.then_branch, out);
            if let Some((_, e)) = &i.else_branch {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Match(m) => {
            collect_call_names_in_expr(&m.expr, out);
            for arm in &m.arms {
                if let Some((_, g)) = &arm.guard {
                    collect_call_names_in_expr(g, out);
                }
                collect_call_names_in_expr(&arm.body, out);
            }
        }
        Expr::Loop(l) => collect_call_names_in_block(&l.body, out),
        Expr::While(w) => {
            collect_call_names_in_expr(&w.cond, out);
            collect_call_names_in_block(&w.body, out);
        }
        Expr::ForLoop(f) => {
            collect_call_names_in_expr(&f.expr, out);
            collect_call_names_in_block(&f.body, out);
        }
        Expr::Closure(c) => collect_call_names_in_expr(&c.body, out),
        Expr::Await(a) => collect_call_names_in_expr(&a.base, out),
        Expr::Try(t) => collect_call_names_in_expr(&t.expr, out),
        Expr::Return(r) => {
            if let Some(e) = &r.expr {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Yield(y) => {
            if let Some(e) = &y.expr {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Reference(r) => collect_call_names_in_expr(&r.expr, out),
        Expr::Unary(u) => collect_call_names_in_expr(&u.expr, out),
        Expr::Binary(b) => {
            collect_call_names_in_expr(&b.left, out);
            collect_call_names_in_expr(&b.right, out);
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Index(i) => {
            collect_call_names_in_expr(&i.expr, out);
            collect_call_names_in_expr(&i.index, out);
        }
        Expr::Field(f) => collect_call_names_in_expr(&f.base, out),
        Expr::Cast(c) => collect_call_names_in_expr(&c.expr, out),
        Expr::Paren(p) => collect_call_names_in_expr(&p.expr, out),
        Expr::Group(g) => collect_call_names_in_expr(&g.expr, out),
        Expr::Range(r) => {
            if let Some(s) = &r.start {
                collect_call_names_in_expr(s, out);
            }
            if let Some(e) = &r.end {
                collect_call_names_in_expr(e, out);
            }
        }
        Expr::Let(l) => collect_call_names_in_expr(&l.expr, out),
        Expr::Assign(a) => {
            collect_call_names_in_expr(&a.left, out);
            collect_call_names_in_expr(&a.right, out);
        }
        Expr::Struct(s) => {
            for f in &s.fields {
                collect_call_names_in_expr(&f.expr, out);
            }
            if let Some(rest) = &s.rest {
                collect_call_names_in_expr(rest, out);
            }
        }
        Expr::Repeat(r) => {
            collect_call_names_in_expr(&r.expr, out);
            collect_call_names_in_expr(&r.len, out);
        }
        Expr::Break(b) => {
            if let Some(e) = &b.expr {
                collect_call_names_in_expr(e, out);
            }
        }
        // Path / Lit / Macro / Const / Continue / Infer / Verbatim — leaves
        // we can't / don't try to recurse into.
        _ => {}
    }
}

/// Return the callee name iff `e` is a single-segment path expression
/// without generic arguments, qself, or a leading `::`. Anything else
/// is too ambiguous to resolve unambiguously without type info.
fn simple_call_name(e: &syn::Expr) -> Option<String> {
    let syn::Expr::Path(p) = e else { return None };
    if p.qself.is_some() || p.path.leading_colon.is_some() || p.path.segments.len() != 1 {
        return None;
    }
    let seg = &p.path.segments[0];
    if !matches!(seg.arguments, syn::PathArguments::None) {
        return None;
    }
    Some(seg.ident.to_string())
}

/// Walk a `syn::UseTree` and append every leaf path (`a::b::Item`,
/// `a::b::*`) to `out`. `prefix` accumulates segments as we descend
/// into nested groups. The first segment is one of `crate`, `super`,
/// `self`, or an external/dep ident — the resolver branches on that.
fn collect_use_paths(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            collect_use_paths(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            prefix.push(n.ident.to_string());
            out.push(prefix.join("::"));
            prefix.pop();
        }
        syn::UseTree::Rename(r) => {
            prefix.push(r.ident.to_string());
            out.push(prefix.join("::"));
            prefix.pop();
        }
        syn::UseTree::Glob(_) => {
            let mut p = prefix.clone();
            p.push("*".to_string());
            out.push(p.join("::"));
        }
        syn::UseTree::Group(g) => {
            for item in &g.items {
                collect_use_paths(item, prefix, out);
            }
        }
    }
}
