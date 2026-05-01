//! @arch:layer(kg_lang)
//! @arch:role(traverse)
//!
//! Tree-sitter-based walker over a parsed TypeScript / TSX source tree.
//!
//! The grammar is permissive: malformed source still parses into a tree
//! with `ERROR`/`MISSING` nodes. We tolerate that — any unrecognized or
//! errored subtree is silently skipped.
//!
//! What we recurse into:
//! * the program root,
//! * `export_statement` (transparent wrapper),
//! * `class_body` / `interface_body` / `enum_body`,
//! * `internal_module` / `module` bodies (TS namespaces).
//!
//! What we do NOT recurse into:
//! * function/method bodies (locals are not graph-worthy in Pass 1+2),
//! * type expressions (no `References` edges yet),
//! * import/export *bindings* — but the `from "..."` specifier on
//!   `import_statement` and re-export `export_statement` is collected
//!   onto the file node's `imports` property for Pass 3 to resolve.

use tree_sitter::Node;
use kg::edge::{EdgeId, EdgeKind, EdgeOut};
use kg::ids::{NodeId, NodeRef, Span};
use kg::indexer::IndexSink;
use kg::kind::{CommonKind, Lang, NodeKind, TsKind};

const LANG: Lang = Lang::Ts;

mod kind {
    pub const CALL_EXPRESSION: &str = "call_expression";
    pub const CLASS: &str = "class_declaration";
    pub const ABSTRACT_CLASS: &str = "abstract_class_declaration";
    pub const INTERFACE: &str = "interface_declaration";
    pub const TYPE_ALIAS: &str = "type_alias_declaration";
    pub const ENUM: &str = "enum_declaration";
    pub const FUNCTION: &str = "function_declaration";
    pub const FUNCTION_SIG: &str = "function_signature";
    pub const LEXICAL_DECL: &str = "lexical_declaration";
    pub const VARIABLE_DECL: &str = "variable_declaration";
    pub const NAMESPACE: &str = "internal_module";
    pub const MODULE: &str = "module";
    pub const EXPORT: &str = "export_statement";
    pub const IMPORT: &str = "import_statement";
    pub const STRING: &str = "string";
    pub const STRING_FRAGMENT: &str = "string_fragment";
    pub const EXPR_STMT: &str = "expression_statement";
    pub const AMBIENT_DECL: &str = "ambient_declaration";
    pub const METHOD: &str = "method_definition";
    pub const METHOD_SIG: &str = "method_signature";
    pub const ABSTRACT_METHOD: &str = "abstract_method_signature";
    pub const PUBLIC_FIELD: &str = "public_field_definition";
    pub const PROPERTY_SIG: &str = "property_signature";
    pub const ENUM_ASSIGN: &str = "enum_assignment";
    pub const PROPERTY_IDENT: &str = "property_identifier";
    pub const CLASS_HERITAGE: &str = "class_heritage";
    pub const EXTENDS_CLAUSE: &str = "extends_clause";
    pub const EXTENDS_TYPE_CLAUSE: &str = "extends_type_clause";
    pub const IMPLEMENTS_CLAUSE: &str = "implements_clause";
    pub const DECORATOR: &str = "decorator";
    pub const CLASS_BODY: &str = "class_body";
    pub const INTERFACE_BODY: &str = "interface_body";
    pub const OBJECT_TYPE: &str = "object_type";
    pub const ENUM_BODY: &str = "enum_body";
    pub const VARIABLE_DECLARATOR: &str = "variable_declarator";
    pub const ARROW_FUNCTION: &str = "arrow_function";
    pub const FUNCTION_EXPR: &str = "function_expression";
    pub const IDENTIFIER: &str = "identifier";
    pub const TYPE_IDENT: &str = "type_identifier";
    pub const NESTED_TYPE_IDENT: &str = "nested_type_identifier";
    pub const JSX_ELEMENT: &str = "jsx_element";
    pub const JSX_SELF_CLOSING: &str = "jsx_self_closing_element";
    pub const JSX_FRAGMENT: &str = "jsx_fragment";
    pub const COMMENT: &str = "comment";
}

