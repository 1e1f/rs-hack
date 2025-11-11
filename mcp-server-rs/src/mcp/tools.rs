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
                // Inspection tools
                Tool {
                    name: "inspect_struct_literals",
                    description: "Inspect struct literal initializations in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {
                                "type": "string",
                                "description": "File path or glob pattern (e.g., \"src/**/*.rs\")"
                            },
                            "name": {
                                "type": "string",
                                "description": "Optional struct name to filter (supports patterns like \"*::Rectangle\")"
                            },
                            "format": {
                                "type": "string",
                                "enum": ["snippets", "locations", "json"],
                                "default": "snippets",
                                "description": "Output format"
                            }
                        },
                        "required": ["paths"]
                    }),
                },
                Tool {
                    name: "inspect_match_arms",
                    description: "Inspect match expression arms in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Optional pattern to match (e.g., \"Status::Active\")"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["paths"]
                    }),
                },
                Tool {
                    name: "inspect_enum_usage",
                    description: "Find all usages of an enum variant in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Enum variant to find (e.g., \"Operator::PropagateError\")"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["paths", "name"]
                    }),
                },
                Tool {
                    name: "inspect_macro_calls",
                    description: "Find macro invocations in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Macro name (e.g., \"eprintln\", \"todo\")"},
                            "content_filter": {"type": "string", "description": "Optional content to filter by"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["paths", "name"]
                    }),
                },
                // Struct operations
                Tool {
                    name: "add_struct_field",
                    description: "Add a field to Rust struct definitions and/or literals",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern"},
                            "struct_name": {"type": "string", "description": "Name of the struct"},
                            "field": {"type": "string", "description": "Field definition (e.g., \"email: String\")"},
                            "position": {"type": "string", "description": "Where to add (\"after:field_name\", \"before:field_name\", or \"last\")"},
                            "literal_default": {"type": "string", "description": "If provided, also update struct literals with this default value"},
                            "apply": {"type": "boolean", "default": false, "description": "If true, make actual changes. If false, show preview only."}
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
                            "field": {"type": "string", "description": "New field definition"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "struct_name", "field"]
                    }),
                },
                // Enum operations
                Tool {
                    name: "add_enum_variant",
                    description: "Add a variant to a Rust enum",
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
                    name: "rename_enum_variant",
                    description: "Rename an enum variant throughout the entire codebase",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string", "description": "File path or glob pattern (usually \"src/**/*.rs\")"},
                            "enum_name": {"type": "string"},
                            "old_variant": {"type": "string"},
                            "new_variant": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "enum_name", "old_variant", "new_variant"]
                    }),
                },
                // Match operations
                Tool {
                    name: "add_match_arm",
                    description: "Add a match arm to match expressions",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "pattern": {"type": "string"},
                            "body": {"type": "string"},
                            "function": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "auto_detect": {"type": "boolean", "default": false, "description": "If true, automatically add all missing enum variants"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "pattern", "body"]
                    }),
                },
                // Transform
                Tool {
                    name: "transform",
                    description: "Generic transformation tool - find and modify any AST nodes",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "node_type": {"type": "string", "enum": ["macro-call", "method-call", "function-call", "enum-usage", "struct-literal", "match-arm", "identifier", "type-ref"]},
                            "action": {"type": "string", "enum": ["comment", "remove", "replace"]},
                            "name": {"type": "string"},
                            "content_filter": {"type": "string"},
                            "with": {"type": "string", "description": "Replacement code (required if action is replace)"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "node_type", "action"]
                    }),
                },
                // Other operations
                Tool {
                    name: "add_derive",
                    description: "Add derive macros to structs or enums",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "paths": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum"]},
                            "name": {"type": "string"},
                            "derives": {"type": "string", "description": "Comma-separated derives (e.g., \"Clone,Debug,Serialize\")"},
                            "where": {"type": "string", "description": "Filter by traits (e.g., \"derives_trait:Clone\")"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["paths", "target_type", "name", "derives"]
                    }),
                },
                // History and revert
                Tool {
                    name: "show_history",
                    description: "Show recent rs-hack operations",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "limit": {"type": "integer", "default": 10, "description": "Number of recent operations to show"}
                        }
                    }),
                },
                Tool {
                    name: "revert_operation",
                    description: "Revert a previous rs-hack operation",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "run_id": {"type": "string", "description": "The run ID from history (7-character hash)"},
                            "force": {"type": "boolean", "default": false, "description": "If true, revert even if files have changed since"}
                        },
                        "required": ["run_id"]
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
            // Inspection tools all use "inspect" command
            "inspect_struct_literals" => {
                args.push("--node-type".to_string());
                args.push("struct-literal".to_string());
                self.add_inspect_args(&arguments, &mut args);
                "inspect"
            }
            "inspect_match_arms" => {
                args.push("--node-type".to_string());
                args.push("match-arm".to_string());
                self.add_inspect_args(&arguments, &mut args);
                "inspect"
            }
            "inspect_enum_usage" => {
                args.push("--node-type".to_string());
                args.push("enum-usage".to_string());
                self.add_inspect_args(&arguments, &mut args);
                "inspect"
            }
            "inspect_macro_calls" => {
                args.push("--node-type".to_string());
                args.push("macro-call".to_string());
                self.add_inspect_args(&arguments, &mut args);
                "inspect"
            }
            // Transform command
            "transform" => {
                self.add_transform_args(&arguments, &mut args)?;
                "transform"
            }
            // History/revert
            "show_history" => {
                if let Some(limit) = arguments.get("limit").and_then(|v| v.as_i64()) {
                    args.push("--limit".to_string());
                    args.push(limit.to_string());
                }
                "history"
            }
            "revert_operation" => {
                if let Some(run_id) = arguments.get("run_id").and_then(|v| v.as_str()) {
                    args.push(run_id.to_string());
                }
                if self.get_bool(arguments, "force") {
                    args.push("--force".to_string());
                }
                "revert"
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

        // Add format
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
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
                        // Only add flag if it's not "apply" - we handle apply separately
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
