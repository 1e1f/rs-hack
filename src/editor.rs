use anyhow::{Context, Result};
use proc_macro2::{LineColumn, Span};
use syn::{
    parse_str, File, Item, ItemEnum, ItemStruct,
    Fields, Field, spanned::Spanned, Arm, ExprMatch,
    visit_mut::VisitMut,
};
use quote::ToTokens;

use crate::operations::*;

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
    
    pub fn apply_operation(&mut self, op: &Operation) -> Result<bool> {
        match op {
            Operation::AddStructField(op) => self.add_struct_field(op),
            Operation::UpdateStructField(op) => self.update_struct_field(op),
            Operation::RemoveStructField(op) => self.remove_struct_field(op),
            Operation::AddEnumVariant(op) => self.add_enum_variant(op),
            Operation::UpdateEnumVariant(op) => self.update_enum_variant(op),
            Operation::RemoveEnumVariant(op) => self.remove_enum_variant(op),
            Operation::AddMatchArm(op) => self.add_match_arm(op),
            Operation::UpdateMatchArm(op) => self.update_match_arm(op),
            Operation::RemoveMatchArm(op) => self.remove_match_arm(op),
            Operation::AddImplMethod(op) => self.add_impl_method(op),
            Operation::AddUseStatement(op) => self.add_use_statement(op),
            Operation::AddDerive(op) => self.add_derive(op),
        }
    }
    
    fn add_struct_field(&mut self, op: &AddStructFieldOp) -> Result<bool> {
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

        self.insert_struct_field(&item_struct, op)
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

    fn update_struct_field(&mut self, op: &UpdateStructFieldOp) -> Result<bool> {
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

        self.replace_struct_field(&item_struct, op)
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

    fn remove_struct_field(&mut self, op: &RemoveStructFieldOp) -> Result<bool> {
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

            return Ok(true);
        }

        anyhow::bail!("Struct '{}' does not have named fields", op.struct_name)
    }

    fn add_enum_variant(&mut self, op: &AddEnumVariantOp) -> Result<bool> {
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

        self.insert_enum_variant(&item_enum, op)
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

    fn update_enum_variant(&mut self, op: &UpdateEnumVariantOp) -> Result<bool> {
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

        Ok(true)
    }

    fn remove_enum_variant(&mut self, op: &RemoveEnumVariantOp) -> Result<bool> {
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

        Ok(true)
    }

    fn add_match_arm(&mut self, op: &AddMatchArmOp) -> Result<bool> {
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
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn update_match_arm(&mut self, op: &UpdateMatchArmOp) -> Result<bool> {
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
            Ok(true)
        } else {
            anyhow::bail!("Pattern '{}' not found in any match expression", op.pattern)
        }
    }

    fn remove_match_arm(&mut self, op: &RemoveMatchArmOp) -> Result<bool> {
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
            Ok(true)
        } else {
            anyhow::bail!("Pattern '{}' not found in any match expression", op.pattern)
        }
    }

    fn add_impl_method(&mut self, op: &AddImplMethodOp) -> Result<bool> {
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
            return Ok(false);
        }

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

        Ok(true)
    }

    fn add_use_statement(&mut self, op: &AddUseStatementOp) -> Result<bool> {
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
            return Ok(false);
        }

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

        Ok(true)
    }

    fn add_derive(&mut self, op: &AddDeriveOp) -> Result<bool> {
        // Find the target item (struct or enum)
        let item_index = self.syntax_tree.items.iter().position(|item| {
            match (&op.target_type as &str, item) {
                ("struct", Item::Struct(s)) => s.ident == op.target_name,
                ("enum", Item::Enum(e)) => e.ident == op.target_name,
                _ => false,
            }
        }).ok_or_else(|| anyhow::anyhow!("{} '{}' not found", op.target_type, op.target_name))?;

        // Get the item and check for existing derives
        let (existing_derives, item_span) = match &self.syntax_tree.items[item_index] {
            Item::Struct(s) => (Self::extract_derives(&s.attrs), s.span()),
            Item::Enum(e) => (Self::extract_derives(&e.attrs), e.span()),
            _ => (Vec::new(), proc_macro2::Span::call_site()),
        };

        // Filter out derives that already exist (idempotent)
        let new_derives: Vec<String> = op.derives.iter()
            .filter(|d| !existing_derives.contains(&d.to_string()))
            .cloned()
            .collect();

        if new_derives.is_empty() {
            // All derives already exist
            return Ok(false);
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

        Ok(true)
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

    /// Replace the attributes section of an item in the source
    fn replace_item_attrs(&mut self, item_span: Span, new_attrs: &[syn::Attribute]) -> Result<()> {
        // Get the position where the actual item starts (pub struct/enum keyword)
        let item_def_start = self.span_to_byte_offset(item_span.start());

        // Find the beginning of the line where the item definition starts
        let mut def_line_start = item_def_start;
        while def_line_start > 0 && self.content.as_bytes()[def_line_start - 1] != b'\n' {
            def_line_start -= 1;
        }

        // Search backwards for attribute lines (lines starting with #[)
        let mut attrs_region_start = def_line_start;
        let mut temp_pos = def_line_start;

        while temp_pos > 0 {
            // Move to start of previous line
            temp_pos = temp_pos.saturating_sub(1);
            let mut prev_line_start = temp_pos;
            while prev_line_start > 0 && self.content.as_bytes()[prev_line_start - 1] != b'\n' {
                prev_line_start -= 1;
            }

            // Get the line content
            let line = if temp_pos < self.content.len() {
                &self.content[prev_line_start..temp_pos + 1]
            } else {
                &self.content[prev_line_start..]
            };
            let trimmed = line.trim();

            if trimmed.starts_with("#[") {
                // Found an attribute line, extend the region
                attrs_region_start = prev_line_start;
                temp_pos = prev_line_start;
            } else if trimmed.is_empty() {
                // Empty line, continue searching
                temp_pos = prev_line_start;
            } else {
                // Hit a non-attribute, non-empty line - stop
                break;
            }

            if prev_line_start == 0 {
                break;
            }
        }

        // Build the new attributes string with proper formatting
        let attrs_str = if new_attrs.is_empty() {
            String::new()
        } else {
            new_attrs.iter()
                .map(|a| format!("{}", a.to_token_stream()))
                .collect::<Vec<_>>()
                .join("\n") + "\n"
        };

        // Replace the entire attributes region
        self.content.replace_range(attrs_region_start..def_line_start, &attrs_str);

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