pub struct Walker<'a> {
    file: String,
    file_id: NodeId,
    src: &'a [u8],
    is_tsx: bool,
    parents: Vec<NodeId>,
    mod_path: Vec<String>,
    /// Top-level import / re-export specifiers collected during the walk
    /// (the literal string after `from`, e.g. `./Foo`, `react`, `@/lib/x`).
    /// Drained onto the file node's `imports` property at the end of
    /// [`Walker::run`] so the daemon's Pass 3 cross-file resolver can read
    /// them back. Mirrors the Rust walker's `imports` field.
    imports: Vec<String>,
    /// `Calls` edges deferred until the structural walk finishes — same
    /// trick the Rust walker uses. The store drops edges whose target
    /// id has no node, so `bar()` calls into imported / external
    /// symbols simply disappear; only same-file unambiguous resolutions
    /// (a single-ident callee that maps onto a same-namespace function
    /// or function-valued constant) survive.
    pending_calls: Vec<(NodeId, NodeId)>,
    sink: &'a mut dyn IndexSink,
}

impl<'a> Walker<'a> {
    pub fn new(file: &str, src: &'a [u8], is_tsx: bool, sink: &'a mut dyn IndexSink) -> Self {
        let file = file.replace('\\', "/");
        let file_id = NodeId::compute(LANG, &file, &file);
        Self {
            file,
            file_id,
            src,
            is_tsx,
            parents: Vec::new(),
            mod_path: Vec::new(),
            imports: Vec::new(),
            pending_calls: Vec::new(),
            sink,
        }
    }

    pub fn run(&mut self, root: Node) {
        let label = self
            .file
            .rsplit_once('/')
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| self.file.clone());

