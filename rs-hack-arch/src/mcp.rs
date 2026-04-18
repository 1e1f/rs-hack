//! @arch:layer(arch)
//! @arch:role(bridge)
//! @hack:ticket(R001-T1, "Promote: board writes relay annotation back to source via rs-hack MCP")
//! @hack:phase(P2)
//! @hack:status(open)
//!
//! MCP (Model Context Protocol) integration for rs-hack-arch.
//! Exposes architecture queries as tools for Claude and other AI assistants.
//!
//! ## Tools
//!
//! - `arch_query`: Query the architecture graph
//! - `arch_trace`: Find paths between nodes
//! - `arch_context`: Get architectural context for a file
//! - `arch_validate`: Validate architecture rules
//!
//! ## Integration with rs-hack-mcp
//!
//! These tools can be added to rs-hack-mcp by adding this crate as a dependency
//! and registering the tools in the MCP server.

use crate::extract::extract_from_workspace;
use crate::graph::ArchGraph;
use crate::query::{get_file_context, trace_path, Query};
use crate::summary;
use crate::ticket::TicketBoard;
use crate::validate::{load_rules_from_metadata, validate, Severity};
use serde::{Deserialize, Serialize};

/// Tool definitions for MCP registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result from a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
}

impl ToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text { text: s.into() }],
            is_error: None,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text { text: s.into() }],
            is_error: Some(true),
        }
    }
}

/// Get all tool definitions for MCP registration.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "arch_query".into(),
            description: "Query the architecture knowledge graph. Examples: 'layer:vivarium', 'role:synthesis AND thread:audio', 'gateway', 'produces:impulse:NoteOn'".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Query string using predicates like layer:X, role:X, thread:X, produces:type:name, consumes:type:name, gateway, owns_voices"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["ids", "verbose"],
                        "description": "Output format (default: ids)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "arch_trace".into(),
            description: "Find paths between nodes in the architecture graph. Useful for understanding data flow.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "Source query (e.g., 'role:bridge')"
                    },
                    "to": {
                        "type": "string",
                        "description": "Target query (e.g., 'role:synthesis')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    }
                },
                "required": ["from", "to"]
            }),
        },
        ToolDefinition {
            name: "arch_context".into(),
            description: "Get architectural context for a file. Returns layer, role, thread, QoS, constraints, and documentation/rationale. Use this before editing a file to understand architectural requirements and design decisions.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path to get context for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "json"],
                        "description": "Output format (default: markdown)"
                    }
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "hack_tickets".into(),
            description: "Scan workspace for @hack:ticket and @hack:relay annotations. Returns a kanban board (Epics/Open/Active/Handoff/Review) of all work items with status, assignee, phase, parent, severity, and handoff messages. Compound sub-ticket IDs (e.g. R007-T1) are recognized and their parent is inferred from the prefix. Use format=json for web UI consumption.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["markdown", "json"],
                        "description": "Output format (default: markdown)"
                    },
                    "status": {
                        "type": "string",
                        "description": "Filter by status (open, claimed, in-progress, handoff, review, done)"
                    },
                    "assignee": {
                        "type": "string",
                        "description": "Filter by assignee (e.g., agent:claude)"
                    }
                }
            }),
        },
        ToolDefinition {
            name: "hack_summary".into(),
            description: "Write a freeform summary for the hack-board. Use this when you want to record what you did, what's left, or anything the next agent should know. The board can later promote this to a structured relay ticket. Low friction — just dump your thoughts as markdown.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Freeform markdown summary of what you did, what's left, gotchas, etc."
                    },
                    "ticket": {
                        "type": "string",
                        "description": "Ticket ID to link this summary to — bare (e.g. T03, R012) or compound (e.g. R007-T1). Optional — orphan summaries go to the board inbox."
                    },
                    "author": {
                        "type": "string",
                        "description": "Who wrote this (e.g., agent:claude). Optional."
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    }
                },
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "arch_validate".into(),
            description: "Validate architecture rules. Returns violations with file locations. Use after making changes to ensure architecture constraints are satisfied.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to workspace root (default: current directory)"
                    },
                    "rules_file": {
                        "type": "string",
                        "description": "Path to custom rules TOML file (default: rules from Cargo.toml metadata)"
                    }
                }
            }),
        },
    ]
}

