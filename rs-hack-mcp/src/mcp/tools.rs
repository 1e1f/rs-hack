use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::process::Command;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub struct ToolRegistry {
    tools: Vec<Tool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: vec![
                // ============================================================
                // INSPECTION TOOLS (1)
                // ============================================================
                Tool {
                    name: "find",
                    description: "Find and list AST nodes. DISCOVERY MODE: Omit --node-type to search ALL types with auto-grouped output (recommended for exploration). TARGETED MODE: Specify --node-type for precise searches. v0.5.3: Shows intelligent hints for qualified paths (e.g., suggests '*::StructName' when finding crate::mod::StructName). v0.5.1: --limit for result limiting, --field-name to find all uses of a field. Use --format snippets (shows code), locations (grep-like), or json (structured). Better than grep: AST-aware, no false positives from comments/strings.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "node_type": {
                                "type": "string",
                                "enum": ["struct-literal", "match-arm", "enum-usage", "function-call", "method-call", "macro-call", "identifier", "type-ref", "struct", "enum", "function", "impl-method", "trait", "const", "static", "type-alias", "mod"],
                                "description": "Type of AST node to inspect. Omit to search ALL types with grouped output."
                            },
                            "name": {"type": "string", "description": "Optional name filter (e.g., \"Shadow\", \"Operator::Error\", \"unwrap\", \"View::Rectangle\"). v0.5.3: Use '*::StructName' wildcard to match all qualified paths."},
                            "variant": {"type": "string", "description": "Filter enum variants by name (only valid with --node-type enum)"},
                            "content_filter": {"type": "string", "description": "Filter by content substring"},
                            "field_name": {"type": "string", "description": "Find all occurrences of a field across struct definitions, enum variants, and struct literals"},
                            "include_comments": {"type": "boolean", "default": true, "description": "Include preceding comments (doc and regular) in output"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"},
                            "limit": {"type": "integer", "description": "Limit number of results (like 'head -N')"}
                        },
                        "required": ["paths"]
                    }),
                },

                // ============================================================
                // UNIFIED CRUD TOOLS (4) - v0.5.0
                // These replace 17 legacy hyphenated commands with semantic operations
                // ============================================================
                Tool {
                    name: "add",
                    description: "Unified add command - auto-detects operation type. Add struct fields, enum variants, impl methods, derives, use statements, match arms, or doc comments. v0.5.3: Shows hints when simple names miss qualified paths (suggests '*::StructName' pattern). Works seamlessly with imported structs - no definition needed for literal-only ops. v0.5.1 Field API: Use --field-name + --field-type (definition only), --field-name + --field-value (literals only), or all three (both). For enum variant literals: --name \"Enum::Variant\" --field-name --field-value --kind struct. IMPORTANT: --variant is for adding NEW variants, not for targeting existing variant literals.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "name": {"type": "string", "description": "Name of the target (struct/enum/function name). Required for most operations except --use. Use Enum::Variant for enum variant struct literals. v0.5.3: Use '*::StructName' wildcard to match all qualified paths (e.g., crate::mod::StructName, other::path::StructName)."},
                            "kind": {"type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"], "description": "Semantic grouping for broad operations. 'struct' = struct definitions + struct literals + enum variant literals. Use this for operations affecting all instances. Mutually exclusive with --node-type."},
                            "node_type": {"type": "string", "description": "Granular AST node type for surgical precision (e.g., 'struct' = definitions only, 'struct-literal' = initialization expressions only). Use this when you need fine control. Mutually exclusive with --kind."},
                            "field": {"type": "string", "description": "[DEPRECATED] Field definition (e.g., \"email: String\"). Use --field-name + --field-type instead."},
                            "field_name": {"type": "string", "description": "[v0.5.1] Field name (e.g., \"email\"). Use with --field-type and/or --field-value. For enum variant literals, use --name \"Enum::Variant\" syntax instead of --variant."},
                            "field_type": {"type": "string", "description": "[v0.5.1] Field type (e.g., \"String\"). Adds to struct definition."},
                            "field_value": {"type": "string", "description": "[v0.5.1] Field value (e.g., \"None\", \"vec![]\"). Adds to struct literals."},
                            "variant": {"type": "string", "description": "Add a NEW variant to an enum definition (e.g., \"Pending\" or \"Error { code: i32 }\"). IMPORTANT: This is NOT for adding fields to existing enum variants - for that, use --name \"Enum::Variant\" --field-name instead. Cannot be combined with --field-name/--field-type/--field-value."},
                            "method": {"type": "string", "description": "Method definition for impl methods (e.g., \"pub fn get_id(&self) -> u64 { self.id }\")"},
                            "derive": {"type": "string", "description": "Comma-separated derive macros (e.g., \"Clone,Debug,Serialize\")"},
                            "use": {"type": "string", "description": "Use statement path (e.g., \"serde::Serialize\"). Omit --name when using --use."},
                            "match_arm": {"type": "string", "description": "Match arm pattern for adding a SINGLE arm (e.g., \"Status::Pending\"). Mutually exclusive with auto_detect. Use for external enums"},
                            "body": {"type": "string", "description": "Body for match arm (e.g., \"println!(\\\"pending\\\")\")"},
                            "function": {"type": "string", "description": "Function name containing the match expression (optional, limits scope)"},
                            "doc_comment": {"type": "string", "description": "Documentation comment text"},
                            "literal_default": {"type": "string", "description": "[DEPRECATED] Use --field-value instead"},
                            "literal_only": {"type": "boolean", "default": false, "description": "Only add to struct/enum literals, not definitions"},
                            "position": {"type": "string", "description": "Position: \"first\", \"last\", \"after:item_name\""},
                            "auto_detect": {"type": "boolean", "default": false, "description": "Auto-detect ALL missing match arms from enum definition. Mutually exclusive with match_arm. Enum must be in scanned files"},
                            "enum_name": {"type": "string", "description": "Enum name for auto_detect mode (required with auto_detect)"},
                            "apply": {"type": "boolean", "default": false, "description": "Apply changes (default is dry-run)"}
                        },
                        "required": ["paths"]
                    }),
                },
                Tool {
                    name: "remove",
                    description: "Unified remove command - auto-detects operation type. Remove struct fields, enum variants, match arms, doc comments, or derives. v0.5.3: Shows hints when simple names miss qualified paths (suggests '*::StructName' pattern). Use --kind (struct/enum/function) or --node-type for granular control.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "name": {"type": "string", "description": "Name of the target (struct/enum/function name) or item to remove. v0.5.3: Use '*::StructName' wildcard to match all qualified paths."},
                            "kind": {"type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"], "description": "Semantic grouping for disambiguation. Mutually exclusive with --node-type."},
                            "node_type": {"type": "string", "description": "Granular AST node type. Mutually exclusive with --kind."},
                            "field_name": {"type": "string", "description": "Name of field to remove from struct"},
                            "variant": {"type": "string", "description": "Name of variant to remove from enum"},
                            "method": {"type": "string", "description": "Name of method to remove from impl"},
                            "derive": {"type": "string", "description": "Derive macro to remove"},
                            "match_arm": {"type": "string", "description": "Match arm pattern to remove"},
                            "function": {"type": "string", "description": "Function name containing the match expression"},
                            "doc_comment": {"type": "boolean", "default": false, "description": "Remove doc comment from the item"},
                            "literal_only": {"type": "boolean", "default": false, "description": "Only remove from literals, not definitions"},
                            "apply": {"type": "boolean", "default": false, "description": "Apply changes (default is dry-run)"}
                        },
                        "required": ["paths", "name"]
                    }),
                },
                Tool {
                    name: "update",
                    description: "Unified update command - auto-detects operation type. Update struct fields, enum variants, match arms, or doc comments. v0.5.1: NEW unified field API with --field-name + --field-type. Use --kind (struct/enum/function) or --node-type for granular control.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "name": {"type": "string", "description": "Name of the target (struct/enum/function name)"},
                            "kind": {"type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"], "description": "Semantic grouping. 'struct' includes enum variant literals. Mutually exclusive with --node-type."},
                            "node_type": {"type": "string", "description": "Granular AST node type. Mutually exclusive with --kind."},
                            "field": {"type": "string", "description": "[DEPRECATED] New field definition (e.g., \"email: Option<String>\"). Use --field-name + --field-type instead."},
                            "field_name": {"type": "string", "description": "[v0.5.1] Field name to update"},
                            "field_type": {"type": "string", "description": "[v0.5.1] New field type (e.g., \"Option<String>\")"},
                            "variant": {"type": "string", "description": "New variant definition for enum variants"},
                            "match_arm": {"type": "string", "description": "Match arm pattern to update"},
                            "body": {"type": "string", "description": "New body for match arm"},
                            "function": {"type": "string", "description": "Function name containing the match expression"},
                            "doc_comment": {"type": "string", "description": "New documentation comment text"},
                            "apply": {"type": "boolean", "default": false, "description": "Apply changes (default is dry-run)"}
                        },
                        "required": ["paths", "name"]
                    }),
                },
                Tool {
                    name: "rename",
                    description: "Unified rename command - rename functions, trait methods, or enum variants. v0.5.1: --kind function now includes trait methods. Defaults to surgical mode (preserves formatting). For functions: provide name in --name. For enum variants: use 'Enum::Variant' syntax or provide --enum-path separately.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "name": {"type": "string", "description": "Current name. For functions/trait methods: 'function_name'. For enum variants: 'Enum::Variant' or just 'Variant' with --enum-path."},
                            "to": {"type": "string", "description": "New name (without path/enum prefix)"},
                            "kind": {"type": "string", "enum": ["function", "enum", "identifier"], "description": "Semantic grouping. 'function' includes trait methods, impl methods, and standalone functions. Mutually exclusive with --node-type."},
                            "node_type": {"type": "string", "enum": ["function-call", "identifier", "enum-variant", "type-ref"], "description": "Granular AST node type. Mutually exclusive with --kind."},
                            "enum_path": {"type": "string", "description": "Qualified enum path for variant renames (e.g., 'types::Status')"},
                            "function_path": {"type": "string", "description": "Module path for function (optional)"},
                            "edit_mode": {"type": "string", "enum": ["surgical", "reformat"], "default": "surgical", "description": "Edit mode: surgical (preserves formatting, default) or reformat"},
                            "validate": {"type": "boolean", "default": true, "description": "Validate with cargo check"},
                            "apply": {"type": "boolean", "default": false, "description": "Apply changes (default is dry-run)"}
                        },
                        "required": ["paths", "name", "to"]
                    }),
                },

                // ============================================================
                // TRANSFORM (1)
                // ============================================================
                Tool {
                    name: "transform",
                    description: "Generic transformation tool - comment, remove, or replace any AST nodes",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "node_type": {"type": "string", "enum": ["macro-call", "method-call", "function-call", "enum-usage", "struct-literal", "match-arm", "identifier", "type-ref"]},
                            "action": {"type": "string", "enum": ["comment", "remove", "replace"]},
                            "name": {"type": "string"},
                            "content_filter": {"type": "string"},
                            "with": {"type": "string", "description": "Replacement code (required if action=replace)"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "node_type", "action"]
                    }),
                },

                // ============================================================
                // BATCH & UTILITY (4)
                // ============================================================
                Tool {
                    name: "batch",
                    description: "Run multiple operations from JSON or YAML specification",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "spec": {"type": "string", "description": "Path to JSON/YAML specification file"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["spec"]
                    }),
                },
                Tool {
                    name: "history",
                    description: "Show history of rs-hack operations",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "limit": {"type": "integer", "default": 10}
                        }
                    }),
                },
                Tool {
                    name: "revert",
                    description: "Revert a previous rs-hack operation by run ID",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "run_id": {"type": "string", "description": "Run ID from history (7-character hash)"},
                            "force": {"type": "boolean", "default": false}
                        },
                        "required": ["run_id"]
                    }),
                },
                Tool {
                    name: "clean",
                    description: "Clean old state data",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "keep_days": {"type": "integer", "default": 30, "description": "Keep state newer than this many days"}
                        }
                    }),
                },
            ],
        }
    }

    pub fn list(&self) -> &[Tool] {
        &self.tools
    }

    pub async fn call(&self, name: &str, arguments: Value) -> Result<String> {
        debug!("Executing tool: {} with args: {:?}", name, arguments);

        // Map tool name to rs-hack command and build arguments
        let (command, args) = self.build_command(name, &arguments)?;

        debug!("Running: rs-hack {} {}", command, args.join(" "));

        // Execute rs-hack command
        let output = Command::new("rs-hack")
            .arg(&command)
            .args(&args)
            .output()
            .map_err(|e| anyhow!("Failed to run rs-hack: {}. Is it installed?", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let result = stdout.trim().to_string();

            // If operation completed and apply was false, add reminder
            if !self.get_bool(&arguments, "apply") && !result.is_empty() {
                Ok(format!("{}\n\n💡 This was a DRY RUN. Use apply=true to make actual changes.", result))
            } else {
                Ok(result)
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("rs-hack failed: {}", stderr))
        }
    }

    fn build_command(&self, tool_name: &str, arguments: &Value) -> Result<(String, Vec<String>)> {
        let mut args = Vec::new();

        // Map tool names to actual rs-hack commands
        let command = match tool_name {
            // Find uses "find" command
            "find" => {
                self.add_find_args(&arguments, &mut args);
                "find"
            }
            // Unified CRUD commands (v0.5.0)
            "add" => {
                self.add_add_args(&arguments, &mut args);
                "add"
            }
            "remove" => {
                self.add_remove_args(&arguments, &mut args);
                "remove"
            }
            "update" => {
                self.add_update_args(&arguments, &mut args);
                "update"
            }
            "rename" => {
                self.add_rename_args(&arguments, &mut args);
                "rename"
            }
            // Transform command
            "transform" => {
                self.add_transform_args(&arguments, &mut args)?;
                "transform"
            }
            // History renamed to "history"
            "history" => {
                if let Some(limit) = arguments.get("limit").and_then(|v| v.as_i64()) {
                    args.push("--limit".to_string());
                    args.push(limit.to_string());
                }
                "history"
            }
            // Revert
            "revert" => {
                if let Some(run_id) = arguments.get("run_id").and_then(|v| v.as_str()) {
                    args.push(run_id.to_string());
                }
                if self.get_bool(arguments, "force") {
                    args.push("--force".to_string());
                }
                "revert"
            }
            // Clean
            "clean" => {
                if let Some(days) = arguments.get("keep_days").and_then(|v| v.as_i64()) {
                    args.push("--keep-days".to_string());
                    args.push(days.to_string());
                }
                "clean"
            }
            // Batch
            "batch" => {
                if let Some(spec) = arguments.get("spec").and_then(|v| v.as_str()) {
                    args.push("--spec".to_string());
                    args.push(spec.to_string());
                }
                if self.get_bool(arguments, "apply") {
                    args.push("--apply".to_string());
                }
                "batch"
            }
            // All other commands map 1:1 (with underscores -> dashes)
            _ => {
                self.add_standard_args(&arguments, &mut args);
                &tool_name.replace('_', "-")
            }
        };

        Ok((command.to_string(), args))
    }

    fn add_find_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Add paths
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add node-type
        if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add name filter
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add variant filter (for enum variant filtering)
        if let Some(variant) = arguments.get("variant").and_then(|v| v.as_str()) {
            args.push("--variant".to_string());
            args.push(variant.to_string());
        }

        // Add content filter
        if let Some(content) = arguments.get("content_filter").and_then(|v| v.as_str()) {
            args.push("--content-filter".to_string());
            args.push(content.to_string());
        }

        // Add include_comments (default is true, only add flag if explicitly set to false)
        if let Some(include_comments) = arguments.get("include_comments").and_then(|v| v.as_bool()) {
            if !include_comments {
                args.push("--include-comments".to_string());
                args.push("false".to_string());
            }
        }

        // Add format
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
        }

        // Add field_name (for field finding)
        if let Some(field_name) = arguments.get("field_name").and_then(|v| v.as_str()) {
            args.push("--field-name".to_string());
            args.push(field_name.to_string());
        }

        // Add limit (for result limiting)
        if let Some(limit) = arguments.get("limit").and_then(|v| v.as_i64()) {
            args.push("--limit".to_string());
            args.push(limit.to_string());
        }
    }

    fn add_transform_args(&self, arguments: &Value, args: &mut Vec<String>) -> Result<()> {
        // Add paths
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add node-type
        if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add action
        if let Some(action) = arguments.get("action").and_then(|v| v.as_str()) {
            args.push("--action".to_string());
            args.push(action.to_string());
        }

        // Add name
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add content-filter
        if let Some(content) = arguments.get("content_filter").and_then(|v| v.as_str()) {
            args.push("--content-filter".to_string());
            args.push(content.to_string());
        }

        // Add with (replacement)
        if let Some(with) = arguments.get("with").and_then(|v| v.as_str()) {
            args.push("--with".to_string());
            args.push(with.to_string());
        }

        // Add apply
        if self.get_bool(arguments, "apply") {
            args.push("--apply".to_string());
        }

        Ok(())
    }

    fn add_standard_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(obj) = arguments.as_object() {
            for (key, value) in obj {
                // Convert snake_case to kebab-case
                let flag = format!("--{}", key.replace('_', "-"));

                match value {
                    Value::Bool(true) => {
                        // Handle boolean true values as flags
                        if key == "apply" || key == "literal_only" || key == "summary" || key == "force" || key == "auto_detect" {
                            args.push(flag);
                        }
                    }
                    Value::Bool(false) => {}, // Skip false booleans (they're usually the default)
                    Value::String(s) => {
                        args.push(flag);
                        args.push(s.clone());
                    }
                    Value::Number(n) => {
                        args.push(flag);
                        args.push(n.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    fn get_bool(&self, arguments: &Value, key: &str) -> bool {
        arguments.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    // ============================================================
    // Unified CRUD command argument builders (v0.5.0)
    // ============================================================

    fn add_add_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Add paths (required)
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add name (optional for some operations like --use)
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add kind or node-type (mutually exclusive)
        if let Some(kind) = arguments.get("kind").and_then(|v| v.as_str()) {
            args.push("--kind".to_string());
            args.push(kind.to_string());
        } else if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add field
        if let Some(field) = arguments.get("field").and_then(|v| v.as_str()) {
            args.push("--field".to_string());
            args.push(field.to_string());
        }

        // Add variant
        if let Some(variant) = arguments.get("variant").and_then(|v| v.as_str()) {
            args.push("--variant".to_string());
            args.push(variant.to_string());
        }

        // Add method
        if let Some(method) = arguments.get("method").and_then(|v| v.as_str()) {
            args.push("--method".to_string());
            args.push(method.to_string());
        }

        // Add derive
        if let Some(derive) = arguments.get("derive").and_then(|v| v.as_str()) {
            args.push("--derive".to_string());
            args.push(derive.to_string());
        }

        // Add use
        if let Some(use_path) = arguments.get("use").and_then(|v| v.as_str()) {
            args.push("--use".to_string());
            args.push(use_path.to_string());
        }

        // Add match-arm
        if let Some(match_arm) = arguments.get("match_arm").and_then(|v| v.as_str()) {
            args.push("--match-arm".to_string());
            args.push(match_arm.to_string());
        }

        // Add body
        if let Some(body) = arguments.get("body").and_then(|v| v.as_str()) {
            args.push("--body".to_string());
            args.push(body.to_string());
        }

        // Add function
        if let Some(function) = arguments.get("function").and_then(|v| v.as_str()) {
            args.push("--function".to_string());
            args.push(function.to_string());
        }

        // Add doc-comment
        if let Some(doc_comment) = arguments.get("doc_comment").and_then(|v| v.as_str()) {
            args.push("--doc-comment".to_string());
            args.push(doc_comment.to_string());
        }

        // Add literal-default
        if let Some(literal_default) = arguments.get("literal_default").and_then(|v| v.as_str()) {
            args.push("--literal-default".to_string());
            args.push(literal_default.to_string());
        }

        // Add literal-only (boolean flag)
        if self.get_bool(arguments, "literal_only") {
            args.push("--literal-only".to_string());
        }

        // Add position
        if let Some(position) = arguments.get("position").and_then(|v| v.as_str()) {
            args.push("--position".to_string());
            args.push(position.to_string());
        }

        // Add auto-detect (boolean flag)
        if self.get_bool(arguments, "auto_detect") {
            args.push("--auto-detect".to_string());
        }

        // Add enum-name
        if let Some(enum_name) = arguments.get("enum_name").and_then(|v| v.as_str()) {
            args.push("--enum-name".to_string());
            args.push(enum_name.to_string());
        }

        // Add apply (boolean flag)
        if self.get_bool(arguments, "apply") {
            args.push("--apply".to_string());
        }
    }

    fn add_remove_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Add paths (required)
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add name (required)
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add kind or node-type (mutually exclusive)
        if let Some(kind) = arguments.get("kind").and_then(|v| v.as_str()) {
            args.push("--kind".to_string());
            args.push(kind.to_string());
        } else if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add field-name
        if let Some(field_name) = arguments.get("field_name").and_then(|v| v.as_str()) {
            args.push("--field-name".to_string());
            args.push(field_name.to_string());
        }

        // Add variant
        if let Some(variant) = arguments.get("variant").and_then(|v| v.as_str()) {
            args.push("--variant".to_string());
            args.push(variant.to_string());
        }

        // Add method
        if let Some(method) = arguments.get("method").and_then(|v| v.as_str()) {
            args.push("--method".to_string());
            args.push(method.to_string());
        }

        // Add derive
        if let Some(derive) = arguments.get("derive").and_then(|v| v.as_str()) {
            args.push("--derive".to_string());
            args.push(derive.to_string());
        }

        // Add match-arm
        if let Some(match_arm) = arguments.get("match_arm").and_then(|v| v.as_str()) {
            args.push("--match-arm".to_string());
            args.push(match_arm.to_string());
        }

        // Add function
        if let Some(function) = arguments.get("function").and_then(|v| v.as_str()) {
            args.push("--function".to_string());
            args.push(function.to_string());
        }

        // Add doc-comment (boolean flag)
        if self.get_bool(arguments, "doc_comment") {
            args.push("--doc-comment".to_string());
        }

        // Add literal-only (boolean flag)
        if self.get_bool(arguments, "literal_only") {
            args.push("--literal-only".to_string());
        }

        // Add apply (boolean flag)
        if self.get_bool(arguments, "apply") {
            args.push("--apply".to_string());
        }
    }

    fn add_update_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Add paths (required)
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add name (required)
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add kind or node-type (mutually exclusive)
        if let Some(kind) = arguments.get("kind").and_then(|v| v.as_str()) {
            args.push("--kind".to_string());
            args.push(kind.to_string());
        } else if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add field
        if let Some(field) = arguments.get("field").and_then(|v| v.as_str()) {
            args.push("--field".to_string());
            args.push(field.to_string());
        }

        // Add variant
        if let Some(variant) = arguments.get("variant").and_then(|v| v.as_str()) {
            args.push("--variant".to_string());
            args.push(variant.to_string());
        }

        // Add match-arm
        if let Some(match_arm) = arguments.get("match_arm").and_then(|v| v.as_str()) {
            args.push("--match-arm".to_string());
            args.push(match_arm.to_string());
        }

        // Add body
        if let Some(body) = arguments.get("body").and_then(|v| v.as_str()) {
            args.push("--body".to_string());
            args.push(body.to_string());
        }

        // Add function
        if let Some(function) = arguments.get("function").and_then(|v| v.as_str()) {
            args.push("--function".to_string());
            args.push(function.to_string());
        }

        // Add doc-comment
        if let Some(doc_comment) = arguments.get("doc_comment").and_then(|v| v.as_str()) {
            args.push("--doc-comment".to_string());
            args.push(doc_comment.to_string());
        }

        // Add apply (boolean flag)
        if self.get_bool(arguments, "apply") {
            args.push("--apply".to_string());
        }
    }

    fn add_rename_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Add paths (required)
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }

        // Add name (required - current name)
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
        }

        // Add to (required - new name)
        if let Some(to) = arguments.get("to").and_then(|v| v.as_str()) {
            args.push("--to".to_string());
            args.push(to.to_string());
        }

        // Add kind or node-type (mutually exclusive)
        if let Some(kind) = arguments.get("kind").and_then(|v| v.as_str()) {
            args.push("--kind".to_string());
            args.push(kind.to_string());
        } else if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }

        // Add enum-path
        if let Some(enum_path) = arguments.get("enum_path").and_then(|v| v.as_str()) {
            args.push("--enum-path".to_string());
            args.push(enum_path.to_string());
        }

        // Add function-path
        if let Some(function_path) = arguments.get("function_path").and_then(|v| v.as_str()) {
            args.push("--function-path".to_string());
            args.push(function_path.to_string());
        }

        // Add edit-mode
        if let Some(edit_mode) = arguments.get("edit_mode").and_then(|v| v.as_str()) {
            args.push("--edit-mode".to_string());
            args.push(edit_mode.to_string());
        }

        // Add validate (default is true, so only add if explicitly set to false)
        if let Some(validate) = arguments.get("validate").and_then(|v| v.as_bool()) {
            if !validate {
                args.push("--no-validate".to_string());
            }
        }

        // Add apply (boolean flag)
        if self.get_bool(arguments, "apply") {
            args.push("--apply".to_string());
        }
    }
}
