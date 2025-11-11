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
                // INSPECTION TOOLS (2)
                // ============================================================
                Tool {
                    name: "inspect",
                    description: "Generic inspection tool - find and list any AST nodes with optional comment extraction. Expression-level: struct-literal, match-arm, enum-usage, function-call, method-call, macro-call, identifier, type-ref. Definition-level: struct, enum, function, impl-method, trait, const, static, type-alias, mod",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"},
                            "node_type": {
                                "type": "string",
                                "enum": ["struct-literal", "match-arm", "enum-usage", "function-call", "method-call", "macro-call", "identifier", "type-ref", "struct", "enum", "function", "impl-method", "trait", "const", "static", "type-alias", "mod"],
                                "description": "Type of AST node to inspect"
                            },
                            "name": {"type": "string", "description": "Optional name filter (e.g., \"Shadow\", \"Operator::Error\", \"unwrap\", \"MyStruct\", \"my_function\")"},
                            "content_filter": {"type": "string", "description": "Filter by content substring"},
                            "include_comments": {"type": "boolean", "default": true, "description": "Include preceding comments (doc and regular) in output"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["paths", "node_type"]
                    }),
                },
                Tool {
                    name: "find",
                    description: "Find locations of AST node definitions (struct, enum, function) - useful for debugging",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "node_type": {"type": "string", "enum": ["struct", "enum", "function"], "description": "Type of definition to find"},
                            "name": {"type": "string", "description": "Name of the item to find"}
                        },
                        "required": ["paths", "node_type", "name"]
                    }),
                },

                // ============================================================
                // STRUCT OPERATIONS (3)
                // ============================================================
                Tool {
                    name: "add_struct_field",
                    description: "Add a field to struct definitions and/or literals",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "struct_name": {"type": "string"},
                            "field": {"type": "string", "description": "Field definition (e.g., \"email: String\")"},
                            "position": {"type": "string", "description": "Position: \"first\", \"last\", \"after:field_name\""},
                            "literal_default": {"type": "string", "description": "Default value for struct literals"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "struct_name", "field"]
                    }),
                },
                Tool {
                    name: "update_struct_field",
                    description: "Update an existing struct field (change type or visibility)",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "struct_name": {"type": "string"},
                            "field": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "struct_name", "field"]
                    }),
                },
                Tool {
                    name: "remove_struct_field",
                    description: "Remove a field from a struct",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "struct_name": {"type": "string"},
                            "field_name": {"type": "string", "description": "Name of field to remove"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "struct_name", "field_name"]
                    }),
                },

                // ============================================================
                // ENUM OPERATIONS (4)
                // ============================================================
                Tool {
                    name: "add_enum_variant",
                    description: "Add a variant to an enum",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "variant": {"type": "string", "description": "Variant definition (e.g., \"Pending\" or \"Error { code: i32 }\")"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "enum_name", "variant"]
                    }),
                },
                Tool {
                    name: "update_enum_variant",
                    description: "Update an existing enum variant",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "variant": {"type": "string", "description": "New variant definition"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "enum_name", "variant"]
                    }),
                },
                Tool {
                    name: "remove_enum_variant",
                    description: "Remove a variant from an enum",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "variant_name": {"type": "string", "description": "Name of variant to remove"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "enum_name", "variant_name"]
                    }),
                },
                Tool {
                    name: "rename_enum_variant",
                    description: "Rename an enum variant throughout the entire codebase",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "old_variant": {"type": "string"},
                            "new_variant": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "enum_name", "old_variant", "new_variant"]
                    }),
                },

                // ============================================================
                // MATCH OPERATIONS (3)
                // ============================================================
                Tool {
                    name: "add_match_arm",
                    description: "Add a match arm to match expressions (supports auto-detect for missing variants)",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "pattern": {"type": "string"},
                            "body": {"type": "string"},
                            "function": {"type": "string", "description": "Function containing the match"},
                            "enum_name": {"type": "string", "description": "Enum name for auto-detect"},
                            "auto_detect": {"type": "boolean", "default": false},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "pattern", "body"]
                    }),
                },
                Tool {
                    name: "update_match_arm",
                    description: "Update an existing match arm",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "pattern": {"type": "string"},
                            "body": {"type": "string", "description": "New match arm body"},
                            "function": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "pattern", "body"]
                    }),
                },
                Tool {
                    name: "remove_match_arm",
                    description: "Remove a match arm from match expressions",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "pattern": {"type": "string", "description": "Pattern to remove"},
                            "function": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "pattern"]
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
                // CODE ORGANIZATION (3)
                // ============================================================
                Tool {
                    name: "add_derive",
                    description: "Add derive macros to structs or enums",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum"]},
                            "name": {"type": "string", "description": "Name of the struct or enum"},
                            "derives": {"type": "string", "description": "Comma-separated derives (e.g., \"Clone,Debug,Serialize\")"},
                            "where": {"type": "string", "description": "Filter by traits (e.g., \"derives_trait:Clone\")"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target_type", "name", "derives"]
                    }),
                },
                Tool {
                    name: "add_impl_method",
                    description: "Add a method to an impl block",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target": {"type": "string", "description": "Target struct/enum name"},
                            "method": {"type": "string", "description": "Method definition (e.g., \"pub fn get_id(&self) -> u64 { self.id }\")"},
                            "position": {"type": "string", "description": "Position: \"first\", \"last\", \"after:method_name\""},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target", "method"]
                    }),
                },
                Tool {
                    name: "add_use",
                    description: "Add a use statement to a file",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "use_path": {"type": "string", "description": "Use path (e.g., \"serde::Serialize\")"},
                            "position": {"type": "string", "description": "Position hint (e.g., \"after:collections\")"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "use_path"]
                    }),
                },

                // ============================================================
                // DOCUMENTATION (3)
                // ============================================================
                Tool {
                    name: "add_doc_comment",
                    description: "Add documentation comment to an item (struct, enum, function)",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum", "function"]},
                            "name": {"type": "string", "description": "Name of the item"},
                            "doc_comment": {"type": "string", "description": "Documentation text"},
                            "style": {"type": "string", "enum": ["line", "block"], "default": "line", "description": "Comment style: line (///) or block (/** */)"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target_type", "name", "doc_comment"]
                    }),
                },
                Tool {
                    name: "update_doc_comment",
                    description: "Update existing documentation comment",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum", "function"]},
                            "name": {"type": "string"},
                            "doc_comment": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target_type", "name", "doc_comment"]
                    }),
                },
                Tool {
                    name: "remove_doc_comment",
                    description: "Remove documentation comment from an item",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum", "function"]},
                            "name": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target_type", "name"]
                    }),
                },

                // ============================================================
                // REFACTORING (1)
                // ============================================================
                Tool {
                    name: "rename_function",
                    description: "Rename a function across the entire codebase",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "old_name": {"type": "string", "description": "Current function name"},
                            "new_name": {"type": "string", "description": "New function name"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "old_name", "new_name"]
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
            // Inspection uses "inspect" command
            "inspect" => {
                self.add_inspect_args(&arguments, &mut args);
                "inspect"
            }
            // Find uses "find" command
            "find" => {
                self.add_find_args(&arguments, &mut args);
                "find"
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

    fn add_inspect_args(&self, arguments: &Value, args: &mut Vec<String>) {
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
    }

    fn add_find_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(paths) = arguments.get("paths").and_then(|v| v.as_str()) {
            args.push("--paths".to_string());
            args.push(paths.to_string());
        }
        if let Some(node_type) = arguments.get("node_type").and_then(|v| v.as_str()) {
            args.push("--node-type".to_string());
            args.push(node_type.to_string());
        }
        if let Some(name) = arguments.get("name").and_then(|v| v.as_str()) {
            args.push("--name".to_string());
            args.push(name.to_string());
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
                        if key == "apply" {
                            args.push("--apply".to_string());
                        }
                    }
                    Value::Bool(false) => {}, // Skip false booleans
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
}
