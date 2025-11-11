use anyhow::{Context, Result};
use proc_macro2::{LineColumn, Span};
use syn::{
    parse_str, File, Item, ItemEnum, ItemStruct,
    Fields, Field, spanned::Spanned, Arm, ExprMatch, ExprStruct,
    visit_mut::VisitMut, Expr,
};
use quote::ToTokens;

use crate::operations::*;
use crate::path_resolver::PathResolver;
use prettyplease;

pub struct RustEditor {
    content: String,
    syntax_tree: File,
    line_offsets: Vec<usize>, // Byte offset for each line start
}

impl RustEditor {
    pub fn new(content: &str) -> Result<Self> {
        let syntax_tree: File = syn::parse_str(content)
            .context("Failed to parse Rust code")?;

        let line_offsets = Self::compute_line_offsets(content);

        Ok(Self {
            content: content.to_string(),
            syntax_tree,
            line_offsets,
        })
    }

    /// Format a field without extra spaces (e.g., "pub name: String" not "pub name : String")
    fn format_field(field: &Field) -> String {
        let mut result = String::new();

        // Add visibility
        if let syn::Visibility::Public(_) = field.vis {
            result.push_str("pub ");
        }

        // Add field name
        if let Some(ident) = &field.ident {
            result.push_str(&ident.to_string());
        }

        // Add colon and type (no space before colon)
        result.push_str(": ");

        // Format type without extra spaces
        let type_str = field.ty.to_token_stream().to_string();
        let type_str = type_str.replace(" < ", "<").replace(" >", ">");
        result.push_str(&type_str);

        result
    }
    