        self.sink.push_node(NodeRef {
            id: self.file_id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::File),
            label,
            qualified: self.file.clone(),
            file: self.file.clone(),
            span: span_of(root),
            synthetic: false,
        });
        if self.is_tsx {
            self.sink.push_property(self.file_id, "tsx", "true");
        }
        if let Some(doc) = collect_file_docs(root, self.src) {
            self.sink.push_doc(self.file_id, &doc);
        }

        self.parents.push(self.file_id);
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            self.walk_top(child, false);
        }
        self.parents.pop();

        // Drain collected import specifiers onto the file node. Newline-joined
        // mirrors the convention `Walker` (Rust) uses for `imports`.
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

    fn emit_contains(&mut self, parent: NodeId, child: NodeId) {
        self.sink.push_edge(EdgeOut {
            id: EdgeId::compute(parent, child, &EdgeKind::Contains),
            from: parent,
            to: child,
            kind: EdgeKind::Contains,
            annotations: vec![],
        });
    }

    fn emit_edge(&mut self, from: NodeId, to: NodeId, edge: EdgeKind) {
        self.sink.push_edge(EdgeOut {
            id: EdgeId::compute(from, to, &edge),
            from,
            to,
            kind: edge,
            annotations: vec![],
        });
    }

    /// Attach any preceding `/** ... */` or `///` doc comments to `target`.
    /// Walks up through wrapper nodes (`export_statement`, `expression_statement`,
    /// `ambient_declaration`) so the comment above `export class Foo {}` lands
    /// on the class, not silently on the wrapper.
    fn attach_docs(&mut self, node: Node, target: NodeId) {
        if let Some(doc) = collect_item_docs(node, self.src) {
            self.sink.push_doc(target, &doc);
        }
    }

    fn walk_top(&mut self, node: Node, is_default_export: bool) {
        let mut is_default = is_default_export;
        match node.kind() {
            kind::IMPORT => {
                self.collect_import_source(node);
            }
            kind::EXPORT => {
                // `export ...`. May be `export default` and may wrap a decl.
                if has_token_child(node, "default") {
                    is_default = true;
                }
                // `export { Foo } from "./bar"` and `export * from "./bar"`
                // both attach a "source" field — same dependency shape as
                // a plain `import`. Capture it so Pass 3 sees re-exports.
                self.collect_import_source(node);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(
                        child.kind(),
                        kind::CLASS
                            | kind::ABSTRACT_CLASS
                            | kind::INTERFACE
                            | kind::TYPE_ALIAS
                            | kind::ENUM
                            | kind::FUNCTION
                            | kind::FUNCTION_SIG
                            | kind::LEXICAL_DECL
                            | kind::VARIABLE_DECL
                            | kind::NAMESPACE
                            | kind::MODULE
                    ) {
                        self.walk_top(child, is_default);
                    }
                }
            }
            kind::CLASS | kind::ABSTRACT_CLASS => self.walk_class(node, is_default),
            kind::INTERFACE => self.walk_interface(node, is_default),
            kind::TYPE_ALIAS => self.walk_type_alias(node, is_default),
            kind::ENUM => self.walk_enum(node, is_default),
            kind::FUNCTION | kind::FUNCTION_SIG => self.walk_function(node, is_default),
            kind::LEXICAL_DECL | kind::VARIABLE_DECL => self.walk_var_decl(node, is_default),
            kind::NAMESPACE | kind::MODULE => self.walk_namespace(node),
            // tree-sitter wraps `namespace foo {}` in an expression_statement
            // because the `namespace` keyword isn't reserved; descend through.
            // Same story for `declare namespace foo {}` → ambient_declaration.
            kind::EXPR_STMT | kind::AMBIENT_DECL => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_top(child, is_default);
                }
            }
            _ => {}
        }
    }

    /// Pull the literal text out of the `source` field of an
    /// `import_statement` or re-export `export_statement` and stash it on
    /// `self.imports`. The source field is a `string` node whose payload
    /// is a `string_fragment` child; falling back to the raw `string`
    /// text (with quotes stripped) keeps us robust against grammar drift.
    fn collect_import_source(&mut self, node: Node) {
        let Some(source) = node.child_by_field_name("source") else {
            return;
        };
        let spec = if source.kind() == kind::STRING {
            first_descendant_kind(source, kind::STRING_FRAGMENT)
                .map(|n| node_text(n, self.src).to_string())
                .unwrap_or_else(|| trim_string_quotes(node_text(source, self.src)).to_string())
        } else {
            trim_string_quotes(node_text(source, self.src)).to_string()
        };
        if spec.is_empty() {
            return;
        }
        self.imports.push(spec);
    }

    fn walk_class(&mut self, node: Node, is_default: bool) {
        let name = field_name_text(node, "name", self.src).unwrap_or("anonymous_class");
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();

        // JSX components: capitalized class in a .tsx file. We use the same
        // heuristic as the function path — capitalized name in .tsx — rather
        // than scanning method bodies for JSX returns.
        let kind = if self.is_tsx && starts_with_uppercase(name) {
            NodeKind::Ts(TsKind::JsxComponent)
        } else {
            NodeKind::Common(CommonKind::Type)
        };

        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind,
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);
        self.sink.push_property(id, "type_kind", "class");
        if node.kind() == kind::ABSTRACT_CLASS {
            self.sink.push_property(id, "abstract", "true");
        }
        if is_default {
            self.sink.push_property(id, "default_export", "true");
        }
        self.record_decorators(node, id);

        // Heritage: class_heritage may contain extends_clause and implements_clause.
        if let Some(heritage) = first_child_of_kind(node, kind::CLASS_HERITAGE) {
            let mut hcursor = heritage.walk();
            for child in heritage.children(&mut hcursor) {
                match child.kind() {
                    kind::EXTENDS_CLAUSE => {
                        if let Some(target) = first_descendant_kind(child, kind::IDENTIFIER)
                            .or_else(|| first_descendant_kind(child, kind::TYPE_IDENT))
                        {
                            let target_name =
                                node_text(target, self.src).to_string();
                            self.sink.push_property(id, "extends_target", &target_name);
                            // Best-effort same-file edge: only emit if a same-file
                            // node already shares the qualified name we'd compute.
                            self.maybe_emit_extends(id, &target_name);
                        }
                    }
                    kind::IMPLEMENTS_CLAUSE => {
                        let mut icursor = child.walk();
                        for cand in child.children(&mut icursor) {
                            if matches!(cand.kind(), kind::TYPE_IDENT | kind::IDENTIFIER) {
                                let target_name = node_text(cand, self.src).to_string();
                                self.sink.push_property(id, "implements_target", &target_name);
                                self.maybe_emit_implements(id, &target_name);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Class body: walk methods + fields.
        if let Some(body) = first_child_of_kind(node, kind::CLASS_BODY) {
            self.parents.push(id);
            let mut cursor = body.walk();
            let qualified_class = self.qualify(name);
            for member in body.children(&mut cursor) {
                self.walk_class_member(&qualified_class, id, member);
            }
            self.parents.pop();
        }
    }

    fn walk_class_member(&mut self, parent_qualified: &str, parent_id: NodeId, node: Node) {
        match node.kind() {
            kind::METHOD | kind::ABSTRACT_METHOD | kind::METHOD_SIG => {
                let Some(name) = field_name_text(node, "name", self.src) else {
                    return;
                };
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.sink.push_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Common(CommonKind::Method),
                    label: name.to_string(),
                    qualified,
                    file: self.file.clone(),
                    span: span_of(node),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.emit_edge(parent_id, id, EdgeKind::Defines);
                self.attach_docs(node, id);
                if has_token_child(node, "async") {
                    self.sink.push_property(id, "async", "true");
                }
                if matches!(node.kind(), kind::ABSTRACT_METHOD) {
                    self.sink.push_property(id, "abstract", "true");
                }
                if matches!(node.kind(), kind::METHOD_SIG | kind::ABSTRACT_METHOD) {
                    self.sink.push_property(id, "required", "true");
                }
                self.record_decorators(node, id);
                if let Some(body) = node.child_by_field_name("body") {
                    self.collect_calls(id, body);
                }
            }
            kind::PUBLIC_FIELD | kind::PROPERTY_SIG => {
                let Some(name) = field_name_text(node, "name", self.src) else {
                    return;
                };
                let qualified = format!("{}::{}", parent_qualified, name);
                let id = self.make_id(&qualified);
                self.sink.push_node(NodeRef {
                    id,
                    lang: LANG,
                    kind: NodeKind::Common(CommonKind::Field),
                    label: name.to_string(),
                    qualified,
                    file: self.file.clone(),
                    span: span_of(node),
                    synthetic: false,
                });
                self.emit_contains(parent_id, id);
                self.attach_docs(node, id);
                self.record_decorators(node, id);
            }
            _ => {}
        }
    }

    fn walk_interface(&mut self, node: Node, is_default: bool) {
        let name = field_name_text(node, "name", self.src).unwrap_or("anonymous_interface");
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();

        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Ts(TsKind::Interface),
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);
        if is_default {
            self.sink.push_property(id, "default_export", "true");
        }

        // extends_type_clause appears before the body.
        if let Some(ext) = first_child_of_kind(node, kind::EXTENDS_TYPE_CLAUSE) {
            let mut cursor = ext.walk();
            for cand in ext.children(&mut cursor) {
                if matches!(cand.kind(), kind::TYPE_IDENT | kind::NESTED_TYPE_IDENT) {
                    let target = node_text(cand, self.src).to_string();
                    self.sink.push_property(id, "extends_target", &target);
                    self.maybe_emit_extends(id, &target);
                }
            }
        }

        // Interface body: object_type with property_signature / method_signature.
        if let Some(body) =
            first_child_of_kind(node, kind::OBJECT_TYPE).or_else(|| first_child_of_kind(node, kind::INTERFACE_BODY))
        {
            self.parents.push(id);
            let qualified_iface = self.qualify(name);
            let mut cursor = body.walk();
            for member in body.children(&mut cursor) {
                self.walk_class_member(&qualified_iface, id, member);
            }
            self.parents.pop();
        }
    }

    fn walk_type_alias(&mut self, node: Node, is_default: bool) {
        let name = field_name_text(node, "name", self.src).unwrap_or("anonymous_alias");
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Ts(TsKind::TypeAlias),
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);
        if is_default {
            self.sink.push_property(id, "default_export", "true");
        }
    }

    fn walk_enum(&mut self, node: Node, is_default: bool) {
        let name = field_name_text(node, "name", self.src).unwrap_or("anonymous_enum");
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Ts(TsKind::Enum),
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);
        if is_default {
            self.sink.push_property(id, "default_export", "true");
        }

        // Variants
        if let Some(body) = first_child_of_kind(node, kind::ENUM_BODY) {
            let qualified_enum = self.qualify(name);
            let mut cursor = body.walk();
            for member in body.children(&mut cursor) {
                let v_name = match member.kind() {
                    kind::PROPERTY_IDENT => Some(node_text(member, self.src)),
                    kind::ENUM_ASSIGN => field_name_text(member, "name", self.src),
                    _ => None,
                };
                let Some(v_name) = v_name else { continue };
                let v_qualified = format!("{}::{}", qualified_enum, v_name);
                let v_id = self.make_id(&v_qualified);
                self.sink.push_node(NodeRef {
                    id: v_id,
                    lang: LANG,
                    kind: NodeKind::Common(CommonKind::Variant),
                    label: v_name.to_string(),
                    qualified: v_qualified,
                    file: self.file.clone(),
                    span: span_of(member),
                    synthetic: false,
                });
                self.emit_contains(id, v_id);
                self.attach_docs(member, v_id);
            }
        }
    }

    fn walk_function(&mut self, node: Node, is_default: bool) {
        let name = field_name_text(node, "name", self.src).unwrap_or("anonymous");
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();

        // JSX heuristic: capitalized name in a .tsx file → JsxComponent.
        let kind = if self.is_tsx && starts_with_uppercase(name) {
            NodeKind::Ts(TsKind::JsxComponent)
        } else {
            NodeKind::Common(CommonKind::Function)
        };

        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind,
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);
        if has_token_child(node, "async") {
            self.sink.push_property(id, "async", "true");
        }
        if node.kind() == kind::FUNCTION_SIG {
            self.sink.push_property(id, "ambient", "true");
        }
        if is_default {
            self.sink.push_property(id, "default_export", "true");
        }
        // Function declarations have a body; function signatures (ambient
        // / interface decls) don't — `child_by_field_name` returning None
        // handles both.
        if let Some(body) = node.child_by_field_name("body") {
            self.collect_calls(id, body);
        }
    }

    fn walk_var_decl(&mut self, node: Node, is_default: bool) {
        // Top-level lexical/variable declarations. Each declarator becomes a
        // Constant node (or a JsxComponent if the value is a function and the
        // name is capitalized in a .tsx file).
        let mut cursor = node.walk();
        for decl in node.children(&mut cursor) {
            if decl.kind() != kind::VARIABLE_DECLARATOR {
                continue;
            }
            let Some(name) = field_name_text(decl, "name", self.src) else {
                continue;
            };
            let qualified = self.qualify(name);
            let id = self.make_id(&qualified);
            let parent = self.current_parent();

            // Detect arrow / function expression value.
            let value = decl.child_by_field_name("value");
            let is_function_value = value
                .map(|v| matches!(v.kind(), kind::ARROW_FUNCTION | kind::FUNCTION_EXPR))
                .unwrap_or(false);

            let kind = if self.is_tsx && is_function_value && starts_with_uppercase(name) {
                NodeKind::Ts(TsKind::JsxComponent)
            } else if is_function_value {
                NodeKind::Common(CommonKind::Function)
            } else {
                NodeKind::Common(CommonKind::Constant)
            };

            self.sink.push_node(NodeRef {
                id,
                lang: LANG,
                kind,
                label: name.to_string(),
                qualified,
                file: self.file.clone(),
                span: span_of(decl),
                synthetic: false,
            });
            self.emit_contains(parent, id);
            // Doc comment is attached to the outer var-decl statement, not the
            // individual declarator inside it.
            self.attach_docs(node, id);
            if is_default {
                self.sink.push_property(id, "default_export", "true");
            }
            if let Some(v) = value {
                if has_jsx_descendant(v) {
                    self.sink.push_property(id, "returns_jsx", "true");
                }
                if is_function_value {
                    // Arrow / function expression: scan its body for calls.
                    // Arrow body may be a `statement_block` (under field
                    // "body") or a single expression (also under "body" —
                    // tree-sitter wraps single-expr bodies the same way).
                    if let Some(body) = v.child_by_field_name("body") {
                        self.collect_calls(id, body);
                    }
                }
            }
        }
    }

    fn walk_namespace(&mut self, node: Node) {
        let Some(name) = field_name_text(node, "name", self.src) else {
            return;
        };
        let qualified = self.qualify(name);
        let id = self.make_id(&qualified);
        let parent = self.current_parent();
        self.sink.push_node(NodeRef {
            id,
            lang: LANG,
            kind: NodeKind::Common(CommonKind::Module),
            label: name.to_string(),
            qualified,
            file: self.file.clone(),
            span: span_of(node),
            synthetic: false,
        });
        self.emit_contains(parent, id);
        self.attach_docs(node, id);

        // Body: a statement_block under field "body".
        if let Some(body) = node.child_by_field_name("body") {
            self.mod_path.push(name.to_string());
            self.parents.push(id);
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.walk_top(child, false);
            }
            self.parents.pop();
            self.mod_path.pop();
        }
    }

    /// Walk every descendant of `body` looking for `call_expression`
    /// nodes whose callee is a bare identifier — `foo()`, not `obj.foo()`,
    /// `Foo.bar()`, or `new Foo()`. Each match queues a `Calls` edge
    /// from `caller` to a same-namespace node with that name. The store
    /// drops the edge if no node with that id exists, so calls into
    /// imported / external / locally-shadowed names disappear silently.
    fn collect_calls(&mut self, caller: NodeId, body: Node) {
        let mut stack: Vec<Node> = vec![body];
        while let Some(node) = stack.pop() {
            if node.kind() == kind::CALL_EXPRESSION {
                if let Some(func) = node.child_by_field_name("function") {
                    if func.kind() == kind::IDENTIFIER {
                        let name = node_text(func, self.src).to_string();
                        if !name.is_empty() {
                            let qualified = self.qualify(&name);
                            let target = self.make_id(&qualified);
                            self.pending_calls.push((caller, target));
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
    }

    fn record_decorators(&mut self, node: Node, target: NodeId) {
        let mut names: Vec<String> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != kind::DECORATOR {
                continue;
            }
            // Decorator child is either an identifier, member_expression,
            // or call_expression. Pull the leftmost identifier.
            if let Some(ident) = first_descendant_kind(child, kind::IDENTIFIER)
                .or_else(|| first_descendant_kind(child, kind::PROPERTY_IDENT))
            {
                names.push(node_text(ident, self.src).to_string());
            }
        }
        if !names.is_empty() {
            self.sink.push_property(target, "decorators", &names.join(","));
        }
    }

    fn maybe_emit_extends(&mut self, from: NodeId, target_name: &str) {
        // Only emit when the target name is a simple ident we can resolve to a
        // same-module qualified id. The store will silently drop dangling edges
        // if no node with that id was emitted.
        if target_name.contains('.') || target_name.contains("::") {
            return;
        }
        let target_qualified = self.qualify(target_name);
        let target_id = self.make_id(&target_qualified);
        self.emit_edge(from, target_id, EdgeKind::Extends);
    }

    fn maybe_emit_implements(&mut self, from: NodeId, target_name: &str) {
        if target_name.contains('.') || target_name.contains("::") {
            return;
        }
        let target_qualified = self.qualify(target_name);
        let target_id = self.make_id(&target_qualified);
        self.emit_edge(from, target_id, EdgeKind::Implements);
    }
}

// ----- helpers -----

fn span_of(node: Node) -> Span {
    let s = node.start_position();
    let e = node.end_position();
    Span {
        start_line: (s.row + 1) as u32,
        start_col: (s.column + 1) as u32,
        end_line: (e.row + 1) as u32,
        end_col: (e.column + 1) as u32,
    }
}

fn node_text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn field_name_text<'a>(node: Node, field: &str, src: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(src).ok())
}

fn first_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        let c = node.child(i)?;
        if c.kind() == kind {
            return Some(c);
        }
    }
    None
}