/// Handle a tool invocation.
pub fn handle_tool(name: &str, args: serde_json::Value) -> ToolResult {
    match name {
        "arch_query" => handle_query(args),
        "arch_trace" => handle_trace(args),
        "arch_context" => handle_context(args),
        "arch_validate" => handle_validate(args),
        "hack_tickets" => handle_tickets(args),
        "hack_summary" => handle_summary(args),
        _ => ToolResult::error(format!("Unknown tool: {}", name)),
    }
}

fn handle_query(args: serde_json::Value) -> ToolResult {
    let query = args["query"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or(".");
    let format = args["format"].as_str().unwrap_or("ids");

    let annotations = match extract_from_workspace(path) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(format!("Failed to extract annotations: {}", e)),
    };

    let graph = ArchGraph::from_annotations(annotations);

    let q = match Query::parse(query) {
        Ok(q) => q,
        Err(e) => return ToolResult::error(format!("Invalid query: {}", e)),
    };

    let result = q.execute(&graph);

    match format {
        "verbose" => {
            let mut output = String::new();
            for id in &result.nodes {
                if let Some(node) = graph.get_node(id) {
                    output.push_str(&format!("{}:{} - {}\n", node.file.display(), node.line, node.id));
                    if let Some(ref layer) = node.properties.layer {
                        output.push_str(&format!("  layer: {}\n", layer));
                    }
                    if !node.properties.roles.is_empty() {
                        output.push_str(&format!("  roles: {}\n", node.properties.roles.join(", ")));
                    }
                    output.push('\n');
                }
            }
            output.push_str(&format!("{} matches", result.count));
            ToolResult::text(output)
        }
        _ => {
            let output = result.nodes.join("\n");
            ToolResult::text(format!("{}\n\n{} matches", output, result.count))
        }
    }
}

fn handle_trace(args: serde_json::Value) -> ToolResult {
    let from = args["from"].as_str().unwrap_or("");
    let to = args["to"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or(".");

    let annotations = match extract_from_workspace(path) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(format!("Failed to extract annotations: {}", e)),
    };

    let graph = ArchGraph::from_annotations(annotations);

    match trace_path(&graph, from, to) {
        Ok(paths) => {
            if paths.is_empty() {
                ToolResult::text(format!("No paths found from '{}' to '{}'", from, to))
            } else {
                let mut output = String::new();
                for (i, path) in paths.iter().enumerate() {
                    output.push_str(&format!("Path {}:\n", i + 1));
                    for (j, node) in path.iter().enumerate() {
                        if j > 0 {
                            output.push_str("  ↓\n");
                        }
                        output.push_str(&format!("  {}\n", node));
                    }
                    output.push('\n');
                }
                ToolResult::text(output)
            }
        }
        Err(e) => ToolResult::error(e),
    }
}

fn handle_context(args: serde_json::Value) -> ToolResult {
    let file = args["file"].as_str().unwrap_or("");
    let path = args["path"].as_str().unwrap_or(".");
    let format = args["format"].as_str().unwrap_or("markdown");

    let annotations = match extract_from_workspace(path) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(format!("Failed to extract annotations: {}", e)),
    };

    let graph = ArchGraph::from_annotations(annotations);
    let context = get_file_context(&graph, file);

    match format {
        "json" => {
            let json = serde_json::json!({
                "file": file,
                "layer": context.layer,
                "roles": context.roles,
                "thread": context.thread,
                "qos": context.qos,
                "produces": context.produces,
                "consumes": context.consumes,
                "patterns": context.patterns,
                "constraints": context.constraints,
                "notes": context.notes,
                "see_also": context.see_also,
                "doc": context.doc,
            });
            ToolResult::text(serde_json::to_string_pretty(&json).unwrap_or_default())
        }
        _ => ToolResult::text(context.to_markdown(file)),
    }
}

