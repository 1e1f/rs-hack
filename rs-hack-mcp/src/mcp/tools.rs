//! @arch:layer(mcp)
//! @arch:role(bridge)
//! @arch:role(discovery)
//! @arch:note(Each MCP tool maps 1:1 to an rs-hack CLI subcommand via subprocess)
//!
//! Tool registry: defines MCP tool schemas and executes them
//! by shelling out to the rs-hack CLI binary.

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

                // ============================================================
                // ARCHITECTURE TOOLS (4) - query @arch: annotations
                // Use these to understand codebase architecture before editing.
                // Requires @arch: annotations in doc comments and an optional
                // [workspace.metadata.arch] schema in Cargo.toml — bootstrap with
                // `rs-hack arch init --apply`. (Note: `rs-hack board init` is a
                // different thing — it installs hack-board slash commands.)
                // ============================================================
                Tool {
                    name: "arch_context",
                    description: "Get architectural context for a file. Returns layer, role, thread, QoS, constraints, and design notes/rationale extracted from @arch: annotations. USE THIS BEFORE EDITING a file to understand its architectural role and constraints. If a file has no annotations, the result will be empty — that's normal for unannotated repos.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "file": {"type": "string", "description": "File path to get context for (e.g., 'src/editor.rs')"},
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "format": {"type": "string", "enum": ["markdown", "json"], "description": "Output format (default: markdown)"}
                        },
                        "required": ["file"]
                    }),
                },
                Tool {
                    name: "arch_query",
                    description: "Query the architecture knowledge graph built from @arch: annotations. Find modules by layer, role, or other properties. Examples: 'layer:core', 'role:parser AND layer:core', 'file:editor'. Better than grep for architectural questions — returns only annotated nodes with their metadata.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "description": "Query using predicates: layer:X, role:X, thread:X, file:X, gateway, owns_voices. Combine with AND/OR/NOT."},
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "format": {"type": "string", "enum": ["ids", "verbose"], "description": "Output format: 'ids' (default) or 'verbose' (with file locations and properties)"}
                        },
                        "required": ["query"]
                    }),
                },
                Tool {
                    name: "arch_validate",
                    description: "Validate architecture rules defined in Cargo.toml [workspace.metadata.arch.rules]. Returns violations with file locations. Use after making changes to ensure architectural constraints (e.g., layer dependencies) are satisfied.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "include_schema_rules": {"type": "boolean", "default": true, "description": "Include auto-generated rules from layer dependency schema"}
                        }
                    }),
                },
                Tool {
                    name: "arch_schema",
                    description: "Show the architecture schema: valid layers, roles, threads, QoS classes, and message types defined in Cargo.toml [workspace.metadata.arch]. Use this to understand what annotations are available in a project.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "format": {"type": "string", "enum": ["text", "json"], "description": "Output format (default: text)"}
                        }
                    }),
                },

                // ============================================================
                // HACK-BOARD TOOLS (5) - source-embedded kanban / SDLC
                // Tickets and relays live as @hack: doc-comment annotations in
                // Rust source. Board state is branch-scoped; moving a ticket
                // is a source edit and shows up in `git diff`. Bootstrap a
                // project with `rs-hack board init`.
                // ============================================================
                Tool {
                    name: "board_status",
                    description: "One-shot board snapshot for planning. Returns per-column counts, tickets actively held (with owners — off-limits for refactor per R5), the handoff queue with one-line next steps, epic child rollups (done/active/handoff/open), pending todos from .hack/todo.md, and a smell signal for `disappeared` tickets in the event log. USE BEFORE picking up work or planning a new relay — it's the fastest orientation on 'what's in flight'.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "format": {"type": "string", "enum": ["markdown", "json"], "description": "Output format (default: markdown)"}
                        }
                    }),
                },
                Tool {
                    name: "board_tickets",
                    description: "List tickets/relays, or synthesize a pickup prompt. Default mode dumps all @hack: items grouped by column. Use `prompt` to get a continuation prompt for a specific ticket — the output includes the embedded SDLC playbook (R1, C1, R3, R4, R2) so a picking-up agent sees the rules before acting. Use `relay_doc` for a markdown doc of a relay. Filter with `status` / `assignee` / `epics_only`.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "format": {"type": "string", "enum": ["markdown", "json"], "description": "Output format for list mode (default: markdown)"},
                            "status": {"type": "string", "enum": ["open", "claimed", "in-progress", "handoff", "review", "done"], "description": "Filter by column/status"},
                            "assignee": {"type": "string", "description": "Filter by assignee (e.g., 'agent:claude')"},
                            "epics_only": {"type": "boolean", "description": "Only include epics (relays with @hack:kind(epic) or inferred children)"},
                            "prompt": {"type": "string", "description": "Synthesize a continuation prompt for this ticket ID (e.g., 'R001'). Includes embedded SDLC playbook."},
                            "relay_doc": {"type": "string", "description": "Synthesize a relay markdown doc for this ticket ID"}
                        }
                    }),
                },
                Tool {
                    name: "board_rules",
                    description: "Print the canonical hack-board SDLC ruleset (R1–R7 + C1 column rule). The same rules are embedded in every pickup prompt from `board_tickets` with `prompt`. USE THIS to orient on how work flows through the board when picking up a ticket or finishing a phase. Narrow by situation with `context` to get only the rules relevant right now.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "context": {"type": "string", "enum": ["pickup", "finishing", "new-work", "archive", "refactor"], "description": "Narrow ruleset to one situation. Omit for the full set."},
                            "format": {"type": "string", "enum": ["markdown", "json", "terse"], "description": "Output format: markdown (with why/apply), json, or terse (one-line rules). Default markdown."}
                        }
                    }),
                },
                Tool {
                    name: "board_claim",
                    description: "Atomically claim the next ticket / relay ID and write the @hack: annotation block. ALWAYS use this instead of picking an ID yourself — the command takes a file lock and scans source for the highest existing ID, so two agents running concurrently can't collide (SDLC rule R2). The annotation is written as a module-level doc-comment (`//! @hack:…`) at the top of `file`. Returns the claimed ID (e.g., 'R008', 'F03'). Set `json` for structured output with id/file/line.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "kind": {"type": "string", "enum": ["relay", "epic", "feature", "bug", "task"], "description": "Kind of item to create. `relay` for a new thread of work; `epic` for a coordination point that owns child relays; feature/bug/task for work units."},
                            "file": {"type": "string", "description": "Target source file where the annotation will be written"},
                            "title": {"type": "string", "description": "Short human-readable title"},
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"},
                            "assignee": {"type": "string", "description": "@hack:assignee(...) value (e.g., 'agent:claude')"},
                            "status": {"type": "string", "enum": ["open", "claimed", "in-progress", "handoff", "review", "done"], "description": "Initial status. Defaults to 'in-progress' for tickets, 'handoff' for relays."},
                            "phase": {"type": "string", "description": "@hack:phase(...) — e.g., 'P1'"},
                            "parent": {"type": "string", "description": "@hack:parent(RXXX) — parent relay ID (only set for epic children)"},
                            "severity": {"type": "string", "description": "@hack:severity(...) — bug-specific"},
                            "handoff_msg": {"type": "string", "description": "@hack:handoff(\"...\") — message describing what's done and what remains"},
                            "next": {"type": "array", "items": {"type": "string"}, "description": "@hack:next(\"...\") — concrete next steps (repeatable)"},
                            "verify": {"type": "array", "items": {"type": "string"}, "description": "@hack:verify(\"...\") — verification commands (repeatable)"},
                            "cleanup": {"type": "array", "items": {"type": "string"}, "description": "@hack:cleanup(\"...\") — deferred cleanup items (repeatable)"},
                            "see": {"type": "array", "items": {"type": "string"}, "description": "@arch:see(path) — architecture doc links (repeatable)"},
                            "json": {"type": "boolean", "description": "Emit {id, file, line} JSON instead of just the ID"}
                        },
                        "required": ["kind", "file", "title"]
                    }),
                },
                Tool {
                    name: "board_summary",
                    description: "Write a freeform progress summary to the hack-board inbox (`.hack/summaries/*.md`). Use for ad-hoc notes that aren't a full handoff — what you just did, what's blocking, gotchas for the next agent. A summary with a `ticket` attaches to that ticket's card; otherwise it lands in the board Inbox. Use this instead of creating a stale ticket annotation just to record progress.",
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "text": {"type": "string", "description": "Summary body (markdown). Pass '-' to read from stdin (CLI only — not useful here)."},
                            "ticket": {"type": "string", "description": "Ticket/relay ID this summary is attached to (e.g., 'R001'). Omit for inbox."},
                            "author": {"type": "string", "description": "Author tag (e.g., 'agent:claude')"},
                            "path": {"type": "string", "description": "Path to workspace root (default: current directory)"}
                        },
                        "required": ["text"]
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

        // Execute rs-hack command (command may be multi-word, e.g. "arch context")
        let cmd_parts: Vec<&str> = command.split_whitespace().collect();
        let output = Command::new("rs-hack")
            .args(&cmd_parts)
            .args(&args)
            .output()
            .map_err(|e| anyhow!("Failed to run rs-hack: {}. Is it installed?", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let result = stdout.trim().to_string();

            // If operation completed and apply was false, add reminder
            // (skip for read-only tools that don't have an apply parameter)
            let is_read_only = name.starts_with("arch_")
                || name == "find"
                || name == "history"
                || name == "board_status"
                || name == "board_tickets"
                || name == "board_rules";
            if !is_read_only && !self.get_bool(&arguments, "apply") && !result.is_empty() {
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
            // Architecture tools - map to "rs-hack arch <subcommand>"
            "arch_context" => {
                self.add_arch_context_args(&arguments, &mut args);
                "arch context"
            }
            "arch_query" => {
                self.add_arch_query_args(&arguments, &mut args);
                "arch query"
            }
            "arch_validate" => {
                self.add_arch_validate_args(&arguments, &mut args);
                "arch validate"
            }
            "arch_schema" => {
                self.add_arch_schema_args(&arguments, &mut args);
                "arch schema"
            }
            // hack-board tools - map to "rs-hack board <subcommand>"
            "board_status" => {
                self.add_board_status_args(&arguments, &mut args);
                "board status"
            }
            "board_tickets" => {
                self.add_board_tickets_args(&arguments, &mut args);
                "board tickets"
            }
            "board_rules" => {
                self.add_board_rules_args(&arguments, &mut args);
                "board rules"
            }
            "board_claim" => {
                self.add_board_claim_args(&arguments, &mut args);
                "board claim"
            }
            "board_summary" => {
                self.add_board_summary_args(&arguments, &mut args);
                "board summary"
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

    // ============================================================
    // Architecture tool argument builders
    // ============================================================

    fn add_arch_context_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // 'file' is a positional arg in the CLI
        if let Some(file) = arguments.get("file").and_then(|v| v.as_str()) {
            // path flag must come before the positional arg
            if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
                args.push("--path".to_string());
                args.push(path.to_string());
            }
            if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
                args.push("--format".to_string());
                args.push(format.to_string());
            }
            args.push(file.to_string());
        }
    }

    fn add_arch_query_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // 'query' is a positional arg in the CLI
        if let Some(query) = arguments.get("query").and_then(|v| v.as_str()) {
            if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
                args.push("--path".to_string());
                args.push(path.to_string());
            }
            if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
                args.push("--format".to_string());
                args.push(format.to_string());
            }
            args.push(query.to_string());
        }
    }

    fn add_arch_validate_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if self.get_bool(arguments, "include_schema_rules") {
            args.push("--include-schema-rules".to_string());
        }
    }

    fn add_arch_schema_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
        }
    }

    // ============================================================
    // hack-board args
    // ============================================================

    fn add_board_status_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
        }
    }

    fn add_board_tickets_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
        }
        if let Some(status) = arguments.get("status").and_then(|v| v.as_str()) {
            args.push("--status".to_string());
            args.push(status.to_string());
        }
        if let Some(assignee) = arguments.get("assignee").and_then(|v| v.as_str()) {
            args.push("--assignee".to_string());
            args.push(assignee.to_string());
        }
        if self.get_bool(arguments, "epics_only") {
            args.push("--epics".to_string());
        }
        if let Some(prompt) = arguments.get("prompt").and_then(|v| v.as_str()) {
            args.push("--prompt".to_string());
            args.push(prompt.to_string());
        }
        if let Some(relay_doc) = arguments.get("relay_doc").and_then(|v| v.as_str()) {
            args.push("--relay-doc".to_string());
            args.push(relay_doc.to_string());
        }
    }

    fn add_board_rules_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(context) = arguments.get("context").and_then(|v| v.as_str()) {
            args.push("--context".to_string());
            args.push(context.to_string());
        }
        if let Some(format) = arguments.get("format").and_then(|v| v.as_str()) {
            args.push("--format".to_string());
            args.push(format.to_string());
        }
    }

    fn add_board_claim_args(&self, arguments: &Value, args: &mut Vec<String>) {
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if let Some(kind) = arguments.get("kind").and_then(|v| v.as_str()) {
            args.push("--kind".to_string());
            args.push(kind.to_string());
        }
        if let Some(file) = arguments.get("file").and_then(|v| v.as_str()) {
            args.push("--file".to_string());
            args.push(file.to_string());
        }
        if let Some(title) = arguments.get("title").and_then(|v| v.as_str()) {
            args.push("--title".to_string());
            args.push(title.to_string());
        }
        if let Some(assignee) = arguments.get("assignee").and_then(|v| v.as_str()) {
            args.push("--assignee".to_string());
            args.push(assignee.to_string());
        }
        if let Some(status) = arguments.get("status").and_then(|v| v.as_str()) {
            args.push("--status".to_string());
            args.push(status.to_string());
        }
        if let Some(phase) = arguments.get("phase").and_then(|v| v.as_str()) {
            args.push("--phase".to_string());
            args.push(phase.to_string());
        }
        if let Some(parent) = arguments.get("parent").and_then(|v| v.as_str()) {
            args.push("--parent".to_string());
            args.push(parent.to_string());
        }
        if let Some(severity) = arguments.get("severity").and_then(|v| v.as_str()) {
            args.push("--severity".to_string());
            args.push(severity.to_string());
        }
        if let Some(handoff) = arguments.get("handoff_msg").and_then(|v| v.as_str()) {
            args.push("--handoff".to_string());
            args.push(handoff.to_string());
        }
        for key in ["next", "verify", "cleanup", "see"] {
            if let Some(arr) = arguments.get(key).and_then(|v| v.as_array()) {
                let flag = format!("--{}", key);
                for item in arr {
                    if let Some(s) = item.as_str() {
                        args.push(flag.clone());
                        args.push(s.to_string());
                    }
                }
            }
        }
        if self.get_bool(arguments, "json") {
            args.push("--json".to_string());
        }
    }

    fn add_board_summary_args(&self, arguments: &Value, args: &mut Vec<String>) {
        // Positional `text` comes last; flags before.
        if let Some(ticket) = arguments.get("ticket").and_then(|v| v.as_str()) {
            args.push("--ticket".to_string());
            args.push(ticket.to_string());
        }
        if let Some(author) = arguments.get("author").and_then(|v| v.as_str()) {
            args.push("--author".to_string());
            args.push(author.to_string());
        }
        if let Some(path) = arguments.get("path").and_then(|v| v.as_str()) {
            args.push("--path".to_string());
            args.push(path.to_string());
        }
        if let Some(text) = arguments.get("text").and_then(|v| v.as_str()) {
            args.push(text.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_tools_are_registered() {
        let reg = ToolRegistry::new();
        let names: Vec<&str> = reg.list().iter().map(|t| t.name).collect();
        for expected in [
            "board_status",
            "board_tickets",
            "board_rules",
            "board_claim",
            "board_summary",
        ] {
            assert!(
                names.contains(&expected),
                "{} missing from tool registry. Registered: {:?}",
                expected,
                names
            );
        }
    }

    #[test]
    fn board_tickets_builds_prompt_command() {
        let reg = ToolRegistry::new();
        let (cmd, args) = reg
            .build_command(
                "board_tickets",
                &json!({"prompt": "R001", "path": "/tmp/ws"}),
            )
            .unwrap();
        assert_eq!(cmd, "board tickets");
        assert!(args.contains(&"--prompt".to_string()));
        assert!(args.contains(&"R001".to_string()));
        assert!(args.contains(&"--path".to_string()));
    }

    #[test]
    fn board_rules_passes_context() {
        let reg = ToolRegistry::new();
        let (cmd, args) = reg
            .build_command("board_rules", &json!({"context": "pickup"}))
            .unwrap();
        assert_eq!(cmd, "board rules");
        assert_eq!(args, vec!["--context".to_string(), "pickup".to_string()]);
    }

    #[test]
    fn board_claim_passes_repeatable_next() {
        let reg = ToolRegistry::new();
        let (cmd, args) = reg
            .build_command(
                "board_claim",
                &json!({
                    "kind": "relay",
                    "file": "src/mod.rs",
                    "title": "test",
                    "next": ["first step", "second step"],
                }),
            )
            .unwrap();
        assert_eq!(cmd, "board claim");
        // Two --next flags, each followed by its value
        let next_count = args.iter().filter(|a| a.as_str() == "--next").count();
        assert_eq!(next_count, 2);
        assert!(args.contains(&"first step".to_string()));
        assert!(args.contains(&"second step".to_string()));
    }

    #[test]
    fn board_summary_positional_text_comes_last() {
        let reg = ToolRegistry::new();
        let (_, args) = reg
            .build_command(
                "board_summary",
                &json!({"text": "did some work", "ticket": "R001"}),
            )
            .unwrap();
        assert_eq!(args.last().map(|s| s.as_str()), Some("did some work"));
        assert!(args.contains(&"--ticket".to_string()));
    }
}