fn first_descendant_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        let child = node.child(i)?;
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = first_descendant_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// True if the node has an immediate child whose kind() matches `tok`.
/// Used to detect anonymous keyword tokens like `default`, `async`,
/// `abstract` that aren't always exposed as fields.
fn has_token_child(node: Node, tok: &str) -> bool {
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == tok {
                return true;
            }
        }
    }
    false
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Strip a leading + trailing matching quote pair (`'`, `"`, or `` ` ``).
/// Used as a fallback when the tree-sitter `string` node has no
/// `string_fragment` child (e.g. an empty string).
fn trim_string_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if first == last && matches!(first, b'\'' | b'"' | b'`') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

fn has_jsx_descendant(node: Node) -> bool {
    if matches!(
        node.kind(),
        kind::JSX_ELEMENT | kind::JSX_SELF_CLOSING | kind::JSX_FRAGMENT
    ) {
        return true;
    }
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if has_jsx_descendant(c) {
                return true;
            }
        }
    }
    false
}

// ----- doc comment extraction -----

/// Walk up through wrapper nodes (`export_statement`, `expression_statement`,
/// `ambient_declaration`) so the comment authored at the wrapper's level is
/// the one we look at. Without this, `/** … */` above `export class Foo {}`
/// would never reach the inner `class_declaration`.
fn doc_anchor_node<'a>(node: Node<'a>) -> Node<'a> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if matches!(
            parent.kind(),
            kind::EXPORT | kind::EXPR_STMT | kind::AMBIENT_DECL
        ) {
            cur = parent;
        } else {
            break;
        }
    }
    cur
}

