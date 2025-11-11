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
                            "path": {
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
                        "required": ["path"]
                    }),
                },
                Tool {
                    name: "inspect_match_arms",
                    description: "Inspect match expression arms in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Optional pattern to match (e.g., \"Status::Active\")"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["path"]
                    }),
                },
                Tool {
                    name: "inspect_enum_usage",
                    description: "Find all usages of an enum variant in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Enum variant to find (e.g., \"Operator::PropagateError\")"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["path", "name"]
                    }),
                },
                Tool {
                    name: "inspect_macro_calls",
                    description: "Find macro invocations in Rust files",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path or glob pattern"},
                            "name": {"type": "string", "description": "Macro name (e.g., \"eprintln\", \"todo\")"},
                            "content_filter": {"type": "string", "description": "Optional content to filter by"},
                            "format": {"type": "string", "enum": ["snippets", "locations", "json"], "default": "snippets"}
                        },
                        "required": ["path", "name"]
                    }),
                },
                // Struct operations
                Tool {
                    name: "add_struct_field",
                    description: "Add a field to Rust struct definitions and/or literals",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path or glob pattern"},
                            "struct_name": {"type": "string", "description": "Name of the struct"},
                            "field": {"type": "string", "description": "Field definition (e.g., \"email: String\")"},
                            "position": {"type": "string", "description": "Where to add (\"after:field_name\", \"before:field_name\", or \"Last\")"},
                            "literal_default": {"type": "string", "description": "If provided, also update struct literals with this default value"},
                            "apply": {"type": "boolean", "default": false, "description": "If true, make actual changes. If false, show preview only."}
                        },
                        "required": ["path", "struct_name", "field"]
                    }),
                },
                Tool {
                    name: "update_struct_field",
                    description: "Update an existing struct field (change type or visibility)",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "struct_name": {"type": "string"},
                            "field": {"type": "string", "description": "New field definition"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "struct_name", "field"]
                    }),
                },
                // Enum operations
                Tool {
                    name: "add_enum_variant",
                    description: "Add a variant to a Rust enum",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "variant": {"type": "string", "description": "Variant definition (e.g., \"Pending\" or \"Error { code: i32 }\")"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "enum_name", "variant"]
                    }),
                },
                Tool {
                    name: "rename_enum_variant",
                    description: "Rename an enum variant throughout the entire codebase",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "File path or glob pattern (usually \"src/**/*.rs\")"},
                            "enum_name": {"type": "string"},
                            "old_variant": {"type": "string"},
                            "new_variant": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "enum_name", "old_variant", "new_variant"]
                    }),
                },
                // Match operations
                Tool {
                    name: "add_match_arm",
                    description: "Add a match arm to match expressions",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "pattern": {"type": "string"},
                            "body": {"type": "string"},
                            "function": {"type": "string"},
                            "enum_name": {"type": "string"},
                            "auto_detect": {"type": "boolean", "default": false, "description": "If true, automatically add all missing enum variants"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "pattern", "body"]
                    }),
                },
                // Transform
                Tool {
                    name: "transform",
                    description: "Generic transformation tool - find and modify any AST nodes",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "node_type": {"type": "string", "enum": ["macro-call", "method-call", "function-call", "enum-usage", "struct-literal", "match-arm", "identifier", "type-ref"]},
                            "action": {"type": "string", "enum": ["comment", "remove", "replace"]},
                            "name": {"type": "string"},
                            "content_filter": {"type": "string"},
                            "replacement": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "node_type", "action"]
                    }),
                },
                // Other operations
                Tool {
                    name: "add_derive",
                    description: "Add derive macros to structs or enums",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "target_type": {"type": "string", "enum": ["struct", "enum"]},
                            "name": {"type": "string"},
                            "derives": {"type": "string", "description": "Comma-separated derives (e.g., \"Clone,Debug,Serialize\")"},
                            "where_filter": {"type": "string"},
                            "apply": {"type": "boolean", "default": false}
                        },
                        "required": ["path", "target_type", "name", "derives"]
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

        // Build rs-hack command
        let mut args = vec![name.replace('_', "-")];

        // Convert JSON arguments to command-line args
        self.args_to_cli(&arguments, &mut args);

        debug!("Running: rs-hack {}", args.join(" "));

        // Execute rs-hack command
        let output = Command::new("rs-hack")
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

    fn args_to_cli(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(obj) = arguments.as_object() {
            for (key, value) in obj {
                let flag = format!("--{}", key.replace('_', "-"));

                match value {
                    Value::Bool(true) => args.push(flag),
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