    fn compute_line_offsets(content: &str) -> Vec<usize> {
        let mut offsets = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                offsets.push(i + 1);
            }
        }
        offsets
    }
    
    pub fn apply_operation(&mut self, op: &Operation) -> Result<ModificationResult> {
        match op {
            Operation::AddStructField(op) => self.add_struct_field(op),
            Operation::UpdateStructField(op) => self.update_struct_field(op),
            Operation::RemoveStructField(op) => self.remove_struct_field(op),
            Operation::AddStructLiteralField(op) => self.add_struct_literal_field(op),
            Operation::AddEnumVariant(op) => self.add_enum_variant(op),
            Operation::UpdateEnumVariant(op) => self.update_enum_variant(op),
            Operation::RemoveEnumVariant(op) => self.remove_enum_variant(op),
            Operation::AddMatchArm(op) => self.add_match_arm(op),
            Operation::UpdateMatchArm(op) => self.update_match_arm(op),
            Operation::RemoveMatchArm(op) => self.remove_match_arm(op),
            Operation::AddImplMethod(op) => self.add_impl_method(op),
            Operation::AddUseStatement(op) => self.add_use_statement(op),
            Operation::AddDerive(op) => self.add_derive(op),
            Operation::Transform(op) => self.transform(op),
            Operation::RenameEnumVariant(op) => self.rename_enum_variant(op),
            Operation::RenameFunction(op) => self.rename_function(op),
            Operation::AddDocComment(op) => self.add_doc_comment_surgical(
                &op.target_type,
                &op.name,
                &op.doc_comment,
                &op.style,
            ),
            Operation::UpdateDocComment(op) => self.update_doc_comment_surgical(
                &op.target_type,
                &op.name,
                &op.doc_comment,
                &DocCommentStyle::Line, // Default to line style for updates
            ),
            Operation::RemoveDocComment(op) => self.remove_doc_comment_surgical(
                &op.target_type,
                &op.name,
            ),
        }
    }
    
    pub(crate) fn add_struct_field(&mut self, op: &AddStructFieldOp) -> Result<ModificationResult> {
        let mut modified_nodes = Vec::new();

        // Find the struct and clone it to avoid borrowing issues
        let item_struct = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Struct(s) = item {
                    if s.ident == op.struct_name {
                        return Some(s.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Struct '{}' not found", op.struct_name))?;

        // Check if the struct matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_struct.attrs, where_filter)? {
                // Struct doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // If literal_default is NOT provided, only modify the definition
        if op.literal_default.is_none() {
            // Create backup of original struct before modification
            let backup_node = BackupNode {
                node_type: "ItemStruct".to_string(),
                identifier: op.struct_name.clone(),
                original_content: self.unparse_item(&Item::Struct(item_struct.clone())),
                location: self.span_to_location(item_struct.span()),
            };

            // Insert the field into the struct definition
            let modified = self.insert_struct_field(&item_struct, op)
                .context("Failed to add field to struct definition")?;

            if !modified {
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }

            return Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            });
        }

        // If literal_default IS provided:
        // 1. Try to add to definition (idempotent - silently skips if field exists OR if field_def is incomplete)
        // 2. Always update literals
        let literal_default = op.literal_default.as_ref().unwrap();

        // Check if field_def contains a type (has ':')
        // If it doesn't, skip definition modification (literals-only mode)
        let has_type = op.field_def.contains(':');

        let mut def_modified = false;
        if has_type {
            // Create backup before any modifications
            let backup_node = BackupNode {
                node_type: "ItemStruct".to_string(),
                identifier: op.struct_name.clone(),
                original_content: self.unparse_item(&Item::Struct(item_struct.clone())),
                location: self.span_to_location(item_struct.span()),
            };

            // Try to insert field into definition (idempotent - returns false if already exists)
            def_modified = self.insert_struct_field(&item_struct, op)
                .context("Failed to add field to struct definition")?;

            if def_modified {
                modified_nodes.push(backup_node);
                // Re-parse the content to update syntax_tree with the struct field changes
                self.syntax_tree = syn::parse_str(&self.content)
                    .context("Failed to re-parse content after adding struct field")?;
                self.line_offsets = Self::compute_line_offsets(&self.content);
            }
        }

        // Always update literals when literal_default is provided
        // Extract field name from field_def (e.g., "return_type: Option<Type>" -> "return_type" or just "return_type")
        let field_name = op.field_def.split(':')
            .next()
            .map(|s| s.trim().to_string())
            .context("Failed to extract field name from field definition")?;

        // Create the AddStructLiteralFieldOp
        let literal_op = AddStructLiteralFieldOp {
            struct_name: op.struct_name.clone(),
            field_def: format!("{}: {}", field_name, literal_default),
            position: op.position.clone(),
            struct_path: None,  // Path resolution not available from struct field operations
        };

        // Update all struct literals
        let literal_result = self.add_struct_literal_field(&literal_op)
            .context("Failed to update struct literals")?;
        modified_nodes.extend(literal_result.modified_nodes);

        Ok(ModificationResult {
            changed: true,
            modified_nodes,
        })
    }
    
    fn insert_struct_field(&mut self, item_struct: &ItemStruct, op: &AddStructFieldOp) -> Result<bool> {
        if let Fields::Named(ref fields) = item_struct.fields {
            // Parse the new field
            let field_code = format!("struct Dummy {{ {} }}", op.field_def);
            let dummy: ItemStruct = parse_str(&field_code)
                .context("Failed to parse field definition")?;

            let new_field = if let Fields::Named(ref nf) = dummy.fields {
                nf.named.first()
                    .context("No field found in definition")?
                    .clone()
            } else {
                anyhow::bail!("Expected named field");
            };

            // Check if field already exists
            let new_field_name = new_field.ident.as_ref()
                .map(|i| i.to_string())
                .context("Field must have a name")?;

            if fields.named.iter().any(|f| {
                f.ident.as_ref().map(|i| i.to_string()) == Some(new_field_name.clone())
            }) {
                // Field already exists, skip adding
                return Ok(false);
            }
            
            // Determine insertion point
            let insert_pos = match &op.position {
                InsertPosition::First => {
                    if let Some(first_field) = fields.named.first() {
                        self.span_to_byte_offset(first_field.span().start())
                    } else {
                        // Empty struct, insert after the opening brace
                        let brace_pos = self.span_to_byte_offset(fields.brace_token.span.join().start());
                        brace_pos + 1
                    }
                }
                InsertPosition::Last => {
                    if let Some(last_field) = fields.named.last() {
                        let end = self.span_to_byte_offset(last_field.span().end());
                        // Find the comma or end
                        self.find_after_field_end(end)
                    } else {
                        // Empty struct
                        let brace_pos = self.span_to_byte_offset(fields.brace_token.span.join().start());
                        brace_pos + 1
                    }
                }
                InsertPosition::After(name) => {
                    let field = fields.named.iter()
                        .find(|f| f.ident.as_ref().map(|i| i.to_string()) == Some(name.clone()))
                        .with_context(|| format!("Field '{}' not found", name))?;
                    let end = self.span_to_byte_offset(field.span().end());
                    self.find_after_field_end(end)
                }
                InsertPosition::Before(name) => {
                    let field = fields.named.iter()
                        .find(|f| f.ident.as_ref().map(|i| i.to_string()) == Some(name.clone()))
                        .with_context(|| format!("Field '{}' not found", name))?;
                    self.span_to_byte_offset(field.span().start())
                }
            };
            
            // Format the new field
            let indent = self.get_indentation(insert_pos);
            let field_str = Self::format_field(&new_field);
            let insert_text = if matches!(op.position, InsertPosition::First) {
                format!("\n{}{},", indent, field_str)
            } else {
                format!("\n{}{},", indent, field_str)
            };

            self.content.insert_str(insert_pos, &insert_text);
            return Ok(true);
        }
        
        anyhow::bail!("Struct '{}' does not have named fields", op.struct_name)
    }

    pub(crate) fn update_struct_field(&mut self, op: &UpdateStructFieldOp) -> Result<ModificationResult> {
        // Find the struct and clone it to avoid borrowing issues
        let item_struct = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Struct(s) = item {
                    if s.ident == op.struct_name {
                        return Some(s.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Struct '{}' not found", op.struct_name))?;

        // Check if the struct matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_struct.attrs, where_filter)? {
                // Struct doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original struct before modification
        let backup_node = BackupNode {
            node_type: "ItemStruct".to_string(),
            identifier: op.struct_name.clone(),
            original_content: self.unparse_item(&Item::Struct(item_struct.clone())),
            location: self.span_to_location(item_struct.span()),
        };

        let modified = self.replace_struct_field(&item_struct, op)?;

        Ok(ModificationResult {
            changed: modified,
            modified_nodes: if modified { vec![backup_node] } else { vec![] },
        })
    }

    fn replace_struct_field(&mut self, item_struct: &ItemStruct, op: &UpdateStructFieldOp) -> Result<bool> {
        if let Fields::Named(ref fields) = item_struct.fields {
            // Parse the new field definition to get the field name
            let field_code = format!("struct Dummy {{ {} }}", op.field_def);
            let dummy: ItemStruct = parse_str(&field_code)
                .context("Failed to parse field definition")?;

            let new_field = if let Fields::Named(ref nf) = dummy.fields {
                nf.named.first()
                    .context("No field found in definition")?
                    .clone()
            } else {
                anyhow::bail!("Expected named field");
            };

            // Extract the field name from the parsed field
            let field_name = new_field.ident.as_ref()
                .map(|i| i.to_string())
                .context("Field must have a name")?;

            // Find the existing field
            let existing_field = fields.named.iter()
                .find(|f| f.ident.as_ref().map(|i| i.to_string()) == Some(field_name.clone()))
                .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in struct '{}'", field_name, op.struct_name))?;

            // Get the span of the existing field
            let start = self.span_to_byte_offset(existing_field.span().start());
            let end = self.span_to_byte_offset(existing_field.span().end());

            // Format and replace the field
            let new_field_str = Self::format_field(&new_field);

            // Remove the old field and insert the new one
            self.content.replace_range(start..end, &new_field_str);

            return Ok(true);
        }

        anyhow::bail!("Struct '{}' does not have named fields", op.struct_name)
    }

    pub(crate) fn remove_struct_field(&mut self, op: &RemoveStructFieldOp) -> Result<ModificationResult> {
        // Find the struct and clone it to avoid borrowing issues
        let item_struct = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Struct(s) = item {
                    if s.ident == op.struct_name {
                        return Some(s.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Struct '{}' not found", op.struct_name))?;

        // Check if the struct matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_struct.attrs, where_filter)? {
                // Struct doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original struct before modification
        let backup_node = BackupNode {
            node_type: "ItemStruct".to_string(),
            identifier: op.struct_name.clone(),
            original_content: self.unparse_item(&Item::Struct(item_struct.clone())),
            location: self.span_to_location(item_struct.span()),
        };

        if let Fields::Named(ref fields) = item_struct.fields {
            // Find the field to remove
            let field_to_remove = fields.named.iter()
                .find(|f| f.ident.as_ref().map(|i| i.to_string()) == Some(op.field_name.clone()))
                .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in struct '{}'", op.field_name, op.struct_name))?;

            // Get the span including the comma
            let start = self.span_to_byte_offset(field_to_remove.span().start());
            let mut end = self.span_to_byte_offset(field_to_remove.span().end());

            // Find and include the comma and any trailing whitespace/newline
            while end < self.content.len() {
                match self.content.as_bytes()[end] as char {
                    ',' => {
                        end += 1;
                        // Also consume the newline after the comma if present
                        if end < self.content.len() && self.content.as_bytes()[end] == b'\n' {
                            end += 1;
                        }
                        break;
                    }
                    ' ' | '\t' => end += 1,
                    '\n' => {
                        end += 1;
                        break;
                    }
                    _ => break,
                }
            }

            // Also need to remove leading whitespace/indentation on the same line
            let mut line_start = start;
            while line_start > 0 && self.content.as_bytes()[line_start - 1] != b'\n' {
                line_start -= 1;
            }

            // Check if there's only whitespace between line_start and start
            let before_field = &self.content[line_start..start];
            if before_field.trim().is_empty() {
                // Remove the whole line
                self.content.replace_range(line_start..end, "");
            } else {
                // Just remove the field and comma
                self.content.replace_range(start..end, "");
            }

            return Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            });
        }

        anyhow::bail!("Struct '{}' does not have named fields", op.struct_name)
    }

    pub(crate) fn add_struct_literal_field(&mut self, op: &AddStructLiteralFieldOp) -> Result<ModificationResult> {
        // Parse the field name from field_def (e.g., "return_type: None" -> "return_type")
        let field_name = op.field_def.split(':')
            .next()
            .map(|s| s.trim().to_string())
            .context("Field definition must contain ':'")?;

        // Create a path resolver if a canonical path was provided
        let path_resolver = if let Some(struct_path) = &op.struct_path {
            let mut resolver = PathResolver::new(struct_path)
                .ok_or_else(|| anyhow::anyhow!("Invalid struct path: {}", struct_path))?;

            // Scan the file for use statements to build the alias map
            resolver.scan_file(&self.syntax_tree);
            Some(resolver)
        } else {
            None
        };

        // Collect backups of all struct literal expressions that will be modified
        let backup_nodes = self.collect_struct_literal_backups(&op.struct_name, path_resolver.as_ref());

        // Use a visitor to find and modify all struct literals
        let mut visitor = StructLiteralFieldAdder {
            struct_name: op.struct_name.clone(),
            field_def: op.field_def.clone(),
            field_name,
            position: op.position.clone(),
            path_resolver,
            modified: false,
        };

        visitor.visit_file_mut(&mut self.syntax_tree);

        if visitor.modified {
            // Reformat the entire file for struct literals
            self.content = prettyplease::unparse(&self.syntax_tree);
            Ok(ModificationResult {
                changed: true,
                modified_nodes: backup_nodes,
            })
        } else {
            Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            })
        }
    }

    /// Collect backups of all struct literal expressions for a given struct name
    fn collect_struct_literal_backups(&self, struct_name: &str, path_resolver: Option<&PathResolver>) -> Vec<BackupNode> {
        use syn::visit::Visit;

        struct LiteralCollector<'a> {
            struct_name: String,
            path_resolver: Option<&'a PathResolver>,
            backups: Vec<BackupNode>,
            counter: usize,
        }

        impl<'ast, 'a> Visit<'ast> for LiteralCollector<'a> {
            fn visit_expr(&mut self, node: &'ast Expr) {
                if let Expr::Struct(expr_struct) = node {
                    let matches = if let Some(resolver) = self.path_resolver {
                        // Use PathResolver for safe matching
                        resolver.matches_target(&expr_struct.path)
                    } else {
                        // Fallback to legacy pattern matching
                        // - "Rectangle" → only Rectangle { ... } (no :: prefix)
                        // - "*::Rectangle" → any path ending with Rectangle (View::Rectangle, etc.)
                        // - "View::Rectangle" → exact match only View::Rectangle

                        if self.struct_name.contains("::") {
                            // Pattern contains :: - check for exact or wildcard match
                            if self.struct_name.starts_with("*::") {
                                // Wildcard: *::Rectangle matches any path ending with Rectangle
                                let target_name = &self.struct_name[3..]; // Skip "*::"
                                expr_struct.path.segments.last()
                                    .map(|seg| seg.ident.to_string() == target_name)
                                    .unwrap_or(false)
                            } else {
                                // Exact path match: View::Rectangle
                                let path_str = expr_struct.path.segments.iter()
                                    .map(|seg| seg.ident.to_string())
                                    .collect::<Vec<_>>()
                                    .join("::");
                                path_str == self.struct_name
                            }
                        } else {
                            // No :: in pattern - only match pure struct literals (no path qualifier)
                            expr_struct.path.segments.len() == 1
                                && expr_struct.path.segments.last()
                                    .map(|seg| seg.ident.to_string() == self.struct_name)
                                    .unwrap_or(false)
                        }
                    };

                    if matches {
                        self.backups.push(BackupNode {
                            node_type: "ExprStruct".to_string(),
                            identifier: format!("{}#{}", self.struct_name, self.counter),
                            original_content: expr_struct.to_token_stream().to_string(),
                            location: NodeLocation {
                                line: 0, // We don't have precise location info in visitor
                                column: 0,
                                end_line: 0,
                                end_column: 0,
                            },
                        });
                        self.counter += 1;
                    }
                }
                syn::visit::visit_expr(self, node);
            }
        }

        let mut collector = LiteralCollector {
            struct_name: struct_name.to_string(),
            path_resolver,
            backups: Vec::new(),
            counter: 0,
        };

        collector.visit_file(&self.syntax_tree);
        collector.backups
    }

    pub(crate) fn add_enum_variant(&mut self, op: &AddEnumVariantOp) -> Result<ModificationResult> {
        // Find the enum and clone it to avoid borrowing issues
        let item_enum = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Enum(e) = item {
                    if e.ident == op.enum_name {
                        return Some(e.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Enum '{}' not found", op.enum_name))?;

        // Check if the enum matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_enum.attrs, where_filter)? {
                // Enum doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original enum before modification
        let backup_node = BackupNode {
            node_type: "ItemEnum".to_string(),
            identifier: op.enum_name.clone(),
            original_content: self.unparse_item(&Item::Enum(item_enum.clone())),
            location: self.span_to_location(item_enum.span()),
        };

        let modified = self.insert_enum_variant(&item_enum, op)?;

        Ok(ModificationResult {
            changed: modified,
            modified_nodes: if modified { vec![backup_node] } else { vec![] },
        })
    }
    
    fn insert_enum_variant(&mut self, item_enum: &ItemEnum, op: &AddEnumVariantOp) -> Result<bool> {
        // Parse the new variant
        let variant_code = format!("enum Dummy {{ {} }}", op.variant_def);
        let dummy: ItemEnum = parse_str(&variant_code)
            .context("Failed to parse variant definition")?;

        let new_variant = dummy.variants.first()
            .context("No variant found in definition")?
            .clone();

        // Check if variant already exists
        let variant_name = new_variant.ident.to_string();
        if item_enum.variants.iter().any(|v| v.ident.to_string() == variant_name) {
            // Variant already exists, skip adding
            return Ok(false);
        }

        // Determine insertion point
        let insert_pos = match &op.position {
            InsertPosition::First => {
                if let Some(first_var) = item_enum.variants.first() {
                    self.span_to_byte_offset(first_var.span().start())
                } else {
                    let brace_pos = self.span_to_byte_offset(item_enum.brace_token.span.join().start());
                    brace_pos + 1
                }
            }
            InsertPosition::Last => {
                if let Some(last_var) = item_enum.variants.last() {
                    let end = self.span_to_byte_offset(last_var.span().end());
                    self.find_after_field_end(end)
                } else {
                    let brace_pos = self.span_to_byte_offset(item_enum.brace_token.span.join().start());
                    brace_pos + 1
                }
            }
            InsertPosition::After(name) => {
                let variant = item_enum.variants.iter()
                    .find(|v| v.ident.to_string() == *name)
                    .with_context(|| format!("Variant '{}' not found", name))?;
                let end = self.span_to_byte_offset(variant.span().end());
                self.find_after_field_end(end)
            }
            InsertPosition::Before(name) => {
                let variant = item_enum.variants.iter()
                    .find(|v| v.ident.to_string() == *name)
                    .with_context(|| format!("Variant '{}' not found", name))?;
                self.span_to_byte_offset(variant.span().start())
            }
        };
        
        let indent = self.get_indentation(insert_pos);
        let variant_str = new_variant.to_token_stream().to_string();
        let insert_text = format!("\n{}{},", indent, variant_str);
        
        self.content.insert_str(insert_pos, &insert_text);
        Ok(true)
    }

    fn update_enum_variant(&mut self, op: &UpdateEnumVariantOp) -> Result<ModificationResult> {
        // Find the enum and clone it
        let item_enum = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Enum(e) = item {
                    if e.ident == op.enum_name {
                        return Some(e.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Enum '{}' not found", op.enum_name))?;

        // Check if the enum matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_enum.attrs, where_filter)? {
                // Enum doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original enum before modification
        let backup_node = BackupNode {
            node_type: "ItemEnum".to_string(),
            identifier: op.enum_name.clone(),
            original_content: self.unparse_item(&Item::Enum(item_enum.clone())),
            location: self.span_to_location(item_enum.span()),
        };

        // Parse the new variant to get its name
        let variant_code = format!("enum Dummy {{ {} }}", op.variant_def);
        let dummy: ItemEnum = parse_str(&variant_code)
            .context("Failed to parse variant definition")?;

        let new_variant = dummy.variants.first()
            .context("No variant found in definition")?
            .clone();

        let variant_name = new_variant.ident.to_string();

        // Find the existing variant
        let existing_variant = item_enum.variants.iter()
            .find(|v| v.ident.to_string() == variant_name)
            .ok_or_else(|| anyhow::anyhow!("Variant '{}' not found in enum '{}'", variant_name, op.enum_name))?;

        // Get the span
        let start = self.span_to_byte_offset(existing_variant.span().start());
        let end = self.span_to_byte_offset(existing_variant.span().end());

        // Format and replace
        let variant_str = new_variant.to_token_stream().to_string();
        self.content.replace_range(start..end, &variant_str);

        Ok(ModificationResult {
            changed: true,
            modified_nodes: vec![backup_node],
        })
    }

    pub(crate) fn remove_enum_variant(&mut self, op: &RemoveEnumVariantOp) -> Result<ModificationResult> {
        // Find the enum
        let item_enum = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Enum(e) = item {
                    if e.ident == op.enum_name {
                        return Some(e.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Enum '{}' not found", op.enum_name))?;

        // Check if the enum matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(&item_enum.attrs, where_filter)? {
                // Enum doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original enum before modification
        let backup_node = BackupNode {
            node_type: "ItemEnum".to_string(),
            identifier: op.enum_name.clone(),
            original_content: self.unparse_item(&Item::Enum(item_enum.clone())),
            location: self.span_to_location(item_enum.span()),
        };

        // Find the variant to remove
        let variant_to_remove = item_enum.variants.iter()
            .find(|v| v.ident.to_string() == op.variant_name)
            .ok_or_else(|| anyhow::anyhow!("Variant '{}' not found in enum '{}'", op.variant_name, op.enum_name))?;

        // Get the span including comma
        let start = self.span_to_byte_offset(variant_to_remove.span().start());
        let mut end = self.span_to_byte_offset(variant_to_remove.span().end());

        // Find and include the comma and trailing whitespace
        while end < self.content.len() {
            match self.content.as_bytes()[end] as char {
                ',' => {
                    end += 1;
                    if end < self.content.len() && self.content.as_bytes()[end] == b'\n' {
                        end += 1;
                    }
                    break;
                }
                ' ' | '\t' => end += 1,
                '\n' => {
                    end += 1;
                    break;
                }
                _ => break,
            }
        }

        // Remove leading whitespace on the line
        let mut line_start = start;
        while line_start > 0 && self.content.as_bytes()[line_start - 1] != b'\n' {
            line_start -= 1;
        }

        let before_variant = &self.content[line_start..start];
        if before_variant.trim().is_empty() {
            self.content.replace_range(line_start..end, "");
        } else {
            self.content.replace_range(start..end, "");
        }

        Ok(ModificationResult {
            changed: true,
            modified_nodes: vec![backup_node],
        })
    }

    pub(crate) fn add_match_arm(&mut self, op: &AddMatchArmOp) -> Result<ModificationResult> {
        if op.auto_detect {
            // Auto-detect mode: find all missing enum variants
            self.add_missing_match_arms(op)
        } else {
            // Normal mode: add a single match arm
            self.add_single_match_arm(op)
        }
    }

    fn add_single_match_arm(&mut self, op: &AddMatchArmOp) -> Result<ModificationResult> {
        // Parse the pattern and body by creating a dummy match expression
        let dummy_match = format!("match () {{ {} => {}, }}", op.pattern, op.body);
        let expr: syn::Expr = parse_str(&dummy_match)
            .with_context(|| format!("Failed to parse pattern/body: {} => {}", op.pattern, op.body))?;

        // Extract the arm from the dummy match
        let arm = if let syn::Expr::Match(match_expr) = expr {
            match_expr.arms.into_iter().next()
                .context("Failed to extract arm from dummy match")?
        } else {
            anyhow::bail!("Expected match expression");
        };

        // Collect backup of function before modification
        let backup_node = if let Some(ref fn_name) = op.function_name {
            self.get_function_backup(fn_name)?
        } else {
            // If no function specified, we'll backup all modified functions later
            // For now, create a generic backup
            BackupNode {
                node_type: "Unknown".to_string(),
                identifier: "match_expression".to_string(),
                original_content: String::new(),
                location: NodeLocation {
                    line: 0,
                    column: 0,
                    end_line: 0,
                    end_column: 0,
                },
            }
        };

        // Find and modify match expressions
        let mut visitor = MatchArmAdder {
            target_function: op.function_name.clone(),
            arm_to_add: arm,
            modified: false,
            current_function: None,
            modified_function: None,
        };

        visitor.visit_file_mut(&mut self.syntax_tree);

        if visitor.modified {
            // Replace just the modified function
            self.replace_modified_functions(&visitor.modified_function)?;
            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            })
        } else {
            Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            })
        }
    }

    /// Format a single item to string using prettyplease
    fn unparse_item(&self, item: &Item) -> String {
        let temp_file = syn::File {
            shebang: None,
            attrs: Vec::new(),
            items: vec![item.clone()],
        };
        prettyplease::unparse(&temp_file).trim().to_string()
    }

    /// Get backup of a function before modification
    fn get_function_backup(&self, fn_name: &str) -> Result<BackupNode> {
        for item in &self.syntax_tree.items {
            if let Item::Fn(f) = item {
                if f.sig.ident == fn_name {
                    return Ok(BackupNode {
                        node_type: "ItemFn".to_string(),
                        identifier: fn_name.to_string(),
                        original_content: self.unparse_item(&Item::Fn(f.clone())),
                        location: self.span_to_location(f.span()),
                    });
                }
            }
        }
        anyhow::bail!("Function '{}' not found", fn_name)
    }

    fn add_missing_match_arms(&mut self, op: &AddMatchArmOp) -> Result<ModificationResult> {
        // Get the enum name
        let enum_name = op.enum_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("enum_name is required for auto-detect"))?;

        // Find all enum variants
        let enum_variants = self.find_enum_variants(enum_name)?;

        if enum_variants.is_empty() {
            anyhow::bail!("Enum '{}' not found or has no variants", enum_name);
        }

        // Find existing match arms
        let existing_patterns = self.find_existing_match_patterns(&op.function_name);

        // Determine missing variants
        let mut missing_variants = Vec::new();
        for variant in &enum_variants {
            let pattern = format!("{}::{}", enum_name, variant);
            let pattern_normalized = pattern.replace(" ", "");

            let exists = existing_patterns.iter().any(|p| {
                p.replace(" ", "") == pattern_normalized
            });

            if !exists {
                missing_variants.push(variant.clone());
            }
        }

        if missing_variants.is_empty() {
            println!("All enum variants already covered in match expressions");
            return Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            });
        }

        // Get backup of function before modification
        let backup_node = if let Some(ref fn_name) = op.function_name {
            self.get_function_backup(fn_name)?
        } else {
            BackupNode {
                node_type: "Unknown".to_string(),
                identifier: "match_expression".to_string(),
                original_content: String::new(),
                location: NodeLocation {
                    line: 0,
                    column: 0,
                    end_line: 0,
                    end_column: 0,
                },
            }
        };

        // Add ALL missing match arms in one pass using a visitor
        let mut arms_to_add = Vec::new();
        for variant in &missing_variants {
            let pattern = format!("{}::{}", enum_name, variant);
            let dummy_match = format!("match () {{ {} => {}, }}", pattern, op.body);
            let expr: syn::Expr = parse_str(&dummy_match)
                .with_context(|| format!("Failed to parse pattern/body: {} => {}", pattern, op.body))?;

            if let syn::Expr::Match(match_expr) = expr {
                if let Some(arm) = match_expr.arms.into_iter().next() {
                    arms_to_add.push((pattern.clone(), arm));
                }
            }
        }

        // Find and modify match expressions with all arms at once
        let mut visitor = MultiMatchArmAdder {
            target_function: op.function_name.clone(),
            arms_to_add,
            modified: false,
            current_function: None,
            modified_function: None,
        };

        visitor.visit_file_mut(&mut self.syntax_tree);

        if visitor.modified {
            // Print what was added
            for variant in &missing_variants {
                println!("Added match arm for: {}::{}", enum_name, variant);
            }

            // Replace just the modified function
            self.replace_modified_functions(&visitor.modified_function)?;
            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            })
        } else {
            Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            })
        }
    }

    fn find_enum_variants(&self, enum_name: &str) -> Result<Vec<String>> {
        // Find the enum in the syntax tree
        for item in &self.syntax_tree.items {
            if let Item::Enum(e) = item {
                if e.ident == enum_name {
                    let variants: Vec<String> = e.variants.iter()
                        .map(|v| v.ident.to_string())
                        .collect();
                    return Ok(variants);
                }
            }
        }

        Ok(Vec::new())
    }

    fn find_existing_match_patterns(&self, function_name: &Option<String>) -> Vec<String> {
        use syn::visit::Visit;

        struct PatternCollector {
            target_function: Option<String>,
            current_function: Option<String>,
            patterns: Vec<String>,
        }

        impl<'ast> Visit<'ast> for PatternCollector {
            fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
                let prev_fn = self.current_function.clone();
                self.current_function = Some(node.sig.ident.to_string());
                syn::visit::visit_item_fn(self, node);
                self.current_function = prev_fn;
            }

            fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
                // Check if we're in the right function (if specified)
                if let Some(ref target) = self.target_function {
                    if self.current_function.as_ref() != Some(target) {
                        syn::visit::visit_expr_match(self, node);
                        return;
                    }
                }

                // Collect all patterns
                for arm in &node.arms {
                    self.patterns.push(arm.pat.to_token_stream().to_string());
                }

                syn::visit::visit_expr_match(self, node);
            }
        }

        let mut collector = PatternCollector {
            target_function: function_name.clone(),
            current_function: None,
            patterns: Vec::new(),
        };

        collector.visit_file(&self.syntax_tree);
        collector.patterns
    }

    pub(crate) fn update_match_arm(&mut self, op: &UpdateMatchArmOp) -> Result<ModificationResult> {
        // Get backup of function before modification
        let backup_node = if let Some(ref fn_name) = op.function_name {
            self.get_function_backup(fn_name)?
        } else {
            BackupNode {
                node_type: "Unknown".to_string(),
                identifier: "match_expression".to_string(),
                original_content: String::new(),
                location: NodeLocation {
                    line: 0,
                    column: 0,
                    end_line: 0,
                    end_column: 0,
                },
            }
        };

        // Parse the new body
        let new_body: syn::Expr = parse_str(&op.new_body)
            .with_context(|| format!("Failed to parse new body: {}", op.new_body))?;

        // Find and modify match expressions
        let mut visitor = MatchArmUpdater {
            target_function: op.function_name.clone(),
            pattern_to_match: op.pattern.clone(),
            new_body,
            modified: false,
            current_function: None,
            modified_function: None,
        };

        visitor.visit_file_mut(&mut self.syntax_tree);

        if visitor.modified {
            // Replace just the modified function
            self.replace_modified_functions(&visitor.modified_function)?;
            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            })
        } else {
            anyhow::bail!("Pattern '{}' not found in any match expression", op.pattern)
        }
    }

    pub(crate) fn remove_match_arm(&mut self, op: &RemoveMatchArmOp) -> Result<ModificationResult> {
        // Get backup of function before modification
        let backup_node = if let Some(ref fn_name) = op.function_name {
            self.get_function_backup(fn_name)?
        } else {
            BackupNode {
                node_type: "Unknown".to_string(),
                identifier: "match_expression".to_string(),
                original_content: String::new(),
                location: NodeLocation {
                    line: 0,
                    column: 0,
                    end_line: 0,
                    end_column: 0,
                },
            }
        };

        // Find and modify match expressions
        let mut visitor = MatchArmRemover {
            target_function: op.function_name.clone(),
            pattern_to_remove: op.pattern.clone(),
            modified: false,
            current_function: None,
            modified_function: None,
        };

        visitor.visit_file_mut(&mut self.syntax_tree);

        if visitor.modified {
            // Replace just the modified function
            self.replace_modified_functions(&visitor.modified_function)?;
            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![backup_node],
            })
        } else {
            anyhow::bail!("Pattern '{}' not found in any match expression", op.pattern)
        }
    }

    pub(crate) fn add_impl_method(&mut self, op: &AddImplMethodOp) -> Result<ModificationResult> {
        // Parse the method definition
        let method_code = format!("impl Dummy {{ {} }}", op.method_def);
        let dummy: syn::ItemImpl = parse_str(&method_code)
            .context("Failed to parse method definition")?;

        let new_method = dummy.items.first()
            .context("No method found in definition")?
            .clone();

        // Get the method name for idempotency check
        let method_name = match &new_method {
            syn::ImplItem::Fn(f) => f.sig.ident.to_string(),
            _ => anyhow::bail!("Only method definitions are supported"),
        };

        // Find the impl block
        let impl_index = self.syntax_tree.items.iter().position(|item| {
            if let Item::Impl(impl_block) = item {
                // Check if this is the right impl block
                if let syn::Type::Path(type_path) = &*impl_block.self_ty {
                    if let Some(segment) = type_path.path.segments.last() {
                        return segment.ident == op.target;
                    }
                }
            }
            false
        }).ok_or_else(|| anyhow::anyhow!("impl block for '{}' not found", op.target))?;

        // Check if method already exists (idempotent)
        let impl_block = match &self.syntax_tree.items[impl_index] {
            Item::Impl(i) => i,
            _ => unreachable!(),
        };

        let method_exists = impl_block.items.iter().any(|item| {
            if let syn::ImplItem::Fn(f) = item {
                f.sig.ident == method_name
            } else {
                false
            }
        });

        if method_exists {
            return Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            });
        }

        // Create backup of original impl block before modification
        let backup_node = BackupNode {
            node_type: "ItemImpl".to_string(),
            identifier: op.target.clone(),
            original_content: self.unparse_item(&self.syntax_tree.items[impl_index].clone()),
            location: self.span_to_location(impl_block.span()),
        };

        // Get the span before modification
        let impl_span = impl_block.span();

        // Add the method to the impl block
        match &mut self.syntax_tree.items[impl_index] {
            Item::Impl(impl_block) => {
                // Add based on position
                match &op.position {
                    InsertPosition::First => {
                        impl_block.items.insert(0, new_method);
                    }
                    InsertPosition::Last => {
                        impl_block.items.push(new_method);
                    }
                    InsertPosition::After(name) => {
                        let pos = impl_block.items.iter().position(|item| {
                            if let syn::ImplItem::Fn(f) = item {
                                f.sig.ident == name
                            } else {
                                false
                            }
                        }).with_context(|| format!("Method '{}' not found", name))?;
                        impl_block.items.insert(pos + 1, new_method);
                    }
                    InsertPosition::Before(name) => {
                        let pos = impl_block.items.iter().position(|item| {
                            if let syn::ImplItem::Fn(f) = item {
                                f.sig.ident == name
                            } else {
                                false
                            }
                        }).with_context(|| format!("Method '{}' not found", name))?;
                        impl_block.items.insert(pos, new_method);
                    }
                }
            }
            _ => unreachable!(),
        }

        // Use prettyplease to format just this impl block
        self.replace_formatted_item(impl_index, impl_span)?;

        Ok(ModificationResult {
            changed: true,
            modified_nodes: vec![backup_node],
        })
    }

    pub(crate) fn add_use_statement(&mut self, op: &AddUseStatementOp) -> Result<ModificationResult> {
        // Parse the use statement
        let use_code = format!("use {};", op.use_path);
        let use_item: syn::ItemUse = parse_str(&use_code)
            .context("Failed to parse use statement")?;

        // Check if this use statement already exists (idempotent)
        let use_exists = self.syntax_tree.items.iter().any(|item| {
            if let Item::Use(existing_use) = item {
                // Compare the use trees
                existing_use.tree.to_token_stream().to_string() ==
                    use_item.tree.to_token_stream().to_string()
            } else {
                false
            }
        });

        if use_exists {
            return Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            });
        }

        // Create a simple backup for use statements (track by line position)
        let backup_node = BackupNode {
            node_type: "ItemUse".to_string(),
            identifier: op.use_path.clone(),
            original_content: format!("use {};", op.use_path),
            location: NodeLocation {
                line: 0,
                column: 0,
                end_line: 0,
                end_column: 0,
            },
        };

        // Find the position to insert the use statement
        let insert_index = match &op.position {
            InsertPosition::First => 0,
            InsertPosition::Last => {
                // Find the last use statement
                self.syntax_tree.items.iter()
                    .rposition(|item| matches!(item, Item::Use(_)))
                    .map(|i| i + 1)
                    .unwrap_or(0)
            }
            InsertPosition::After(path) => {
                // Find the use statement matching the path
                let pos = self.syntax_tree.items.iter().position(|item| {
                    if let Item::Use(u) = item {
                        u.tree.to_token_stream().to_string().contains(path)
                    } else {
                        false
                    }
                }).with_context(|| format!("Use statement for '{}' not found", path))?;
                pos + 1
            }
            InsertPosition::Before(path) => {
                // Find the use statement matching the path
                self.syntax_tree.items.iter().position(|item| {
                    if let Item::Use(u) = item {
                        u.tree.to_token_stream().to_string().contains(path)
                    } else {
                        false
                    }
                }).with_context(|| format!("Use statement for '{}' not found", path))?
            }
        };

        // Insert the use statement into the AST
        self.syntax_tree.items.insert(insert_index, Item::Use(use_item));

        // Find the byte position in the source where we need to insert
        // We want to insert at the beginning of a line
        let insert_line_pos = if insert_index == 0 {
            // Insert at very beginning
            0
        } else {
            // Insert after the previous item
            let prev_item = &self.syntax_tree.items[insert_index - 1];
            let span = prev_item.span();
            let end_pos = self.span_to_byte_offset(span.end());

            // Find the end of this line (where the newline is)
            let mut line_end = end_pos;
            while line_end < self.content.len() && self.content.as_bytes()[line_end] != b'\n' {
                line_end += 1;
            }
            // Move past the newline to the start of the next line
            if line_end < self.content.len() {
                line_end + 1
            } else {
                // At end of file, add a newline first
                self.content.push('\n');
                self.content.len()
            }
        };

        // Format the use statement
        let use_str = format!("use {};\n", op.use_path);

        // Insert the use statement
        self.content.insert_str(insert_line_pos, &use_str);

        Ok(ModificationResult {
            changed: true,
            modified_nodes: vec![backup_node],
        })
    }

    pub(crate) fn add_derive(&mut self, op: &AddDeriveOp) -> Result<ModificationResult> {
        // Find the target item (struct or enum)
        let item_index = self.syntax_tree.items.iter().position(|item| {
            match (&op.target_type as &str, item) {
                ("struct", Item::Struct(s)) => s.ident == op.target_name,
                ("enum", Item::Enum(e)) => e.ident == op.target_name,
                _ => false,
            }
        }).ok_or_else(|| anyhow::anyhow!("{} '{}' not found", op.target_type, op.target_name))?;

        // Get the item and check for existing derives
        let (existing_derives, item_span, item_attrs) = match &self.syntax_tree.items[item_index] {
            Item::Struct(s) => (Self::extract_derives(&s.attrs), s.span(), &s.attrs),
            Item::Enum(e) => (Self::extract_derives(&e.attrs), e.span(), &e.attrs),
            _ => (Vec::new(), proc_macro2::Span::call_site(), &Vec::new() as &Vec<syn::Attribute>),
        };

        // Check if the item matches the where filter (if specified)
        if let Some(ref where_filter) = op.where_filter {
            if !self.matches_where_filter(item_attrs, where_filter)? {
                // Item doesn't match filter - skip without error
                return Ok(ModificationResult {
                    changed: false,
                    modified_nodes: vec![],
                });
            }
        }

        // Create backup of original item before modification
        let backup_node = BackupNode {
            node_type: if op.target_type == "struct" { "ItemStruct" } else { "ItemEnum" }.to_string(),
            identifier: op.target_name.clone(),
            original_content: self.unparse_item(&self.syntax_tree.items[item_index].clone()),
            location: self.span_to_location(item_span),
        };

        // Filter out derives that already exist (idempotent)
        let new_derives: Vec<String> = op.derives.iter()
            .filter(|d| !existing_derives.contains(&d.to_string()))
            .cloned()
            .collect();

        if new_derives.is_empty() {
            // All derives already exist
            return Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            });
        }

        // Combine existing and new derives
        let mut all_derives = existing_derives;
        all_derives.extend(new_derives);

        // Convert to string refs for the update function
        let all_derives_refs: Vec<&str> = all_derives.iter().map(|s| s.as_str()).collect();

        // Update the AST item's attributes
        match &mut self.syntax_tree.items[item_index] {
            Item::Struct(s) => {
                Self::update_derive_attr(&mut s.attrs, &all_derives_refs)?;
            }
            Item::Enum(e) => {
                Self::update_derive_attr(&mut e.attrs, &all_derives_refs)?;
            }
            _ => unreachable!(),
        }

        // Use prettyplease to format just this item
        self.replace_formatted_item(item_index, item_span)?;

        Ok(ModificationResult {
            changed: true,
            modified_nodes: vec![backup_node],
        })
    }

    /// Replace an item in the content with a formatted version
    fn replace_formatted_item(&mut self, item_index: usize, original_span: Span) -> Result<()> {
        // Get the item start and end positions from the original source
        let item_start_pos = self.span_to_byte_offset(original_span.start());
        let item_end_pos = self.span_to_byte_offset(original_span.end());

        // Find the actual start (including attributes)
        let mut actual_start = item_start_pos;

        // Search backwards for attributes
        let mut temp_pos = item_start_pos;
        while temp_pos > 0 {
            // Move to previous line
            temp_pos = temp_pos.saturating_sub(1);
            let mut line_start = temp_pos;
            while line_start > 0 && self.content.as_bytes()[line_start - 1] != b'\n' {
                line_start -= 1;
            }

            let line = if temp_pos < self.content.len() {
                &self.content[line_start..temp_pos + 1]
            } else {
                &self.content[line_start..]
            };
            let trimmed = line.trim();

            if trimmed.starts_with("#[") {
                actual_start = line_start;
                temp_pos = line_start;
            } else if trimmed.is_empty() {
                temp_pos = line_start;
            } else {
                break;
            }

            if line_start == 0 {
                break;
            }
        }

        // Create a temporary file with just this item for pretty formatting
        let item_clone = self.syntax_tree.items[item_index].clone();
        let temp_file = syn::File {
            shebang: None,
            attrs: Vec::new(),
            items: vec![item_clone],
        };

        // Format the item using prettyplease
        let formatted = prettyplease::unparse(&temp_file);
        let formatted = formatted.trim();

        // Replace in content
        self.content.replace_range(actual_start..item_end_pos, formatted);

        Ok(())
    }

    /// Extract existing derive traits from attributes - returns owned Strings
    fn extract_derives(attrs: &[syn::Attribute]) -> Vec<String> {
        for attr in attrs {
            if attr.path().is_ident("derive") {
                if let Ok(syn::Meta::List(meta_list)) = attr.meta.clone().try_into() {
                    let tokens_str = meta_list.tokens.to_string();
                    return tokens_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
            }
        }
        Vec::new()
    }

    /// Check if an item matches the where filter criteria
    /// Supports filters like:
    /// - "derives_trait:Clone" - matches if item derives Clone
    /// - "derives_trait:Clone,Debug" - matches if item derives Clone OR Debug
    fn matches_where_filter(&self, attrs: &[syn::Attribute], where_filter: &str) -> Result<bool> {
        // Parse the filter: "derives_trait:Clone,Debug"
        if let Some(filter_value) = where_filter.strip_prefix("derives_trait:") {
            let required_traits: Vec<&str> = filter_value.split(',').map(|s| s.trim()).collect();
            let existing_derives = Self::extract_derives(attrs);

            // Check if ANY of the required traits are present
            for required_trait in required_traits {
                if existing_derives.iter().any(|d| d == required_trait) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        // Unknown filter type - default to match (don't break existing behavior)
        Ok(true)
    }

    /// Update or create derive attribute in the attribute list
    fn update_derive_attr(attrs: &mut Vec<syn::Attribute>, derives: &[&str]) -> Result<()> {
        let derive_str = derives.join(", ");

        // Parse a dummy struct with the derive to extract the attribute
        let dummy = format!("#[derive({})]\nstruct Dummy;", derive_str);
        let parsed: syn::ItemStruct = parse_str(&dummy)
            .context("Failed to parse derive attribute")?;

        let new_attr = parsed.attrs.into_iter()
            .find(|a| a.path().is_ident("derive"))
            .context("Failed to extract derive attribute")?;

        // Find existing derive attribute and replace it
        if let Some(pos) = attrs.iter().position(|a| a.path().is_ident("derive")) {
            attrs[pos] = new_attr;
        } else {
            // Add new derive attribute at the beginning
            attrs.insert(0, new_attr);
        }

        Ok(())
    }

    /// Replace the modified function(s) in the content with formatted versions
    fn replace_modified_functions(&mut self, modified_function: &Option<String>) -> Result<()> {
        // If no specific function was targeted, format the entire file
        if modified_function.is_none() {
            self.content = prettyplease::unparse(&self.syntax_tree);
            return Ok(());
        }

        // Parse the ORIGINAL content to get the correct spans
        let original_syntax_tree: File = syn::parse_str(&self.content)
            .context("Failed to re-parse original content")?;

        let function_name = modified_function.as_ref().unwrap();

        // Find the function in the ORIGINAL syntax tree to get correct byte positions
        let original_fn = original_syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Fn(f) = item {
                    if f.sig.ident == function_name {
                        return Some(f.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Function '{}' not found in original", function_name))?;

        // Get the span of the original function (these are the correct byte positions)
        let start = self.span_to_byte_offset(original_fn.span().start());
        let end = self.span_to_byte_offset(original_fn.span().end());

        // Find the MODIFIED function in the modified syntax tree
        let modified_fn = self.syntax_tree.items.iter()
            .find_map(|item| {
                if let Item::Fn(f) = item {
                    if f.sig.ident == function_name {
                        return Some(f.clone());
                    }
                }
                None
            })
            .ok_or_else(|| anyhow::anyhow!("Function '{}' not found in modified AST", function_name))?;

        // Format just the modified function using prettyplease
        let dummy_file = syn::File {
            shebang: None,
            attrs: Vec::new(),
            items: vec![Item::Fn(modified_fn)],
        };

        let formatted_fn = prettyplease::unparse(&dummy_file);

        // Extract just the function (remove any extra newlines at start/end)
        let formatted_fn = formatted_fn.trim();

        // Replace the function in the original content using original spans
        self.content.replace_range(start..end, formatted_fn);

        Ok(())
    }
    
    fn span_to_byte_offset(&self, pos: LineColumn) -> usize {
        let line_idx = pos.line.saturating_sub(1);
        if line_idx < self.line_offsets.len() {
            self.line_offsets[line_idx] + pos.column
        } else {
            self.content.len()
        }
    }
    
    fn find_after_field_end(&self, pos: usize) -> usize {
        // Look for comma or newline after the field
        let mut i = pos;
        while i < self.content.len() {
            match self.content.as_bytes()[i] as char {
                ',' => return i + 1,
                '\n' => return i + 1,
                _ => i += 1,
            }
        }
        pos
    }
    
    fn get_indentation(&self, pos: usize) -> String {
        // Find the start of the current line
        let mut line_start = pos;
        while line_start > 0 && self.content.as_bytes()[line_start - 1] != b'\n' {
            line_start -= 1;
        }
        
        // Count spaces/tabs at the start of the line
        let mut indent = String::new();
        let mut i = line_start;
        while i < self.content.len() {
            match self.content.as_bytes()[i] as char {
                ' ' | '\t' => {
                    indent.push(self.content.as_bytes()[i] as char);
                    i += 1;
                }
                _ => break,
            }
        }
        
        // If we're inserting in an empty struct/enum, add default indentation
        if indent.is_empty() {
            "    ".to_string()
        } else {
            indent
        }
    }
    
    pub fn to_string(&self) -> String {
        self.content.clone()
    }

    /// Inspect and list AST nodes (e.g., struct literals) in the file
    pub(crate) fn inspect(&self, node_type: &str, name_filter: Option<&str>, include_comments: bool) -> Result<Vec<crate::operations::InspectResult>> {
        use syn::visit::Visit;
        use crate::operations::InspectResult;

        let mut results = Vec::new();

        match node_type {
            "struct-literal" => {
                // Find all struct literal expressions
                struct StructLiteralVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for StructLiteralVisitor<'a> {
                    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
                        // Match based on pattern:
                        // - "Rectangle" → only Rectangle { ... } (no :: prefix)
                        // - "*::Rectangle" → any path ending with Rectangle (View::Rectangle, etc.)
                        // - "View::Rectangle" → exact match only View::Rectangle

                        let filter = match self.name_filter {
                            Some(f) => f,
                            None => {
                                // No filter - match anything
                                let struct_name = node.path.segments.last()
                                    .map(|seg| seg.ident.to_string())
                                    .unwrap_or_default();

                                let snippet = self.editor.format_expr_struct(node);
                                let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                                // Extract preceding comment if requested
                                let preceding_comment = if self.include_comments {
                                    extract_preceding_comment(&self.editor.content, location.line)
                                } else {
                                    None
                                };

                                self.results.push(InspectResult {
                                    file_path: String::new(),
                                    node_type: "ExprStruct".to_string(),
                                    identifier: struct_name,
                                    location,
                                    snippet,
                                    preceding_comment,
                                });

                                syn::visit::visit_expr_struct(self, node);
                                return;
                            }
                        };

                        // Check if this struct literal matches the filter pattern
                        let matches = if filter.contains("::") {
                            // Pattern contains :: - check for exact or wildcard match
                            if filter.starts_with("*::") {
                                // Wildcard: *::Rectangle matches any path ending with Rectangle
                                let target_name = &filter[3..]; // Skip "*::"
                                node.path.segments.last()
                                    .map(|seg| seg.ident.to_string() == target_name)
                                    .unwrap_or(false)
                            } else {
                                // Exact path match: View::Rectangle
                                let path_str = node.path.segments.iter()
                                    .map(|seg| seg.ident.to_string())
                                    .collect::<Vec<_>>()
                                    .join("::");
                                path_str == filter
                            }
                        } else {
                            // No :: in pattern - only match pure struct literals (no path qualifier)
                            node.path.get_ident()
                                .map(|ident| ident.to_string() == filter)
                                .unwrap_or(false)
                        };

                        if !matches {
                            syn::visit::visit_expr_struct(self, node);
                            return;
                        }

                        // Get the struct name for the identifier
                        let struct_name = node.path.segments.last()
                            .map(|seg| seg.ident.to_string())
                            .unwrap_or_default();

                        // Format the struct literal
                        let snippet = self.editor.format_expr_struct(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "ExprStruct".to_string(),
                            identifier: struct_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_struct(self, node);
                    }
                }

                let mut visitor = StructLiteralVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "match-arm" => {
                // Find all match arms
                struct MatchArmVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    pattern_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for MatchArmVisitor<'a> {
                    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
                        // Iterate through all arms in this match expression
                        for arm in &node.arms {
                            // Convert pattern to string for matching
                            let pat = &arm.pat;
                            let pattern_str = quote::quote!(#pat).to_string();

                            // Apply pattern filter if specified
                            if let Some(filter) = self.pattern_filter {
                                // Normalize both for comparison (remove spaces)
                                let normalized_pattern = pattern_str.replace(" ", "");
                                let normalized_filter = filter.replace(" ", "");

                                if !normalized_pattern.contains(&normalized_filter) {
                                    continue;
                                }
                            }

                            // Format the match arm (pattern => body)
                            let snippet = self.editor.format_match_arm(arm);
                            let location = self.editor.span_to_location(arm.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                            // Extract preceding comment if requested
                            let preceding_comment = if self.include_comments {
                                extract_preceding_comment(&self.editor.content, location.line)
                            } else {
                                None
                            };

                            self.results.push(InspectResult {
                                file_path: String::new(), // Will be filled in by caller
                                node_type: "MatchArm".to_string(),
                                identifier: pattern_str.replace(" ", ""),
                                location,
                                snippet,
                                preceding_comment,
                            });
                        }

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_match(self, node);
                    }
                }

                let mut visitor = MatchArmVisitor {
                    results: &mut results,
                    pattern_filter: name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "enum-usage" => {
                // Find all enum variant usages (paths like Operator::Error)
                struct EnumUsageVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    path_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for EnumUsageVisitor<'a> {
                    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
                        // Convert path to string
                        let path = &node.path;
                        let path_str = quote::quote!(#path).to_string();

                        // Apply path filter if specified
                        if let Some(filter) = self.path_filter {
                            // Normalize both for comparison (remove spaces)
                            let normalized_path = path_str.replace(" ", "");
                            let normalized_filter = filter.replace(" ", "");

                            if !normalized_path.contains(&normalized_filter) {
                                syn::visit::visit_expr_path(self, node);
                                return;
                            }
                        }

                        // Format the path expression
                        let snippet = self.editor.format_expr_path(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "ExprPath".to_string(),
                            identifier: path_str.replace(" ", ""),
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_path(self, node);
                    }
                }

                let mut visitor = EnumUsageVisitor {
                    results: &mut results,
                    path_filter: name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "function-call" => {
                // Find all function call expressions
                struct FunctionCallVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for FunctionCallVisitor<'a> {
                    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
                        // Extract function name from the call expression
                        let func_name = if let syn::Expr::Path(expr_path) = &*node.func {
                            // Get the last segment of the path as the function name
                            expr_path.path.segments.last()
                                .map(|seg| seg.ident.to_string())
                                .unwrap_or_default()
                        } else {
                            // For other expression types, use quote to convert to string
                            quote::quote!(#node.func).to_string()
                        };

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if func_name != filter {
                                syn::visit::visit_expr_call(self, node);
                                return;
                            }
                        }

                        // Format the function call
                        let snippet = self.editor.format_expr_call(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "ExprCall".to_string(),
                            identifier: func_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_call(self, node);
                    }
                }

                let mut visitor = FunctionCallVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "method-call" => {
                // Find all method call expressions
                struct MethodCallVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for MethodCallVisitor<'a> {
                    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
                        // Extract method name
                        let method_name = node.method.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if method_name != filter {
                                syn::visit::visit_expr_method_call(self, node);
                                return;
                            }
                        }

                        // Format the method call
                        let snippet = self.editor.format_expr_method_call(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "ExprMethodCall".to_string(),
                            identifier: method_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_method_call(self, node);
                    }
                }

                let mut visitor = MethodCallVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "identifier" => {
                // Find all identifier references
                struct IdentifierVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for IdentifierVisitor<'a> {
                    fn visit_ident(&mut self, node: &'ast syn::Ident) {
                        // Extract identifier name
                        let ident_name = node.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if ident_name != filter {
                                syn::visit::visit_ident(self, node);
                                return;
                            }
                        }

                        // Format the identifier
                        let snippet = self.editor.format_ident(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "Ident".to_string(),
                            identifier: ident_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting
                        syn::visit::visit_ident(self, node);
                    }
                }

                let mut visitor = IdentifierVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "type-ref" => {
                // Find all type path usages
                struct TypeRefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for TypeRefVisitor<'a> {
                    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
                        // Extract type name (last segment of path)
                        let type_name = node.path.segments.last()
                            .map(|seg| seg.ident.to_string())
                            .unwrap_or_default();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if type_name != filter {
                                syn::visit::visit_type_path(self, node);
                                return;
                            }
                        }

                        // Format the type path
                        let snippet = self.editor.format_type_path(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        // Get full path for identifier
                        let path = &node.path;
                        let path_str = quote::quote!(#path).to_string();

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "TypePath".to_string(),
                            identifier: path_str.replace(" ", ""),
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting
                        syn::visit::visit_type_path(self, node);
                    }
                }

                let mut visitor = TypeRefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "macro-call" => {
                // Find all macro call expressions
                struct MacroCallVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for MacroCallVisitor<'a> {
                    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
                        // Extract macro name from the path
                        let macro_name = node.mac.path.segments.last()
                            .map(|seg| seg.ident.to_string())
                            .unwrap_or_default();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if macro_name != filter {
                                syn::visit::visit_expr_macro(self, node);
                                return;
                            }
                        }

                        // Format the macro call
                        let snippet = self.editor.format_expr_macro(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(), // Will be filled in by caller
                            node_type: "ExprMacro".to_string(),
                            identifier: macro_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        // Continue visiting nested expressions
                        syn::visit::visit_expr_macro(self, node);
                    }

                    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
                        // Also catch macro calls at statement level (e.g., println! as statement)
                        if let syn::Stmt::Macro(macro_stmt) = node {
                            let macro_name = macro_stmt.mac.path.segments.last()
                                .map(|seg| seg.ident.to_string())
                                .unwrap_or_default();

                            // Apply name filter if specified
                            if let Some(filter) = self.name_filter {
                                if macro_name != filter {
                                    syn::visit::visit_stmt(self, node);
                                    return;
                                }
                            }

                            // Format the macro call
                            let snippet = self.editor.format_stmt_macro(macro_stmt);
                            let location = self.editor.span_to_location(macro_stmt.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                            self.results.push(InspectResult {
                                file_path: String::new(), // Will be filled in by caller
                                node_type: "StmtMacro".to_string(),
                                identifier: macro_name,
                                location,
                                snippet,
                                preceding_comment,
                            });
                        }

                        // Continue visiting
                        syn::visit::visit_stmt(self, node);
                    }
                }

                let mut visitor = MacroCallVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                // Visit all items in the file
                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "struct" => {
                // Find all struct definitions
                struct StructDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for StructDefVisitor<'a> {
                    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
                        let struct_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if struct_name != filter {
                                syn::visit::visit_item_struct(self, node);
                                return;
                            }
                        }

                        // Format the struct definition
                        let snippet = self.editor.format_item_struct(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemStruct".to_string(),
                            identifier: struct_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_struct(self, node);
                    }
                }

                let mut visitor = StructDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "enum" => {
                // Find all enum definitions
                struct EnumDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for EnumDefVisitor<'a> {
                    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
                        let enum_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if enum_name != filter {
                                syn::visit::visit_item_enum(self, node);
                                return;
                            }
                        }

                        // Format the enum definition
                        let snippet = self.editor.format_item_enum(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemEnum".to_string(),
                            identifier: enum_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_enum(self, node);
                    }
                }

                let mut visitor = EnumDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "function" => {
                // Find all function definitions
                struct FunctionDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for FunctionDefVisitor<'a> {
                    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
                        let fn_name = node.sig.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if fn_name != filter {
                                syn::visit::visit_item_fn(self, node);
                                return;
                            }
                        }

                        // Format the function definition
                        let snippet = self.editor.format_item_fn(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemFn".to_string(),
                            identifier: fn_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_fn(self, node);
                    }
                }

                let mut visitor = FunctionDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "impl-method" => {
                // Find methods within impl blocks
                struct ImplMethodVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                    current_impl_type: Option<String>,
                }

                impl<'ast, 'a> Visit<'ast> for ImplMethodVisitor<'a> {
                    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
                        // Track which type this impl is for
                        let impl_type = if let syn::Type::Path(type_path) = &*node.self_ty {
                            type_path.path.segments.last()
                                .map(|seg| seg.ident.to_string())
                        } else {
                            None
                        };

                        let prev_impl_type = self.current_impl_type.clone();
                        self.current_impl_type = impl_type;

                        syn::visit::visit_item_impl(self, node);

                        self.current_impl_type = prev_impl_type;
                    }

                    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
                        let method_name = node.sig.ident.to_string();

                        // Create identifier with impl type context if available
                        let identifier = if let Some(ref impl_type) = self.current_impl_type {
                            format!("{}::{}", impl_type, method_name)
                        } else {
                            method_name.clone()
                        };

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            // Support filtering by "Type::method" or just "method"
                            if !identifier.contains(filter) && method_name != filter {
                                syn::visit::visit_impl_item_fn(self, node);
                                return;
                            }
                        }

                        // Format the method definition
                        let snippet = self.editor.format_impl_item_fn(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ImplItemFn".to_string(),
                            identifier,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_impl_item_fn(self, node);
                    }
                }

                let mut visitor = ImplMethodVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                    current_impl_type: None,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "trait" => {
                // Find all trait definitions
                struct TraitDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for TraitDefVisitor<'a> {
                    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
                        let trait_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if trait_name != filter {
                                syn::visit::visit_item_trait(self, node);
                                return;
                            }
                        }

                        // Format the trait definition
                        let snippet = self.editor.format_item_trait(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemTrait".to_string(),
                            identifier: trait_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_trait(self, node);
                    }
                }

                let mut visitor = TraitDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "const" => {
                // Find all const definitions
                struct ConstDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for ConstDefVisitor<'a> {
                    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
                        let const_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if const_name != filter {
                                syn::visit::visit_item_const(self, node);
                                return;
                            }
                        }

                        // Format the const definition
                        let snippet = self.editor.format_item_const(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemConst".to_string(),
                            identifier: const_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_const(self, node);
                    }
                }

                let mut visitor = ConstDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "static" => {
                // Find all static definitions
                struct StaticDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for StaticDefVisitor<'a> {
                    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
                        let static_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if static_name != filter {
                                syn::visit::visit_item_static(self, node);
                                return;
                            }
                        }

                        // Format the static definition
                        let snippet = self.editor.format_item_static(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemStatic".to_string(),
                            identifier: static_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_static(self, node);
                    }
                }

                let mut visitor = StaticDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "type-alias" => {
                // Find all type alias definitions
                struct TypeAliasVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for TypeAliasVisitor<'a> {
                    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
                        let type_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if type_name != filter {
                                syn::visit::visit_item_type(self, node);
                                return;
                            }
                        }

                        // Format the type alias definition
                        let snippet = self.editor.format_item_type(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemType".to_string(),
                            identifier: type_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_type(self, node);
                    }
                }

                let mut visitor = TypeAliasVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            "mod" => {
                // Find all module definitions
                struct ModDefVisitor<'a> {
                    results: &'a mut Vec<InspectResult>,
                    name_filter: Option<&'a str>,
                    editor: &'a RustEditor,
                    include_comments: bool,
                }

                impl<'ast, 'a> Visit<'ast> for ModDefVisitor<'a> {
                    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
                        let mod_name = node.ident.to_string();

                        // Apply name filter if specified
                        if let Some(filter) = self.name_filter {
                            if mod_name != filter {
                                syn::visit::visit_item_mod(self, node);
                                return;
                            }
                        }

                        // Format the module definition
                        let snippet = self.editor.format_item_mod(node);
                        let location = self.editor.span_to_location(node.span());

                        // Extract preceding comment if requested
                        let preceding_comment = if self.include_comments {
                            extract_preceding_comment(&self.editor.content, location.line)
                        } else {
                            None
                        };

                        self.results.push(InspectResult {
                            file_path: String::new(),
                            node_type: "ItemMod".to_string(),
                            identifier: mod_name,
                            location,
                            snippet,
                            preceding_comment,
                        });

                        syn::visit::visit_item_mod(self, node);
                    }
                }

                let mut visitor = ModDefVisitor {
                    results: &mut results,
                    name_filter,
                    editor: self,
                    include_comments,
                };

                for item in &self.syntax_tree.items {
                    syn::visit::visit_item(&mut visitor, item);
                }
            }
            _ => anyhow::bail!("Unsupported node type: {}", node_type),
        }

        Ok(results)
    }

    /// Format an ExprStruct node as a string - extracts original source
    fn format_expr_struct(&self, expr: &syn::ExprStruct) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(expr.span().start());
        let end = self.span_to_byte_offset(expr.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format a match arm as a string - extracts original source
    fn format_match_arm(&self, arm: &syn::Arm) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(arm.span().start());
        let end = self.span_to_byte_offset(arm.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an ExprPath node as a string - extracts original source
    fn format_expr_path(&self, expr: &syn::ExprPath) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(expr.span().start());
        let end = self.span_to_byte_offset(expr.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an ExprCall node as a string - extracts original source
    fn format_expr_call(&self, expr: &syn::ExprCall) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(expr.span().start());
        let end = self.span_to_byte_offset(expr.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an ExprMethodCall node as a string - extracts original source
    fn format_expr_method_call(&self, expr: &syn::ExprMethodCall) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(expr.span().start());
        let end = self.span_to_byte_offset(expr.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an Ident node as a string - just return the identifier
    fn format_ident(&self, ident: &syn::Ident) -> String {
        ident.to_string()
    }

    /// Format a TypePath node as a string - extracts original source
    fn format_type_path(&self, ty: &syn::TypePath) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(ty.span().start());
        let end = self.span_to_byte_offset(ty.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an ExprMacro node as a string - extracts original source
    fn format_expr_macro(&self, expr: &syn::ExprMacro) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(expr.span().start());
        let end = self.span_to_byte_offset(expr.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format a StmtMacro node as a string - extracts original source
    fn format_stmt_macro(&self, stmt: &syn::StmtMacro) -> String {
        // Extract the original source code from the file content using the span
        let start = self.span_to_byte_offset(stmt.span().start());
        let end = self.span_to_byte_offset(stmt.span().end());

        // Get the original text and collapse to single line
        let original = &self.content[start..end];

        // Replace multiple whitespace/newlines with single space for single-line format
        original.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Format an ItemStruct node as a string - extracts original source
    fn format_item_struct(&self, item: &syn::ItemStruct) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemEnum node as a string - extracts original source
    fn format_item_enum(&self, item: &syn::ItemEnum) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemFn node as a string - extracts original source
    fn format_item_fn(&self, item: &syn::ItemFn) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ImplItemFn node as a string - extracts original source
    fn format_impl_item_fn(&self, item: &syn::ImplItemFn) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemTrait node as a string - extracts original source
    fn format_item_trait(&self, item: &syn::ItemTrait) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemConst node as a string - extracts original source
    fn format_item_const(&self, item: &syn::ItemConst) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemStatic node as a string - extracts original source
    fn format_item_static(&self, item: &syn::ItemStatic) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemType node as a string - extracts original source
    fn format_item_type(&self, item: &syn::ItemType) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Format an ItemMod node as a string - extracts original source
    fn format_item_mod(&self, item: &syn::ItemMod) -> String {
        let start = self.span_to_byte_offset(item.span().start());
        let end = self.span_to_byte_offset(item.span().end());
        let original = &self.content[start..end];
        original.to_string()
    }

    /// Find the index of an item by type and name
    #[allow(dead_code)]
    pub(crate) fn find_item_index(&self, node_type: &str, name: &str) -> Result<usize> {
        for (index, item) in self.syntax_tree.items.iter().enumerate() {
            match (node_type, item) {
                ("struct", Item::Struct(s)) if s.ident == name => {
                    return Ok(index);
                }
                ("enum", Item::Enum(e)) if e.ident == name => {
                    return Ok(index);
                }
                ("fn", Item::Fn(f)) if f.sig.ident == name => {
                    return Ok(index);
                }
                ("impl", Item::Impl(impl_block)) => {
                    // For impl blocks, match on the self_ty
                    if let syn::Type::Path(type_path) = &*impl_block.self_ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            if segment.ident == name {
                                return Ok(index);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        anyhow::bail!("Item '{}' of type '{}' not found", name, node_type)
    }

    /// Replace an item at a specific index with a new item
    #[allow(dead_code)]
    pub(crate) fn replace_item_at_index(&mut self, index: usize, new_item: Item) -> Result<()> {
        if index >= self.syntax_tree.items.len() {
            anyhow::bail!("Index {} out of bounds", index);
        }

        // Replace the item in the syntax tree
        self.syntax_tree.items[index] = new_item;

        // Reformat the entire file using prettyplease
        self.content = prettyplease::unparse(&self.syntax_tree);

        // Recompute line offsets
        self.line_offsets = Self::compute_line_offsets(&self.content);

        Ok(())
    }

    pub fn find_node(&self, node_type: &str, name: &str) -> Result<Vec<NodeLocation>> {
        let mut locations = Vec::new();
        
        for item in &self.syntax_tree.items {
            match (node_type, item) {
                ("struct", Item::Struct(s)) if s.ident == name => {
                    locations.push(self.span_to_location(s.span()));
                }
                ("enum", Item::Enum(e)) if e.ident == name => {
                    locations.push(self.span_to_location(e.span()));
                }
                ("fn", Item::Fn(f)) if f.sig.ident == name => {
                    locations.push(self.span_to_location(f.span()));
                }
                _ => {}
            }
        }
        
        if locations.is_empty() {
            anyhow::bail!("Node '{}' of type '{}' not found", name, node_type);
        }
        
        Ok(locations)
    }
    
    fn span_to_location(&self, span: Span) -> NodeLocation {
        let start = span.start();
        let end = span.end();

        NodeLocation {
            line: start.line,
            column: start.column,
            end_line: end.line,
            end_column: end.column,
        }
    }

    /// Generic transform operation - find matching nodes and apply action
    pub(crate) fn transform(&mut self, op: &crate::operations::TransformOp) -> Result<ModificationResult> {
        use crate::operations::{InspectResult, TransformAction};

        // First, use inspect to find all matching nodes (comments not needed for transform)
        let matches = self.inspect(&op.node_type, op.name_filter.as_deref(), false)?;

        // Apply content filter if specified
        let filtered_matches: Vec<InspectResult> = if let Some(ref content_filter) = op.content_filter {
            matches.into_iter()
                .filter(|m| m.snippet.contains(content_filter))
                .collect()
        } else {
            matches
        };

        if filtered_matches.is_empty() {
            return Ok(ModificationResult {
                changed: false,
                modified_nodes: vec![],
            });
        }

        // Now apply the transformation action to each match
        // We need to work backwards through the file to avoid offset issues
        let mut sorted_matches = filtered_matches;
        sorted_matches.sort_by(|a, b| {
            b.location.line.cmp(&a.location.line)
                .then(b.location.column.cmp(&a.location.column))
        });

        let mut modified_nodes = Vec::new();

        for match_result in &sorted_matches {
            // Create backup node
            let backup_node = BackupNode {
                node_type: match_result.node_type.clone(),
                identifier: match_result.identifier.clone(),
                original_content: match_result.snippet.clone(),
                location: match_result.location.clone(),
            };

            // Find the byte offsets for this node
            let start_offset = self.line_column_to_byte_offset(
                match_result.location.line,
                match_result.location.column
            )?;
            let end_offset = self.line_column_to_byte_offset(
                match_result.location.end_line,
                match_result.location.end_column
            )?;

            // Extract the original text
            let original_text = &self.content[start_offset..end_offset];

            // Apply the action
            let replacement = match &op.action {
                TransformAction::Comment => {
                    // Comment out the code
                    format!("// {}", original_text.replace("\n", "\n// "))
                }
                TransformAction::Remove => {
                    // Remove the entire node
                    String::new()
                }
                TransformAction::Replace { with } => {
                    // Replace with provided code
                    with.clone()
                }
            };

            // Replace in content
            self.content.replace_range(start_offset..end_offset, &replacement);

            // Recompute line offsets after each change
            self.line_offsets = Self::compute_line_offsets(&self.content);

            modified_nodes.push(backup_node);
        }

        // Re-parse the content if we made changes
        if !modified_nodes.is_empty() {
            // Don't reparse for now - we're doing text-level operations
            // self.syntax_tree = syn::parse_str(&self.content)
            //     .context("Failed to re-parse content after transformation")?;
        }

        Ok(ModificationResult {
            changed: !modified_nodes.is_empty(),
            modified_nodes,
        })
    }

    /// Rename an enum variant across the entire file
    pub(crate) fn rename_enum_variant(&mut self, op: &crate::operations::RenameEnumVariantOp) -> Result<ModificationResult> {
        use crate::operations::EditMode;

        // Create a path resolver if a canonical path was provided
        let path_resolver = if let Some(enum_path) = &op.enum_path {
            let mut resolver = PathResolver::new(enum_path)
                .ok_or_else(|| anyhow::anyhow!("Invalid enum path: {}", enum_path))?;

            // Scan the file for use statements to build the alias map
            resolver.scan_file(&self.syntax_tree);
            Some(resolver)
        } else {
            None
        };

        match op.edit_mode {
            EditMode::Surgical => {
                // Use non-mutating visitor to collect replacement locations
                use syn::visit::Visit;
                use crate::surgical::Replacement;

                let mut collector = EnumVariantReplacementCollector {
                    enum_name: op.enum_name.clone(),
                    old_variant: op.old_variant.clone(),
                    new_variant: op.new_variant.clone(),
                    path_resolver,
                    replacements: Vec::new(),
                };

                collector.visit_file(&self.syntax_tree);

                if collector.replacements.is_empty() {
                    return Ok(ModificationResult {
                        changed: false,
                        modified_nodes: vec![],
                    });
                }

                // Apply surgical edits to original content
                self.content = crate::surgical::apply_surgical_edits(&self.content, collector.replacements);

                // Recompute line offsets
                self.line_offsets = Self::compute_line_offsets(&self.content);

                // Re-parse the modified content
                self.syntax_tree = syn::parse_str(&self.content)
                    .context("Failed to re-parse after surgical edit")?;

                let backup_node = BackupNode {
                    node_type: "EnumVariantRename".to_string(),
                    identifier: format!("{}::{} -> {} (surgical)", op.enum_name, op.old_variant, op.new_variant),
                    original_content: format!("Renamed {} to {} in enum {} (surgical mode)", op.old_variant, op.new_variant, op.enum_name),
                    location: NodeLocation {
                        line: 1,
                        column: 0,
                        end_line: 1,
                        end_column: 0,
                    },
                };

                Ok(ModificationResult {
                    changed: true,
                    modified_nodes: vec![backup_node],
                })
            }
            EditMode::Reformat => {
                // Use mutating visitor (original behavior)
                let mut renamer = EnumVariantRenamer {
                    enum_name: op.enum_name.clone(),
                    old_variant: op.old_variant.clone(),
                    new_variant: op.new_variant.clone(),
                    path_resolver,
                    modified: false,
                };

                // Visit and mutate the syntax tree
                renamer.visit_file_mut(&mut self.syntax_tree);

                if !renamer.modified {
                    return Ok(ModificationResult {
                        changed: false,
                        modified_nodes: vec![],
                    });
                }

                // Reformat the entire file using prettyplease
                self.content = prettyplease::unparse(&self.syntax_tree);

                // Recompute line offsets
                self.line_offsets = Self::compute_line_offsets(&self.content);

                // Create a backup node for the entire file operation
                let backup_node = BackupNode {
                    node_type: "EnumVariantRename".to_string(),
                    identifier: format!("{}::{} -> {}", op.enum_name, op.old_variant, op.new_variant),
                    original_content: format!("Renamed {} to {} in enum {}", op.old_variant, op.new_variant, op.enum_name),
                    location: NodeLocation {
                        line: 1,
                        column: 0,
                        end_line: 1,
                        end_column: 0,
                    },
                };

                Ok(ModificationResult {
                    changed: true,
                    modified_nodes: vec![backup_node],
                })
            }
        }
    }

    /// Rename a function across the entire file
    pub(crate) fn rename_function(&mut self, op: &crate::operations::RenameFunctionOp) -> Result<ModificationResult> {
        use crate::operations::EditMode;

        // Create a path resolver if a canonical path was provided
        let path_resolver = if let Some(function_path) = &op.function_path {
            let mut resolver = PathResolver::new(function_path)
                .ok_or_else(|| anyhow::anyhow!("Invalid function path: {}", function_path))?;

            // Scan the file for use statements to build the alias map
            resolver.scan_file(&self.syntax_tree);
            Some(resolver)
        } else {
            None
        };

        match op.edit_mode {
            EditMode::Surgical => {
                // Use non-mutating visitor to collect replacement locations
                use syn::visit::Visit;

                let mut collector = FunctionReplacementCollector {
                    old_name: op.old_name.clone(),
                    new_name: op.new_name.clone(),
                    path_resolver,
                    replacements: Vec::new(),
                };

                collector.visit_file(&self.syntax_tree);

                if collector.replacements.is_empty() {
                    return Ok(ModificationResult {
                        changed: false,
                        modified_nodes: vec![],
                    });
                }

                // Apply surgical edits to original content
                self.content = crate::surgical::apply_surgical_edits(&self.content, collector.replacements);

                // Recompute line offsets
                self.line_offsets = Self::compute_line_offsets(&self.content);

                // Re-parse the modified content
                self.syntax_tree = syn::parse_str(&self.content)
                    .context("Failed to re-parse after surgical edit")?;

                let backup_node = BackupNode {
                    node_type: "FunctionRename".to_string(),
                    identifier: format!("{} -> {} (surgical)", op.old_name, op.new_name),
                    original_content: format!("Renamed {} to {} (surgical mode)", op.old_name, op.new_name),
                    location: NodeLocation {
                        line: 1,
                        column: 0,
                        end_line: 1,
                        end_column: 0,
                    },
                };

                Ok(ModificationResult {
                    changed: true,
                    modified_nodes: vec![backup_node],
                })
            }
            EditMode::Reformat => {
                // Use mutating visitor (original behavior)
                let mut renamer = FunctionRenamer {
                    old_name: op.old_name.clone(),
                    new_name: op.new_name.clone(),
                    path_resolver,
                    modified: false,
                };

                // Visit and mutate the syntax tree
                renamer.visit_file_mut(&mut self.syntax_tree);

                if !renamer.modified {
                    return Ok(ModificationResult {
                        changed: false,
                        modified_nodes: vec![],
                    });
                }

                // Reformat the entire file using prettyplease
                self.content = prettyplease::unparse(&self.syntax_tree);

                // Recompute line offsets
                self.line_offsets = Self::compute_line_offsets(&self.content);

                // Create a backup node for the entire file operation
                let backup_node = BackupNode {
                    node_type: "FunctionRename".to_string(),
                    identifier: format!("{} -> {}", op.old_name, op.new_name),
                    original_content: format!("Renamed {} to {}", op.old_name, op.new_name),
                    location: NodeLocation {
                        line: 1,
                        column: 0,
                        end_line: 1,
                        end_column: 0,
                    },
                };

                Ok(ModificationResult {
                    changed: true,
                    modified_nodes: vec![backup_node],
                })
            }
        }
    }

    /// Convert line/column to byte offset
    fn line_column_to_byte_offset(&self, line: usize, column: usize) -> Result<usize> {
        if line == 0 || line > self.line_offsets.len() {
            anyhow::bail!("Line {} out of range", line);
        }

        let line_start = self.line_offsets[line - 1];
        Ok(line_start + column)
    }
}

// Visitor for adding match arms
struct MatchArmAdder {
    target_function: Option<String>,
    arm_to_add: Arm,
    modified: bool,
    current_function: Option<String>,
    modified_function: Option<String>,
}

impl VisitMut for MatchArmAdder {
    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        let prev_fn = self.current_function.clone();
        self.current_function = Some(node.sig.ident.to_string());

        // Continue visiting nested items
        syn::visit_mut::visit_item_fn_mut(self, node);

        self.current_function = prev_fn;
    }

    fn visit_expr_match_mut(&mut self, node: &mut ExprMatch) {
        // Check if we're in the right function (if specified)
        if let Some(ref target) = self.target_function {
            if self.current_function.as_ref() != Some(target) {
                // Continue visiting nested expressions
                syn::visit_mut::visit_expr_match_mut(self, node);
                return;
            }
        }

        // Check if the pattern already exists (idempotent)
        let pattern_str = self.arm_to_add.pat.to_token_stream().to_string();
        let already_exists = node.arms.iter().any(|arm| {
            arm.pat.to_token_stream().to_string() == pattern_str
        });

        if !already_exists {
            // Add the arm to the end
            node.arms.push(self.arm_to_add.clone());
            self.modified = true;
            self.modified_function = self.current_function.clone();
        }

        // Continue visiting nested expressions
        syn::visit_mut::visit_expr_match_mut(self, node);
    }
}

// Visitor for updating match arms
struct MatchArmUpdater {
    target_function: Option<String>,
    pattern_to_match: String,
    new_body: syn::Expr,
    modified: bool,
    current_function: Option<String>,
    modified_function: Option<String>,
}

impl VisitMut for MatchArmUpdater {
    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        let prev_fn = self.current_function.clone();
        self.current_function = Some(node.sig.ident.to_string());

        syn::visit_mut::visit_item_fn_mut(self, node);

        self.current_function = prev_fn;
    }

    fn visit_expr_match_mut(&mut self, node: &mut ExprMatch) {
        // Check if we're in the right function (if specified)
        if let Some(ref target) = self.target_function {
            if self.current_function.as_ref() != Some(target) {
                syn::visit_mut::visit_expr_match_mut(self, node);
                return;
            }
        }

        // Find and update the matching arm
        for arm in &mut node.arms {
            let pattern_str = arm.pat.to_token_stream().to_string();
            // Normalize whitespace for comparison
            let pattern_normalized = pattern_str.replace(" ", "");
            let target_normalized = self.pattern_to_match.replace(" ", "");

            if pattern_normalized == target_normalized {
                arm.body = Box::new(self.new_body.clone());
                self.modified = true;
                self.modified_function = self.current_function.clone();
                break;
            }
        }

        syn::visit_mut::visit_expr_match_mut(self, node);
    }
}

// Visitor for removing match arms
struct MatchArmRemover {
    target_function: Option<String>,
    pattern_to_remove: String,
    modified: bool,
    current_function: Option<String>,
    modified_function: Option<String>,
}

impl VisitMut for MatchArmRemover {
    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        let prev_fn = self.current_function.clone();
        self.current_function = Some(node.sig.ident.to_string());

        syn::visit_mut::visit_item_fn_mut(self, node);

        self.current_function = prev_fn;
    }

    fn visit_expr_match_mut(&mut self, node: &mut ExprMatch) {
        // Check if we're in the right function (if specified)
        if let Some(ref target) = self.target_function {
            if self.current_function.as_ref() != Some(target) {
                syn::visit_mut::visit_expr_match_mut(self, node);
                return;
            }
        }

        // Find and remove the matching arm
        let mut index_to_remove = None;
        for (i, arm) in node.arms.iter().enumerate() {
            let pattern_str = arm.pat.to_token_stream().to_string();
            // Normalize whitespace for comparison
            let pattern_normalized = pattern_str.replace(" ", "");
            let target_normalized = self.pattern_to_remove.replace(" ", "");

            if pattern_normalized == target_normalized {
                index_to_remove = Some(i);
                break;
            }
        }

        if let Some(index) = index_to_remove {
            node.arms.remove(index);
            self.modified = true;
            self.modified_function = self.current_function.clone();
        }

        syn::visit_mut::visit_expr_match_mut(self, node);
    }
}

// Visitor for adding multiple match arms at once (for auto-detect)
struct MultiMatchArmAdder {
    target_function: Option<String>,
    arms_to_add: Vec<(String, Arm)>,  // (pattern_string, arm)
    modified: bool,
    current_function: Option<String>,
    modified_function: Option<String>,
}

impl VisitMut for MultiMatchArmAdder {
    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        let prev_fn = self.current_function.clone();
        self.current_function = Some(node.sig.ident.to_string());

        syn::visit_mut::visit_item_fn_mut(self, node);

        self.current_function = prev_fn;
    }

    fn visit_expr_match_mut(&mut self, node: &mut ExprMatch) {
        // Check if we're in the right function (if specified)
        if let Some(ref target) = self.target_function {
            if self.current_function.as_ref() != Some(target) {
                syn::visit_mut::visit_expr_match_mut(self, node);
                return;
            }
        }

        // Add all missing arms
        for (pattern_str, arm) in &self.arms_to_add {
            // Check if the pattern already exists (idempotent)
            let already_exists = node.arms.iter().any(|existing_arm| {
                existing_arm.pat.to_token_stream().to_string() == *pattern_str
            });

            if !already_exists {
                node.arms.push(arm.clone());
                self.modified = true;
                self.modified_function = self.current_function.clone();
            }
        }

        syn::visit_mut::visit_expr_match_mut(self, node);
    }
}

// Visitor for adding fields to struct literal expressions
struct StructLiteralFieldAdder {
    struct_name: String,
    field_def: String,
    field_name: String,
    position: InsertPosition,
    path_resolver: Option<PathResolver>,
    modified: bool,
}

impl VisitMut for StructLiteralFieldAdder {
    fn visit_expr_mut(&mut self, node: &mut Expr) {
        // Check if this is a struct literal expression
        if let Expr::Struct(expr_struct) = node {
            let is_match = if let Some(resolver) = &self.path_resolver {
                // Use PathResolver for safe matching
                resolver.matches_target(&expr_struct.path)
            } else {
                // Fallback to legacy pattern matching
                // - "Rectangle" → only Rectangle { ... } (no :: prefix)
                // - "*::Rectangle" → any path ending with Rectangle (View::Rectangle, etc.)
                // - "View::Rectangle" → exact match only View::Rectangle

                if self.struct_name.contains("::") {
                    // Pattern contains :: - check for exact or wildcard match
                    if self.struct_name.starts_with("*::") {
                        // Wildcard: *::Rectangle matches any path ending with Rectangle
                        let target_name = &self.struct_name[3..]; // Skip "*::"
                        expr_struct.path.segments.last()
                            .map(|seg| seg.ident.to_string() == target_name)
                            .unwrap_or(false)
                    } else {
                        // Exact path match: View::Rectangle
                        let path_str = expr_struct.path.segments.iter()
                            .map(|seg| seg.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::");
                        path_str == self.struct_name
                    }
                } else {
                    // No :: in pattern - only match pure struct literals (no path qualifier)
                    expr_struct.path.segments.len() == 1
                        && expr_struct.path.segments.last()
                            .map(|seg| seg.ident.to_string())
                            .as_ref() == Some(&self.struct_name)
                }
            };

            if is_match {
                // Check if field already exists (idempotent)
                let field_exists = expr_struct.fields.iter().any(|fv| {
                    fv.member.to_token_stream().to_string() == self.field_name
                });

                if !field_exists {
                    // Parse the field value from field_def
                    // field_def is like "return_type: None"
                    let field_value_code = format!("{{ {} }}", self.field_def);
                    if let Ok(expr) = parse_str::<ExprStruct>(&format!("Dummy {}", field_value_code)) {
                        if let Some(new_fv) = expr.fields.first() {
                            // Determine where to insert
                            match &self.position {
                                InsertPosition::First => {
                                    expr_struct.fields.insert(0, new_fv.clone());
                                    self.modified = true;
                                }
                                InsertPosition::Last => {
                                    expr_struct.fields.push(new_fv.clone());
                                    self.modified = true;
                                }
                                InsertPosition::After(after_field) => {
                                    // Find the position of the field to insert after
                                    if let Some(pos) = expr_struct.fields.iter().position(|fv| {
                                        fv.member.to_token_stream().to_string() == *after_field
                                    }) {
                                        expr_struct.fields.insert(pos + 1, new_fv.clone());
                                        self.modified = true;
                                    }
                                }
                                InsertPosition::Before(before_field) => {
                                    // Find the position of the field to insert before
                                    if let Some(pos) = expr_struct.fields.iter().position(|fv| {
                                        fv.member.to_token_stream().to_string() == *before_field
                                    }) {
                                        expr_struct.fields.insert(pos, new_fv.clone());
                                        self.modified = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // IMPORTANT: Visit children AFTER processing this node
        // This ensures we traverse into nested expressions
        syn::visit_mut::visit_expr_mut(self, node);
    }
}

// Visitor for renaming enum variants
struct EnumVariantRenamer {
    enum_name: String,
    old_variant: String,
    new_variant: String,
    path_resolver: Option<PathResolver>,
    modified: bool,
}

impl EnumVariantRenamer {
    /// Rename a path segment if it matches using path resolution.
    ///
    /// This method handles various path forms:
    /// - Simple paths: `EnumName::Variant`
    /// - Qualified paths: `crate::module::EnumName::Variant`
    /// - Imported paths: `Variant` (when enum is imported via use statement)
    ///
    /// When a PathResolver is configured, it validates that paths refer to
    /// the correct enum before renaming.
    fn rename_path(&mut self, path: &mut syn::Path) {
        // Check if the path ends with EnumName::VariantName
        let segments: Vec<_> = path.segments.iter().collect();
        let len = segments.len();

        if len >= 2 {
            // Path has at least enum and variant segments
            let potential_variant = &segments[len - 1];
            let potential_enum = &segments[len - 2];

            if potential_enum.ident == self.enum_name
                && potential_variant.ident == self.old_variant
            {
                // Path ends with EnumName::VariantName

                // If we have a path resolver, validate the enum path
                if let Some(resolver) = &self.path_resolver {
                    // Extract just the enum path (everything except the variant)
                    let enum_path = syn::Path {
                        leading_colon: path.leading_colon,
                        segments: path.segments.iter()
                            .take(len - 1)
                            .cloned()
                            .collect(),
                    };

                    // Only rename if the enum path matches our target
                    if resolver.matches_target(&enum_path) {
                        path.segments[len - 1].ident = syn::Ident::new(
                            &self.new_variant,
                            path.segments[len - 1].ident.span()
                        );
                        self.modified = true;
                    }
                } else {
                    // No resolver - use simple matching (backward compatible)
                    // Only match if it's exactly EnumName::Variant (2 segments)
                    if len == 2 {
                        path.segments[1].ident = syn::Ident::new(
                            &self.new_variant,
                            path.segments[1].ident.span()
                        );
                        self.modified = true;
                    }
                }
            }
        } else if len == 1 {
            // Single segment path - check if it's an imported variant
            if segments[0].ident == self.old_variant {
                // When using path resolver, we don't rename single-segment paths
                // unless they're explicitly imported (which would be unusual for variants)
                // For backward compatibility, we still rename them when no resolver is present
                if self.path_resolver.is_none() {
                    path.segments[0].ident = syn::Ident::new(
                        &self.new_variant,
                        path.segments[0].ident.span()
                    );
                    self.modified = true;
                }
            }
        }
    }
}

impl VisitMut for EnumVariantRenamer {
    /// Rename in enum definition
    fn visit_item_enum_mut(&mut self, node: &mut syn::ItemEnum) {
        if node.ident == self.enum_name {
            for variant in &mut node.variants {
                if variant.ident == self.old_variant {
                    variant.ident = syn::Ident::new(&self.new_variant, variant.ident.span());
                    self.modified = true;
                }
            }
        }

        // Continue visiting nested items
        syn::visit_mut::visit_item_enum_mut(self, node);
    }

    /// Rename in patterns (match arms, let bindings, function parameters, etc.)
    fn visit_pat_mut(&mut self, pat: &mut syn::Pat) {
        match pat {
            syn::Pat::TupleStruct(tuple_struct) => {
                self.rename_path(&mut tuple_struct.path);
            }
            syn::Pat::Struct(struct_pat) => {
                self.rename_path(&mut struct_pat.path);
            }
            syn::Pat::Path(path_pat) => {
                self.rename_path(&mut path_pat.path);
            }
            _ => {}
        }

        // Continue visiting nested patterns
        syn::visit_mut::visit_pat_mut(self, pat);
    }

    /// Rename in expressions (constructor calls, references, etc.)
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        match expr {
            syn::Expr::Path(expr_path) => {
                self.rename_path(&mut expr_path.path);
            }
            syn::Expr::Call(call) => {
                if let syn::Expr::Path(path) = &mut *call.func {
                    self.rename_path(&mut path.path);
                }
            }
            syn::Expr::Struct(struct_expr) => {
                self.rename_path(&mut struct_expr.path);
            }
            _ => {}
        }

        // Continue visiting nested expressions
        syn::visit_mut::visit_expr_mut(self, expr);
    }
}

// Non-mutating visitor for collecting replacement locations (surgical mode)
struct EnumVariantReplacementCollector {
    enum_name: String,
    old_variant: String,
    new_variant: String,
    path_resolver: Option<PathResolver>,
    replacements: Vec<crate::surgical::Replacement>,
}

impl EnumVariantReplacementCollector {
    /// Check if a path matches and collect replacement if it does
    fn collect_path_replacement(&mut self, path: &syn::Path) {
        let segments: Vec<_> = path.segments.iter().collect();
        let len = segments.len();

        if len >= 2 {
            let potential_variant = &segments[len - 1];
            let potential_enum = &segments[len - 2];

            if potential_enum.ident == self.enum_name
                && potential_variant.ident == self.old_variant
            {
                // Path ends with EnumName::VariantName

                // Validate with path resolver if available
                let should_rename = if let Some(resolver) = &self.path_resolver {
                    let enum_path = syn::Path {
                        leading_colon: path.leading_colon,
                        segments: path.segments.iter()
                            .take(len - 1)
                            .cloned()
                            .collect(),
                    };
                    resolver.matches_target(&enum_path)
                } else {
                    // No resolver - only match exactly 2 segments
                    len == 2
                };

                if should_rename {
                    let span = potential_variant.ident.span();
                    let start = span.start();
                    let end = span.end();

                    self.replacements.push(crate::surgical::Replacement::new(
                        start,
                        end,
                        self.new_variant.clone(),
                    ));
                }
            }
        } else if len == 1 && self.path_resolver.is_none() {
            // Single segment - only without path resolver (backward compat)
            if segments[0].ident == self.old_variant {
                let span = segments[0].ident.span();
                let start = span.start();
                let end = span.end();

                self.replacements.push(crate::surgical::Replacement::new(
                    start,
                    end,
                    self.new_variant.clone(),
                ));
            }
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for EnumVariantReplacementCollector {
    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        if node.ident == self.enum_name {
            for variant in &node.variants {
                if variant.ident == self.old_variant {
                    let span = variant.ident.span();
                    let start = span.start();
                    let end = span.end();

                    self.replacements.push(crate::surgical::Replacement::new(
                        start,
                        end,
                        self.new_variant.clone(),
                    ));
                }
            }
        }
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_pat(&mut self, pat: &'ast syn::Pat) {
        match pat {
            syn::Pat::TupleStruct(tuple_struct) => {
                self.collect_path_replacement(&tuple_struct.path);
            }
            syn::Pat::Struct(struct_pat) => {
                self.collect_path_replacement(&struct_pat.path);
            }
            syn::Pat::Path(path_pat) => {
                self.collect_path_replacement(&path_pat.path);
            }
            _ => {}
        }
        syn::visit::visit_pat(self, pat);
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            syn::Expr::Path(expr_path) => {
                self.collect_path_replacement(&expr_path.path);
            }
            syn::Expr::Call(call) => {
                if let syn::Expr::Path(path) = &*call.func {
                    self.collect_path_replacement(&path.path);
                }
            }
            syn::Expr::Struct(struct_expr) => {
                self.collect_path_replacement(&struct_expr.path);
            }
            _ => {}
        }
        syn::visit::visit_expr(self, expr);
    }
}

// Mutating visitor for renaming functions (reformat mode)
struct FunctionRenamer {
    old_name: String,
    new_name: String,
    path_resolver: Option<PathResolver>,
    modified: bool,
}

impl FunctionRenamer {
    /// Rename a function identifier if it matches
    fn rename_ident(&mut self, ident: &mut syn::Ident) {
        if ident == &self.old_name {
            *ident = syn::Ident::new(&self.new_name, ident.span());
            self.modified = true;
        }
    }

    /// Check if a path matches our target function (with path resolution)
    fn matches_target_function(&self, path: &syn::Path) -> bool {
        if let Some(resolver) = &self.path_resolver {
            resolver.matches_target(path)
        } else {
            // Simple matching: just check if the last segment is our function name
            path.segments.len() == 1 && path.segments.last().unwrap().ident == self.old_name
        }
    }
}

impl VisitMut for FunctionRenamer {
    fn visit_item_fn_mut(&mut self, node: &mut syn::ItemFn) {
        // Rename function definition
        self.rename_ident(&mut node.sig.ident);
        syn::visit_mut::visit_item_fn_mut(self, node);
    }

    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        match expr {
            syn::Expr::Call(call) => {
                // Rename function calls
                if let syn::Expr::Path(expr_path) = &mut *call.func {
                    if self.matches_target_function(&expr_path.path) {
                        if let Some(last_seg) = expr_path.path.segments.last_mut() {
                            self.rename_ident(&mut last_seg.ident);
                        }
                    }
                }
            }
            syn::Expr::Path(expr_path) => {
                // Rename function references (not calls)
                if self.matches_target_function(&expr_path.path) {
                    if let Some(last_seg) = expr_path.path.segments.last_mut() {
                        self.rename_ident(&mut last_seg.ident);
                    }
                }
            }
            _ => {}
        }
        syn::visit_mut::visit_expr_mut(self, expr);
    }
}

// Non-mutating visitor for collecting function replacement locations (surgical mode)
struct FunctionReplacementCollector {
    old_name: String,
    new_name: String,
    path_resolver: Option<PathResolver>,
    replacements: Vec<crate::surgical::Replacement>,
}

impl FunctionReplacementCollector {
    /// Collect replacement for a function identifier
    fn collect_replacement(&mut self, ident: &syn::Ident) {
        if ident == &self.old_name {
            let span = ident.span();
            let start = span.start();
            let end = span.end();

            self.replacements.push(crate::surgical::Replacement::new(
                start,
                end,
                self.new_name.clone(),
            ));
        }
    }

    /// Check if a path matches our target function
    fn matches_target_function(&self, path: &syn::Path) -> bool {
        if let Some(resolver) = &self.path_resolver {
            resolver.matches_target(path)
        } else {
            // Simple matching
            path.segments.len() == 1 && path.segments.last().unwrap().ident == self.old_name
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for FunctionReplacementCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        // Collect function definition rename
        self.collect_replacement(&node.sig.ident);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            syn::Expr::Call(call) => {
                // Collect function call renames
                if let syn::Expr::Path(expr_path) = &*call.func {
                    if self.matches_target_function(&expr_path.path) {
                        if let Some(last_seg) = expr_path.path.segments.last() {
                            self.collect_replacement(&last_seg.ident);
                        }
                    }
                }
                // Visit call arguments but NOT the func (already handled above)
                for arg in &call.args {
                    syn::visit::visit_expr(self, arg);
                }
                // Don't call the default visitor which would re-visit call.func
                return;
            }
            syn::Expr::Path(expr_path) => {
                // Collect function reference renames (not calls)
                if self.matches_target_function(&expr_path.path) {
                    if let Some(last_seg) = expr_path.path.segments.last() {
                        self.collect_replacement(&last_seg.ident);
                    }
                }
            }
            _ => {}
        }
        syn::visit::visit_expr(self, expr);
    }
}

// ============================================================================
// Doc Comment Operations
// ============================================================================

/// Generate a documentation comment in the specified style
fn generate_doc_comment(text: &str, style: &DocCommentStyle) -> String {
    match style {
        DocCommentStyle::Line => {
            // Split by newlines and add /// prefix to each line
            text.lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        "///".to_string()
                    } else {
                        format!("/// {}", line)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        DocCommentStyle::Block => {
            // Simple block comment
            if text.contains('\n') {
                // Multi-line block comment
                let lines = text.lines()
                    .map(|line| format!(" * {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("/**\n{}\n */", lines)
            } else {
                // Single-line block comment
                format!("/** {} */", text)
            }
        }
    }
}

/// Extract preceding comments (both doc and regular) before a given line
/// Returns None if no comments found, Some(comment_text) if comments exist
fn extract_preceding_comment(content: &str, start_line: usize) -> Option<String> {
    if start_line == 0 {
        return None;
    }

    let lines: Vec<&str> = content.lines().collect();
    if start_line > lines.len() {
        return None;
    }

    let line_idx = start_line.saturating_sub(1); // Convert 1-based to 0-based

    // Scan backwards from target to find comment lines
    let mut comment_start = line_idx;
    let mut found_any_comment = false;

    while comment_start > 0 {
        let prev_line = lines[comment_start - 1].trim();

        // Check if this is a comment line
        let is_comment = prev_line.starts_with("///")
            || prev_line.starts_with("//!")
            || prev_line.starts_with("//")
            || prev_line.starts_with("/**")
            || prev_line.starts_with("/*!")
            || prev_line.starts_with("/*")
            || (prev_line.starts_with("*") && !prev_line.starts_with("*/"))
            || prev_line == "*/";

        if is_comment {
            comment_start -= 1;
            found_any_comment = true;
        } else if prev_line.is_empty() && found_any_comment {
            // Allow blank lines within comment blocks, but don't continue past them
            // unless we're in the middle of a block comment
            comment_start -= 1;
        } else {
            break;
        }
    }

    if !found_any_comment {
        return None;
    }

    // Extract the comment lines and preserve original formatting
    let comment_lines: Vec<String> = lines[comment_start..line_idx]
        .iter()
        .map(|&line| line.to_string())
        .collect();

    if comment_lines.is_empty() {
        None
    } else {
        Some(comment_lines.join("\n"))
    }
}

/// Find the byte position and indentation of a target item
struct TargetFinder {
    target_type: String,
    target_name: String,
    found_position: Option<(usize, String)>, // (line_number, indentation)
}

impl TargetFinder {
    fn new(target_type: String, target_name: String) -> Self {
        Self {
            target_type,
            target_name,
            found_position: None,
        }
    }
}

impl<'ast> syn::visit::Visit<'ast> for TargetFinder {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if self.target_type == "struct" && node.ident.to_string() == self.target_name {
            // Use struct_token span to get the line where "struct" keyword appears,
            // not the first line of the item (which includes doc comments)
            let line = node.struct_token.span.start().line;
            self.found_position = Some((line, String::new()));
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if self.target_type == "enum" && node.ident.to_string() == self.target_name {
            // Use enum_token span to get the line where "enum" keyword appears
            let line = node.enum_token.span.start().line;
            self.found_position = Some((line, String::new()));
        }
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if self.target_type == "function" && node.sig.ident.to_string() == self.target_name {
            // Use fn_token span to get the line where "fn" keyword appears
            let line = node.sig.fn_token.span.start().line;
            self.found_position = Some((line, String::new()));
        }
        syn::visit::visit_item_fn(self, node);
    }
}

impl RustEditor {
    /// Add a documentation comment to a target item using surgical editing
    pub fn add_doc_comment_surgical(
        &mut self,
        target_type: &str,
        target_name: &str,
        doc_text: &str,
        style: &DocCommentStyle,
    ) -> Result<ModificationResult> {
        use syn::visit::Visit;

        // Find the target item
        let mut finder = TargetFinder::new(
            target_type.to_string(),
            target_name.to_string(),
        );
        finder.visit_file(&self.syntax_tree);

        if let Some((line_num, _indent)) = finder.found_position {
            // Lines are 1-indexed from syn, convert to 0-indexed
            let line_idx = line_num.saturating_sub(1);

            // Generate the doc comment
            let comment = generate_doc_comment(doc_text, style);

            // Find the actual line in the source
            let lines: Vec<&str> = self.content.lines().collect();
            if line_idx >= lines.len() {
                anyhow::bail!("Target not found at line {}", line_num);
            }

            let target_line = lines[line_idx].to_string(); // Clone to avoid borrow issues
            let target_line_len = target_line.len();

            // Detect indentation from the target line
            let indent = target_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            // Build the new content with comment inserted
            let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

            // Insert comment lines before the target
            let comment_lines: Vec<String> = comment
                .lines()
                .map(|line| format!("{}{}", indent, line))
                .collect();

            // Insert in reverse order to maintain indices
            for (i, comment_line) in comment_lines.iter().rev().enumerate() {
                new_lines.insert(line_idx, comment_line.clone());
            }

            // Update content
            self.content = new_lines.join("\n");

            // Re-parse to update syntax tree
            self.syntax_tree = syn::parse_str(&self.content)
                .context("Failed to re-parse after adding comment")?;

            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![BackupNode {
                    node_type: target_type.to_string(),
                    identifier: target_name.to_string(),
                    original_content: target_line,
                    location: NodeLocation {
                        line: line_num,
                        column: 1,
                        end_line: line_num,
                        end_column: target_line_len,
                    },
                }],
            })
        } else {
            anyhow::bail!("Target {} '{}' not found", target_type, target_name)
        }
    }

    /// Update an existing documentation comment on a target item using surgical editing
    pub fn update_doc_comment_surgical(
        &mut self,
        target_type: &str,
        target_name: &str,
        doc_text: &str,
        style: &DocCommentStyle,
    ) -> Result<ModificationResult> {
        use syn::visit::Visit;

        // Find the target item
        let mut finder = TargetFinder::new(
            target_type.to_string(),
            target_name.to_string(),
        );
        finder.visit_file(&self.syntax_tree);

        if let Some((line_num, _indent)) = finder.found_position {
            // Lines are 1-indexed from syn, convert to 0-indexed
            let line_idx = line_num.saturating_sub(1);

            // Find the actual line in the source
            let lines: Vec<&str> = self.content.lines().collect();
            if line_idx >= lines.len() {
                anyhow::bail!("Target not found at line {}", line_num);
            }

            let target_line = lines[line_idx].to_string(); // Clone to avoid borrow issues
            let target_line_len = target_line.len();

            // Detect indentation from the target line
            let indent = target_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            // Scan backwards from target to find existing doc comment lines
            let mut doc_comment_start = line_idx;
            while doc_comment_start > 0 {
                let prev_line = lines[doc_comment_start - 1].trim();
                if prev_line.starts_with("///") || prev_line.starts_with("//!") ||
                   prev_line.starts_with("/**") || prev_line.starts_with("/*!") ||
                   (prev_line.starts_with("*") && !prev_line.starts_with("*/")) ||
                   prev_line == "*/" {
                    doc_comment_start -= 1;
                } else {
                    break;
                }
            }

            // Build new content with old comments removed and new ones inserted
            let mut new_lines: Vec<String> = Vec::new();

            // Add lines before the doc comments
            for i in 0..doc_comment_start {
                new_lines.push(lines[i].to_string());
            }

            // Generate and add new doc comment
            let comment = generate_doc_comment(doc_text, style);
            let comment_lines: Vec<String> = comment
                .lines()
                .map(|line| format!("{}{}", indent, line))
                .collect();

            for comment_line in comment_lines {
                new_lines.push(comment_line);
            }

            // Add remaining lines (from target onwards)
            for i in line_idx..lines.len() {
                new_lines.push(lines[i].to_string());
            }

            // Update content
            self.content = new_lines.join("\n");

            // Re-parse to update syntax tree
            self.syntax_tree = syn::parse_str(&self.content)
                .context("Failed to re-parse after updating comment")?;

            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![BackupNode {
                    node_type: target_type.to_string(),
                    identifier: target_name.to_string(),
                    original_content: target_line,
                    location: NodeLocation {
                        line: line_num,
                        column: 1,
                        end_line: line_num,
                        end_column: target_line_len,
                    },
                }],
            })
        } else {
            anyhow::bail!("Target {} '{}' not found", target_type, target_name)
        }
    }

    /// Remove a documentation comment from a target item using surgical editing
    pub fn remove_doc_comment_surgical(
        &mut self,
        target_type: &str,
        target_name: &str,
    ) -> Result<ModificationResult> {
        use syn::visit::Visit;

        // Find the target item
        let mut finder = TargetFinder::new(
            target_type.to_string(),
            target_name.to_string(),
        );
        finder.visit_file(&self.syntax_tree);

        if let Some((line_num, _indent)) = finder.found_position {
            // Lines are 1-indexed from syn, convert to 0-indexed
            let line_idx = line_num.saturating_sub(1);

            // Find the actual line in the source
            let lines: Vec<&str> = self.content.lines().collect();
            if line_idx >= lines.len() {
                anyhow::bail!("Target not found at line {}", line_num);
            }

            let target_line = lines[line_idx].to_string(); // Clone to avoid borrow issues
            let target_line_len = target_line.len();

            // Scan backwards from target to find existing doc comment lines
            let mut doc_comment_start = line_idx;
            while doc_comment_start > 0 {
                let prev_line = lines[doc_comment_start - 1].trim();
                if prev_line.starts_with("///") || prev_line.starts_with("//!") ||
                   prev_line.starts_with("/**") || prev_line.starts_with("/*!") ||
                   (prev_line.starts_with("*") && !prev_line.starts_with("*/")) ||
                   prev_line == "*/" {
                    doc_comment_start -= 1;
                } else {
                    break;
                }
            }

            // Build new content with doc comments removed
            let mut new_lines: Vec<String> = Vec::new();

            // Add lines before the doc comments
            for i in 0..doc_comment_start {
                new_lines.push(lines[i].to_string());
            }

            // Skip the doc comment lines (from doc_comment_start to line_idx)

            // Add remaining lines (from target onwards)
            for i in line_idx..lines.len() {
                new_lines.push(lines[i].to_string());
            }

            // Update content
            self.content = new_lines.join("\n");

            // Re-parse to update syntax tree
            self.syntax_tree = syn::parse_str(&self.content)
                .context("Failed to re-parse after removing comment")?;

            Ok(ModificationResult {
                changed: true,
                modified_nodes: vec![BackupNode {
                    node_type: target_type.to_string(),
                    identifier: target_name.to_string(),
                    original_content: target_line,
                    location: NodeLocation {
                        line: line_num,
                        column: 1,
                        end_line: line_num,
                        end_column: target_line_len,
                    },
                }],
            })
        } else {
            anyhow::bail!("Target {} '{}' not found", target_type, target_name)
        }
    }
}
