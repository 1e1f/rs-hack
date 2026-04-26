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
//! * import/export bindings (cross-file work is Pass 3).

use tree_sitter::Node;
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::indexer::IndexSink;
use yah_kg::kind::{CommonKind, Lang, NodeKind, TsKind};

const LANG: Lang = Lang::Ts;

mod kind {
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
}

pub struct Walker<'a> {
    file: String,
    file_id: NodeId,
    src: &'a [u8],
    is_tsx: bool,
    parents: Vec<NodeId>,
    mod_path: Vec<String>,
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

        self.parents.push(self.file_id);
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            self.walk_top(child, false);
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

    fn walk_top(&mut self, node: Node, is_default_export: bool) {
        let mut is_default = is_default_export;
        match node.kind() {
            kind::EXPORT => {
                // `export ...`. May be `export default` and may wrap a decl.
                if has_token_child(node, "default") {
                    is_default = true;
                }
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
            // Imports skipped intentionally — Pass 3.
            _ => {}
        }
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
        if has_token_child(node, "async") {
            self.sink.push_property(id, "async", "true");
        }
        if node.kind() == kind::FUNCTION_SIG {
            self.sink.push_property(id, "ambient", "true");
        }
        if is_default {
            self.sink.push_property(id, "default_export", "true");
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
            if is_default {
                self.sink.push_property(id, "default_export", "true");
            }
            if let Some(v) = value {
                if has_jsx_descendant(v) {
                    self.sink.push_property(id, "returns_jsx", "true");
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