/// Collect `/** … */` block and `///` line doc comments immediately preceding
/// `node`. `//!` lines, plain `//` line comments, and `/* … */` (single-star)
/// block comments stop the walk — they're not item docs.
fn collect_item_docs(node: Node, src: &[u8]) -> Option<String> {
    let anchor = doc_anchor_node(node);
    let mut parts: Vec<String> = Vec::new();
    let mut cur = anchor.prev_sibling();
    while let Some(sib) = cur {
        if sib.kind() != kind::COMMENT {
            break;
        }
        let raw = sib.utf8_text(src).unwrap_or("");
        match extract_outer_doc(raw) {
            Some(text) => {
                parts.push(text);
                cur = sib.prev_sibling();
            }
            None => break,
        }
    }
    if parts.is_empty() {
        None
    } else {
        parts.reverse();
        Some(parts.join("\n"))
    }
}

/// Collect `//!` line and leading `/** … */` block comments at the top of the
/// program. `//!` is the same Rust-inner convention already used in yah-ui's
/// `.ts` files; we honor it so `@yah:ticket(…)` headers attach to the file
/// node. We collect from any position up to (but not into) the first non-doc,
/// non-comment node.
fn collect_file_docs(root: Node, src: &[u8]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cursor = root.walk();
    let mut seen_decl = false;
    for child in root.children(&mut cursor) {
        if child.kind() != kind::COMMENT {
            // First non-comment node closes the leading block. We still keep
            // collecting `//!` after this (Rust allows them anywhere) — but
            // any `/** */` block past this point belongs to whatever decl
            // follows it, not the file.
            seen_decl = true;
            continue;
        }
        let raw = child.utf8_text(src).unwrap_or("");
        if let Some(text) = extract_inner_doc(raw) {
            parts.push(text);
        } else if !seen_decl {
            if let Some(text) = extract_outer_doc(raw) {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Recognize `/** … */` and `///` as item-level doc comments. Returns the
/// stripped body, or `None` when the comment isn't a doc.
fn extract_outer_doc(raw: &str) -> Option<String> {
    if let Some(body) = block_doc_body(raw) {
        return Some(body);
    }
    if let Some(rest) = raw.strip_prefix("///") {
        // `////…` is a visual divider, not a doc.
        if rest.starts_with('/') {
            return None;
        }
        return Some(line_doc_body(rest));
    }
    None
}

/// Recognize `//!` line comments as file-level docs (Rust-inner convention).
fn extract_inner_doc(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("//!")?;
    Some(line_doc_body(rest))
}

fn line_doc_body(rest: &str) -> String {
    rest.strip_prefix(' ').unwrap_or(rest).to_string()
}

fn block_doc_body(raw: &str) -> Option<String> {
    // Must open `/**` and close `*/`, and not be the empty `/**/`.
    if !raw.starts_with("/**") {
        return None;
    }
    if raw == "/**/" || raw == "/***/" {
        return None;
    }
    let inner = raw.strip_prefix("/**")?;
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let mut lines: Vec<String> = Vec::new();
    for (i, line) in inner.lines().enumerate() {
        let body = if i == 0 {
            // First line is on the same row as `/**`; trim leading spaces only.
            line.trim_start()
        } else {
            // Subsequent lines: strip leading whitespace, optional `*`, then a
            // single space (the JSDoc convention).
            let trimmed = line.trim_start();
            let after_star = trimmed.strip_prefix('*').unwrap_or(trimmed);
            after_star.strip_prefix(' ').unwrap_or(after_star)
        };
        // Trailing whitespace inside doc lines is never meaningful — and a
        // single-line `/** foo */` ends up as `foo ` after stripping markers.
        lines.push(body.trim_end().to_string());
    }
    while lines.first().map(|s| s.trim().is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
