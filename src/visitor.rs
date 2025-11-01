// Future: Advanced AST traversal utilities for finding specific patterns
// For now, basic traversal is handled in editor.rs

use syn::visit::Visit;
use syn::{*, spanned::Spanned};

pub struct NodeFinder {
    pub matches: Vec<NodeMatch>,
}

#[derive(Debug, Clone)]
pub enum NodeMatch {
    Struct { name: String, span: proc_macro2::Span },
    Enum { name: String, span: proc_macro2::Span },
    Function { name: String, span: proc_macro2::Span },
    MatchExpr { span: proc_macro2::Span },
}

impl NodeFinder {
    pub fn new() -> Self {
        Self {
            matches: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for NodeFinder {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.matches.push(NodeMatch::Struct {
            name: node.ident.to_string(),
            span: node.span(),
        });
        syn::visit::visit_item_struct(self, node);
    }
    
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.matches.push(NodeMatch::Enum {
            name: node.ident.to_string(),
            span: node.span(),
        });
        syn::visit::visit_item_enum(self, node);
    }
    
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.matches.push(NodeMatch::Function {
            name: node.sig.ident.to_string(),
            span: node.span(),
        });
        syn::visit::visit_item_fn(self, node);
    }
    
    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        self.matches.push(NodeMatch::MatchExpr {
            span: node.span(),
        });
        syn::visit::visit_expr_match(self, node);
    }
}