fn handle_validate(args: serde_json::Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or(".");
    let rules_file = args["rules_file"].as_str();

    let annotations = match extract_from_workspace(path) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(format!("Failed to extract annotations: {}", e)),
    };

    let graph = ArchGraph::from_annotations(annotations);

    // Load rules from file or from Cargo.toml metadata
    let rules = if let Some(rules_path) = rules_file {
        match std::fs::read_to_string(rules_path) {
            Ok(content) => match crate::validate::load_rules(&content) {
                Ok(r) => r,
                Err(e) => return ToolResult::error(format!("Failed to parse rules: {}", e)),
            },
            Err(e) => return ToolResult::error(format!("Failed to read rules file: {}", e)),
        }
    } else {
        // Try to load from Cargo.toml metadata
        match load_rules_from_metadata(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Note: Could not load rules from Cargo.toml: {}", e);
                Vec::new()
            }
        }
    };

    if rules.is_empty() {
        return ToolResult::text("No architecture rules defined. Add rules to [workspace.metadata.arch.rules] in Cargo.toml");
    }

    let violations = validate(&graph, &rules);

    if violations.is_empty() {
        ToolResult::text("✓ No violations found")
    } else {
        let mut output = String::new();
        let errors = violations.iter().filter(|v| v.severity == Severity::Error).count();
        let warnings = violations.iter().filter(|v| v.severity == Severity::Warning).count();

        for v in &violations {
            let prefix = match v.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARNING",
            };
            let location = match (&v.file, v.line) {
                (Some(f), Some(l)) => format!("{}:{}", f.display(), l),
                (Some(f), None) => f.display().to_string(),
                _ => "unknown".into(),
            };
            output.push_str(&format!("{}: {} - {} ({})\n", prefix, v.rule, v.message, location));
        }

        if errors > 0 {
            output.push_str(&format!("\n✗ {} errors, {} warnings", errors, warnings));
        } else {
            output.push_str(&format!("\n⚠ {} warnings", warnings));
        }

        ToolResult::text(output)
    }
}

fn handle_tickets(args: serde_json::Value) -> ToolResult {
    let path = args["path"].as_str().unwrap_or(".");
    let format = args["format"].as_str().unwrap_or("markdown");
    let status_filter = args["status"].as_str();
    let assignee_filter = args["assignee"].as_str();

    let annotations = match extract_from_workspace(path) {
        Ok(a) => a,
        Err(e) => return ToolResult::error(format!("Failed to extract annotations: {}", e)),
    };

    let board = TicketBoard::from_annotations(&annotations);

    // Apply filters
    let tickets: Vec<_> = board
        .tickets
        .iter()
        .filter(|t| {
            if let Some(sf) = status_filter {
                let expected = crate::ticket::TicketStatus::parse(sf);
                if t.status != expected {
                    return false;
                }
            }
            if let Some(af) = assignee_filter {
                if t.assignee.as_deref() != Some(af) {
                    return false;
                }
            }
            true
        })
        .collect();

    if tickets.is_empty() {
        return ToolResult::text("No tickets found");
    }

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&tickets).unwrap_or_default();
            ToolResult::text(json)
        }
        _ => {
            // Build a filtered board for markdown output
            let filtered = TicketBoard {
                tickets: tickets.into_iter().cloned().collect(),
            };
            ToolResult::text(filtered.to_markdown())
        }
    }
}

fn handle_summary(args: serde_json::Value) -> ToolResult {
    let text = match args["text"].as_str() {
        Some(t) => t,
        None => return ToolResult::error("Missing required 'text' parameter"),
    };
    let ticket = args["ticket"].as_str();
    let author = args["author"].as_str();
    let path = args["path"].as_str().unwrap_or(".");

    match summary::write_summary(std::path::Path::new(path), text, ticket, author) {
        Ok(s) => {
            let mut msg = format!("Summary written: {}", s.file.display());
            if let Some(ref tid) = s.ticket {
                msg.push_str(&format!("\nLinked to ticket: {}", tid));
            } else {
                msg.push_str("\nNo ticket linked (will appear in board inbox)");
            }
            msg.push_str(&format!("\nID: {}", s.id));
            ToolResult::text(msg)
        }
        Err(e) => ToolResult::error(format!("Failed to write summary: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions() {
        let defs = tool_definitions();
        assert_eq!(defs.len(), 6);
        assert!(defs.iter().any(|d| d.name == "arch_query"));
        assert!(defs.iter().any(|d| d.name == "arch_context"));
    }
}
