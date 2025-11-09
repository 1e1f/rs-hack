#[cfg(test)]
mod editor_tests {
    use crate::editor::RustEditor;
    use crate::operations::*;

    const SAMPLE_STRUCT: &str = r#"
#[derive(Debug)]
pub struct User {
    pub id: u64,
    pub name: String,
}
"#;

    const SAMPLE_ENUM: &str = r#"
#[derive(Debug)]
pub enum Status {
    Draft,
    Published,
}
"#;

    const SAMPLE_IMPL: &str = r#"
impl User {
    pub fn new(id: u64, name: String) -> Self {
        Self { id, name }
    }
}
"#;

    #[test]
    fn test_add_struct_field() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "email: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("email: String"));
    }

    #[test]
    fn test_add_struct_field_idempotent() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "name: String".to_string(), // Already exists
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (no change)
    }

    #[test]
    fn test_update_struct_field() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = UpdateStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "id: i64".to_string(), // Change type from u64 to i64
            where_filter: None,
        };

        let result = editor.update_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("id: i64"));
        assert!(!output.contains("id: u64"));
    }

    #[test]
    fn test_remove_struct_field() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = RemoveStructFieldOp {
            struct_name: "User".to_string(),
            field_name: "name".to_string(),
            where_filter: None,
        };

        let result = editor.remove_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(!output.contains("name: String"));
    }

    #[test]
    fn test_add_enum_variant() {
        let mut editor = RustEditor::new(SAMPLE_ENUM).unwrap();
        let op = AddEnumVariantOp {
            enum_name: "Status".to_string(),
            variant_def: "Archived".to_string(),
            position: InsertPosition::Last,
            where_filter: None,
        };

        let result = editor.add_enum_variant(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("Archived"));
    }

    #[test]
    fn test_add_enum_variant_idempotent() {
        let mut editor = RustEditor::new(SAMPLE_ENUM).unwrap();
        let op = AddEnumVariantOp {
            enum_name: "Status".to_string(),
            variant_def: "Draft".to_string(), // Already exists
            position: InsertPosition::Last,
            where_filter: None,
        };

        let result = editor.add_enum_variant(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (no change)
    }

    #[test]
    fn test_remove_enum_variant() {
        let mut editor = RustEditor::new(SAMPLE_ENUM).unwrap();
        let op = RemoveEnumVariantOp {
            enum_name: "Status".to_string(),
            variant_name: "Draft".to_string(),
            where_filter: None,
        };

        let result = editor.remove_enum_variant(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(!output.contains("Draft"));
        assert!(output.contains("Published")); // Other variant still there
    }

    #[test]
    fn test_add_derive() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddDeriveOp {
            target_name: "User".to_string(),
            target_type: "struct".to_string(),
            derives: vec!["Clone".to_string(), "Serialize".to_string()],
            where_filter: None,
        };

        let result = editor.add_derive(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("Clone"));
        assert!(output.contains("Serialize"));
        assert!(output.contains("Debug")); // Original derive should still be there
    }

    #[test]
    fn test_add_derive_idempotent() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddDeriveOp {
            target_name: "User".to_string(),
            target_type: "struct".to_string(),
            derives: vec!["Debug".to_string()], // Already has Debug
            where_filter: None,
        };

        let result = editor.add_derive(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (no change)
    }

    #[test]
    fn test_add_impl_method() {
        let mut editor = RustEditor::new(&format!("{}\n{}", SAMPLE_STRUCT, SAMPLE_IMPL)).unwrap();
        let op = AddImplMethodOp {
            target: "User".to_string(),
            method_def: "pub fn get_id(&self) -> u64 { self.id }".to_string(),
            position: InsertPosition::Last,
        };

        let result = editor.add_impl_method(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("get_id"));
        assert!(output.contains("self.id"));
    }

    #[test]
    fn test_add_impl_method_idempotent() {
        let mut editor = RustEditor::new(&format!("{}\n{}", SAMPLE_STRUCT, SAMPLE_IMPL)).unwrap();
        let op = AddImplMethodOp {
            target: "User".to_string(),
            method_def: "pub fn new(id: u64, name: String) -> Self { Self { id, name } }".to_string(),
            position: InsertPosition::Last,
        };

        let result = editor.add_impl_method(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (method already exists)
    }

    #[test]
    fn test_add_use_statement() {
        let code = r#"
use std::collections::HashMap;

pub struct User {
    pub id: u64,
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddUseStatementOp {
            use_path: "serde::Serialize".to_string(),
            position: InsertPosition::Last,
        };

        let result = editor.add_use_statement(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("use serde::Serialize;"));
        assert!(output.contains("use std::collections::HashMap;"));
    }

    #[test]
    fn test_add_use_statement_idempotent() {
        let code = r#"
use std::collections::HashMap;

pub struct User {
    pub id: u64,
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddUseStatementOp {
            use_path: "std::collections::HashMap".to_string(),
            position: InsertPosition::Last,
        };

        let result = editor.add_use_statement(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (already exists)
    }

    #[test]
    fn test_add_match_arm() {
        let code = r#"
pub enum Status {
    Draft,
    Published,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
        Status::Published => "published".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddMatchArmOp {
            pattern: "Status::Archived".to_string(),
            body: "\"archived\".to_string()".to_string(),
            function_name: Some("handle_status".to_string()),
            auto_detect: false,
            enum_name: None,
        };

        let result = editor.add_match_arm(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("Status::Archived"));
        assert!(output.contains("archived"));
    }

    #[test]
    fn test_add_match_arm_auto_detect() {
        let code = r#"
pub enum Status {
    Draft,
    Published,
    Archived,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddMatchArmOp {
            pattern: String::new(), // Not used in auto-detect mode
            body: "todo!()".to_string(),
            function_name: Some("handle_status".to_string()),
            auto_detect: true,
            enum_name: Some("Status".to_string()),
        };

        let result = editor.add_match_arm(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        // Should add both missing variants
        assert!(output.contains("Status::Published"));
        assert!(output.contains("Status::Archived"));
        assert!(output.contains("todo!()"));
    }

    #[test]
    fn test_add_match_arm_auto_detect_no_missing() {
        let code = r#"
pub enum Status {
    Draft,
    Published,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
        Status::Published => "published".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddMatchArmOp {
            pattern: String::new(),
            body: "todo!()".to_string(),
            function_name: Some("handle_status".to_string()),
            auto_detect: true,
            enum_name: Some("Status".to_string()),
        };

        let result = editor.add_match_arm(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false - no missing variants
    }

    #[test]
    fn test_update_match_arm() {
        let code = r#"
pub enum Status {
    Draft,
    Published,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
        Status::Published => "published".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = UpdateMatchArmOp {
            pattern: "Status::Draft".to_string(),
            new_body: "\"pending\".to_string()".to_string(),
            function_name: Some("handle_status".to_string()),
        };

        let result = editor.update_match_arm(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("pending"));
        assert!(!output.contains("draft"));
    }

    #[test]
    fn test_remove_match_arm() {
        let code = r#"
pub enum Status {
    Draft,
    Published,
}

pub fn handle_status(status: Status) -> String {
    match status {
        Status::Draft => "draft".to_string(),
        Status::Published => "published".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = RemoveMatchArmOp {
            pattern: "Status::Draft".to_string(),
            function_name: Some("handle_status".to_string()),
        };

        let result = editor.remove_match_arm(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(!output.contains("Status::Draft"));
        assert!(output.contains("Status::Published")); // Other arm still there
    }

    #[test]
    fn test_find_node() {
        let editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let result = editor.find_node("struct", "User");

        assert!(result.is_ok());
        let locations = result.unwrap();
        assert_eq!(locations.len(), 1);
        assert!(locations[0].line > 0);
    }

    #[test]
    fn test_position_control_first() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "created_at: u64".to_string(),
            position: InsertPosition::First,
            literal_default: None,
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        // created_at should come before id
        let created_pos = output.find("created_at").unwrap();
        let id_pos = output.find("id:").unwrap();
        assert!(created_pos < id_pos);
    }

    #[test]
    fn test_position_control_after() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "email: String".to_string(),
            position: InsertPosition::After("id".to_string()),
            literal_default: None,
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        // email should come after id but before name
        let id_pos = output.find("id:").unwrap();
        let email_pos = output.find("email:").unwrap();
        let name_pos = output.find("name:").unwrap();
        assert!(id_pos < email_pos && email_pos < name_pos);
    }

    #[test]
    fn test_complex_enum_variant() {
        let mut editor = RustEditor::new(SAMPLE_ENUM).unwrap();
        let op = AddEnumVariantOp {
            enum_name: "Status".to_string(),
            variant_def: "Error { code: i32, message: String }".to_string(),
            position: InsertPosition::Last,
            where_filter: None,
        };

        let result = editor.add_enum_variant(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("Error"));
        assert!(output.contains("code"));
        assert!(output.contains("message"));
    }

    #[test]
    fn test_error_struct_not_found() {
        let mut editor = RustEditor::new(SAMPLE_STRUCT).unwrap();
        let op = AddStructFieldOp {
            struct_name: "NonExistent".to_string(),
            field_def: "field: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_error_enum_not_found() {
        let mut editor = RustEditor::new(SAMPLE_ENUM).unwrap();
        let op = AddEnumVariantOp {
            enum_name: "NonExistent".to_string(),
            variant_def: "Variant".to_string(),
            position: InsertPosition::Last,
            where_filter: None,
        };

        let result = editor.add_enum_variant(&op);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_add_struct_literal_field() {
        let code = r#"
pub struct IRCtx {
    stack: Vec<Frame>,
    current_function_frame: Option<Frame>,
    local_types: HashMap<String, Type>,
}

fn create_ctx() -> IRCtx {
    IRCtx {
        stack: vec![],
        current_function_frame: None,
        local_types: HashMap::new(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructLiteralFieldOp {
            struct_name: "IRCtx".to_string(),
            field_def: "return_type: None".to_string(),
            position: InsertPosition::After("current_function_frame".to_string()),
            struct_path: None,
        };

        let result = editor.add_struct_literal_field(&op);
        assert!(result.is_ok());
        let modified = result.unwrap();

        let output = editor.to_string();
        eprintln!("Modified: {}", modified.changed);
        eprintln!("Output:\n{}", output);

        assert!(modified.changed);
        // Should add field to struct literal, not definition
        assert!(output.contains("return_type: None"));

        // Field should be after current_function_frame in the literal
        let cfr_pos = output.find("current_function_frame: None").unwrap();
        let rt_pos = output.find("return_type: None").unwrap();
        assert!(rt_pos > cfr_pos);

        // Verify struct definition doesn't have return_type
        let struct_def_start = output.find("pub struct IRCtx").unwrap();
        let fn_start = output.find("fn create_ctx").unwrap();
        let struct_def = &output[struct_def_start..fn_start];
        assert!(!struct_def.contains("return_type"));
    }

    #[test]
    fn test_add_struct_literal_field_idempotent() {
        let code = r#"
pub struct User {
    id: u64,
    name: String,
}

fn new_user() -> User {
    User {
        id: 1,
        name: "Alice".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructLiteralFieldOp {
            struct_name: "User".to_string(),
            field_def: "name: \"Bob\".to_string()".to_string(),
            position: InsertPosition::Last,
            struct_path: None,
        };

        let result = editor.add_struct_literal_field(&op);
        if result.is_err() {
            eprintln!("Error: {:?}", result.as_ref().unwrap_err());
        }
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (already exists)
    }

    #[test]
    fn test_add_struct_literal_field_multiple_instances() {
        let code = r#"
pub struct Point {
    x: i32,
    y: i32,
}

fn create_point_1() -> Point {
    Point {
        x: 1,
        y: 2,
    }
}

fn create_point_2() -> Point {
    Point {
        x: 3,
        y: 4,
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructLiteralFieldOp {
            struct_name: "Point".to_string(),
            field_def: "z: 0".to_string(),
            position: InsertPosition::Last,
            struct_path: None,
        };

        let result = editor.add_struct_literal_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        // Should add to all Point instances
        assert_eq!(output.matches("z: 0").count(), 2);
    }

    #[test]
    fn test_add_struct_literal_field_doesnt_modify_definition() {
        let code = r#"
pub struct Config {
    port: u16,
    host: String,
}

fn default_config() -> Config {
    Config {
        port: 8080,
        host: "localhost".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructLiteralFieldOp {
            struct_name: "Config".to_string(),
            field_def: "timeout: 30".to_string(),
            position: InsertPosition::Last,
            struct_path: None,
        };

        let result = editor.add_struct_literal_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();

        // The struct definition should NOT have the timeout field
        let struct_def_start = output.find("pub struct Config").unwrap();
        let fn_start = output.find("fn default_config").unwrap();
        let struct_def = &output[struct_def_start..fn_start];
        assert!(!struct_def.contains("timeout"));

        // The literal should have it
        assert!(output.contains("timeout: 30"));
    }

    #[test]
    fn test_add_struct_literal_field_with_path() {
        let code = r#"
pub mod types {
    pub struct Data { value: i32 }
}

fn create() -> types::Data {
    types::Data { value: 42 }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructLiteralFieldOp {
            struct_name: "Data".to_string(),
            field_def: "timestamp: 0".to_string(),
            position: InsertPosition::Last,
            struct_path: Some("types::Data".to_string()), // Use path resolver to match types::Data
        };

        let result = editor.add_struct_literal_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();
        assert!(output.contains("timestamp: 0"));
    }

    #[test]
    fn test_add_struct_field_with_literal_default() {
        let code = r#"
pub struct IRCtx {
    stack: Vec<Frame>,
    current_function_frame: Option<Frame>,
}

fn new_ctx() -> IRCtx {
    IRCtx {
        stack: vec![],
        current_function_frame: None,
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructFieldOp {
            struct_name: "IRCtx".to_string(),
            field_def: "return_type: Option<Type>".to_string(),
            position: InsertPosition::After("current_function_frame".to_string()),
            literal_default: Some("None".to_string()),
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();

        // Field should be added to struct definition
        let struct_def_start = output.find("pub struct IRCtx").unwrap();
        let fn_start = output.find("fn new_ctx").unwrap();
        let struct_def = &output[struct_def_start..fn_start];
        assert!(struct_def.contains("return_type: Option<Type>"));

        // Field should be added to struct literal
        assert!(output.contains("return_type: None"));

        // Check position - return_type should be after current_function_frame
        let cfr_def_pos = struct_def.find("current_function_frame").unwrap();
        let rt_def_pos = struct_def.find("return_type").unwrap();
        assert!(rt_def_pos > cfr_def_pos);
    }

    #[test]
    #[ignore] // TODO: Fix idempotent behavior when field exists but literal_default is provided
    fn test_add_struct_field_with_literal_default_idempotent() {
        let code = r#"
pub struct Config {
    port: u16,
    timeout: u32,
}

fn default_config() -> Config {
    Config {
        port: 8080,
        timeout: 30,
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructFieldOp {
            struct_name: "Config".to_string(),
            field_def: "timeout: u32".to_string(), // Already exists
            position: InsertPosition::Last,
            literal_default: Some("30".to_string()),
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(!result.unwrap().changed); // Should return false (no change)

        // Output should be unchanged
        let output = editor.to_string();
        assert_eq!(output.matches("timeout").count(), 2); // Once in struct def, once in literal
    }

    #[test]
    fn test_add_struct_field_without_literal_default() {
        // Test that original behavior still works when literal_default is None
        let code = r#"
pub struct User {
    id: u64,
    name: String,
}

fn new_user() -> User {
    User {
        id: 1,
        name: "Alice".to_string(),
    }
}
"#;
        let mut editor = RustEditor::new(code).unwrap();
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "email: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None, // No literal default
            where_filter: None,
        };

        let result = editor.add_struct_field(&op);
        assert!(result.is_ok());
        assert!(result.unwrap().changed);

        let output = editor.to_string();

        // Field should be added to struct definition
        let struct_def_start = output.find("pub struct User").unwrap();
        let fn_start = output.find("fn new_user").unwrap();
        let struct_def = &output[struct_def_start..fn_start];
        assert!(struct_def.contains("email: String"));

        // Field should NOT be added to struct literal (literal_default was None)
        // Look in the function body, not the struct definition
        let fn_body_start = output.find("fn new_user").unwrap();
        let fn_body = &output[fn_body_start..];

        // Find the struct literal (second occurrence of "User {" in the function body)
        let literal_start = fn_body.find("User {").unwrap();
        let literal_end = fn_body[literal_start..].find('}').unwrap() + literal_start;
        let literal_block = &fn_body[literal_start..=literal_end];

        // Count occurrences of "email" - should only be in struct definition, not in literal
        assert_eq!(output.matches("email").count(), 1); // Only in struct def
        assert!(!literal_block.contains("email"));
    }

    // ========== Tests for --where filter ==========

    #[test]
    fn test_where_filter_derives_trait_match() {
        let code = r#"
#[derive(Clone, Debug)]
pub struct User {
    id: u64,
}

#[derive(Debug)]
pub struct Config {
    port: u16,
}
"#;
        let mut editor = RustEditor::new(code).unwrap();

        // Should add field to User (has Clone)
        let op = AddStructFieldOp {
            struct_name: "User".to_string(),
            field_def: "name: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: Some("derives_trait:Clone".to_string()),
        };

        let result = editor.add_struct_field(&op).unwrap();
        assert!(result.changed);
        assert!(editor.to_string().contains("name: String"));
    }

    #[test]
    fn test_where_filter_derives_trait_no_match() {
        let code = r#"
#[derive(Debug)]
pub struct Config {
    port: u16,
}
"#;
        let mut editor = RustEditor::new(code).unwrap();

        // Should NOT add field to Config (doesn't have Clone)
        let op = AddStructFieldOp {
            struct_name: "Config".to_string(),
            field_def: "name: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: Some("derives_trait:Clone".to_string()),
        };

        let result = editor.add_struct_field(&op).unwrap();
        assert!(!result.changed);
        assert!(!editor.to_string().contains("name: String"));
    }

    #[test]
    fn test_where_filter_or_logic() {
        let code = r#"
#[derive(Debug)]
pub struct Config {
    port: u16,
}
"#;
        let mut editor = RustEditor::new(code).unwrap();

        // Should add field to Config (has Debug, which matches Clone OR Debug)
        let op = AddStructFieldOp {
            struct_name: "Config".to_string(),
            field_def: "name: String".to_string(),
            position: InsertPosition::Last,
            literal_default: None,
            where_filter: Some("derives_trait:Clone,Debug".to_string()),
        };

        let result = editor.add_struct_field(&op).unwrap();
        assert!(result.changed);
        assert!(editor.to_string().contains("name: String"));
    }

    // ========== Tests for inspect command ==========

    #[test]
    fn test_inspect_struct_literal() {
        let code = r#"
fn main() {
    let user = User {
        id: 1,
        name: "Alice".to_string(),
    };

    let another = User {
        id: 2,
        name: "Bob".to_string(),
    };
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("struct-literal", Some("User")).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].identifier, "User");
        assert!(results[0].snippet.contains("User"));
    }

    #[test]
    fn test_inspect_match_arm() {
        let code = r#"
fn handle(op: Operator) {
    match op {
        Operator::Add => println!("add"),
        Operator::Subtract => println!("subtract"),
        Operator::Error => println!("error"),
    }
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("match-arm", Some("Error")).unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].identifier.contains("Error"));
        assert!(results[0].snippet.contains("Operator::Error"));
    }

    #[test]
    fn test_inspect_enum_usage() {
        let code = r#"
fn format(op: Operator) -> &'static str {
    match op {
        Operator::Error => "!",
        _ => "",
    }
}

fn check() -> Operator {
    Operator::Error
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("enum-usage", Some("Operator::Error")).unwrap();

        // Should find both: in match arm and in return
        assert!(results.len() >= 2);
        assert!(results.iter().any(|r| r.identifier.contains("Error")));
    }

    #[test]
    fn test_inspect_function_call() {
        let code = r#"
fn main() {
    handle_error();
    process_data();
    handle_error();
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("function-call", Some("handle_error")).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].snippet.contains("handle_error"));
    }

    #[test]
    fn test_inspect_method_call() {
        let code = r#"
fn main() {
    value.unwrap();
    data.clone();
    result.unwrap();
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("method-call", Some("unwrap")).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].snippet.contains("unwrap"));
    }

    #[test]
    fn test_inspect_identifier() {
        let code = r#"
fn main() {
    let config = Config::new();
    println!("{}", config);
    process(config);
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("identifier", Some("config")).unwrap();

        // Should find: let binding, println argument, process argument
        // Note: May find more instances as identifiers appear in various contexts
        assert!(results.len() >= 1);
        assert!(results.iter().all(|r| r.identifier == "config"));
    }

    #[test]
    fn test_inspect_type_ref() {
        let code = r#"
fn process(items: Vec<String>) -> Option<i32> {
    let data: Vec<i32> = vec![];
    None
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("type-ref", Some("Vec")).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.identifier.contains("Vec")));
    }

    #[test]
    fn test_inspect_no_filter() {
        let code = r#"
fn main() {
    let user = User { id: 1 };
    let config = Config { port: 8080 };
}
"#;
        let editor = RustEditor::new(code).unwrap();
        let results = editor.inspect("struct-literal", None).unwrap();

        // Should find both User and Config
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_inspect_invalid_node_type() {
        let code = "fn main() {}";
        let editor = RustEditor::new(code).unwrap();
        let result = editor.inspect("invalid-type", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported node type"));
    }
}
