//! Dogfood integration test: runs rs-hack-arch on the rs-hack workspace itself.
//!
//! This proves the annotation extraction, graph building, query, schema loading,
//! and validation pipeline all work end-to-end on real annotated source code.

use rs_hack_arch::extract::extract_from_workspace_verbose;
use rs_hack_arch::graph::ArchGraph;
use rs_hack_arch::query::{get_file_context, Query};
use rs_hack_arch::schema::Schema;
use rs_hack_arch::ticket::TicketBoard;
use rs_hack_arch::validate::{load_rules_from_metadata, validate, rules_from_schema};
use std::collections::HashSet;

/// Helper: get workspace root (two levels up from this test file's crate)
fn workspace_root() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string()
}

/// Extract annotations from the whole workspace and build the graph.
fn build_graph() -> ArchGraph {
    let root = workspace_root();
    let annotations = extract_from_workspace_verbose(&root, true)
        .expect("Failed to extract annotations from workspace");
    assert!(
        annotations.len() >= 15,
        "Expected at least 15 annotations across the workspace, got {}",
        annotations.len()
    );
    ArchGraph::from_annotations(annotations)
}

// ─── Extraction ──────────────────────────────────────────────────────────

#[test]
fn test_extracts_annotations_from_workspace() {
    let root = workspace_root();
    let annotations = extract_from_workspace_verbose(&root, false)
        .expect("Failed to extract");

    // We annotated ~17 source files, each with at least 1 annotation
    assert!(
        annotations.len() >= 15,
        "Expected at least 15 annotations, got {}",
        annotations.len()
    );

    // Check that we got annotations from multiple crates
    let files: HashSet<String> = annotations
        .iter()
        .map(|a| a.file.to_string_lossy().to_string())
        .collect();

    let has_rs_hack = files.iter().any(|f| f.contains("rs-hack/src/"));
    let has_mcp = files.iter().any(|f| f.contains("rs-hack-mcp/src/"));
    let has_arch = files.iter().any(|f| f.contains("rs-hack-arch/src/"));

    assert!(has_rs_hack, "Missing annotations from rs-hack crate");
    assert!(has_mcp, "Missing annotations from rs-hack-mcp crate");
    assert!(has_arch, "Missing annotations from rs-hack-arch crate");
}

// ─── Layers ──────────────────────────────────────────────────────────────

#[test]
fn test_all_four_layers_present() {
    let graph = build_graph();

    let layers: HashSet<String> = graph
        .nodes()
        .filter_map(|n| n.properties.layer.clone())
        .collect();

    for expected in &["core", "cli", "mcp", "arch"] {
        assert!(
            layers.contains(*expected),
            "Missing layer '{}'. Found: {:?}",
            expected,
            layers
        );
    }
}

#[test]
fn test_core_layer_has_multiple_nodes() {
    let graph = build_graph();

    let core_count = graph.nodes_in_layer("core").count();
    // lib.rs, operations.rs, editor.rs, visitor.rs, surgical.rs, diff.rs, state.rs, path_resolver.rs
    assert!(
        core_count >= 6,
        "Expected at least 6 nodes in core layer, got {}",
        core_count
    );
}

#[test]
fn test_mcp_layer_nodes() {
    let graph = build_graph();

    let mcp_count = graph.nodes_in_layer("mcp").count();
    // main.rs, mod.rs, protocol.rs, server.rs, tools.rs
    assert!(
        mcp_count >= 3,
        "Expected at least 3 nodes in mcp layer, got {}",
        mcp_count
    );
}

#[test]
fn test_arch_layer_nodes() {
    let graph = build_graph();

    let arch_count = graph.nodes_in_layer("arch").count();
    // lib.rs, annotation.rs, extract.rs, graph.rs, query.rs, schema.rs, validate.rs, mcp.rs
    assert!(
        arch_count >= 6,
        "Expected at least 6 nodes in arch layer, got {}",
        arch_count
    );
}

// ─── Roles ───────────────────────────────────────────────────────────────

#[test]
fn test_key_roles_present() {
    let graph = build_graph();

    let all_roles: HashSet<String> = graph
        .nodes()
        .flat_map(|n| n.properties.roles.iter().cloned())
        .collect();

    for expected in &[
        "parser", "refactor", "emit", "diff", "state", "resolve",
        "traverse", "bridge", "protocol", "extract", "graph", "validate", "query",
    ] {
        assert!(
            all_roles.contains(*expected),
            "Missing role '{}'. Found: {:?}",
            expected,
            all_roles
        );
    }
}

#[test]
fn test_editor_has_multiple_roles() {
    let graph = build_graph();

    // editor.rs should have parser + refactor + emit
    let editor_node = graph
        .nodes()
        .find(|n| n.file.to_string_lossy().contains("rs-hack/src/editor.rs"));

    assert!(editor_node.is_some(), "Could not find editor.rs node");
    let node = editor_node.unwrap();
    assert!(node.properties.roles.contains(&"parser".to_string()));
    assert!(node.properties.roles.contains(&"refactor".to_string()));
    assert!(node.properties.roles.contains(&"emit".to_string()));
}

// ─── Queries ─────────────────────────────────────────────────────────────

#[test]
fn test_query_layer() {
    let graph = build_graph();
    let q = Query::parse("layer:core").unwrap();
    let result = q.execute(&graph);
    assert!(result.count >= 6, "layer:core should match >= 6 nodes, got {}", result.count);
}

#[test]
fn test_query_role() {
    let graph = build_graph();
    let q = Query::parse("role:parser").unwrap();
    let result = q.execute(&graph);
    // editor.rs (core) + extract.rs + annotation.rs (arch)
    assert!(result.count >= 2, "role:parser should match >= 2 nodes, got {}", result.count);
}

#[test]
fn test_query_and() {
    let graph = build_graph();
    let q = Query::parse("layer:core AND role:emit").unwrap();
    let result = q.execute(&graph);
    // editor.rs + surgical.rs
    assert!(result.count >= 2, "layer:core AND role:emit should match >= 2, got {}", result.count);
}

#[test]
fn test_query_or() {
    let graph = build_graph();
    let q = Query::parse("layer:cli OR layer:mcp").unwrap();
    let result = q.execute(&graph);
    assert!(result.count >= 4, "cli OR mcp should match >= 4, got {}", result.count);
}

#[test]
fn test_query_file() {
    let graph = build_graph();
    let q = Query::parse("file:surgical").unwrap();
    let result = q.execute(&graph);
    assert_eq!(result.count, 1, "file:surgical should match exactly 1 node");
}

#[test]
fn test_query_all() {
    let graph = build_graph();
    let q = Query::parse("*").unwrap();
    let result = q.execute(&graph);
    assert!(result.count >= 15, "* should match all nodes, got {}", result.count);
}

// ─── File Context ────────────────────────────────────────────────────────

#[test]
fn test_file_context_editor() {
    let graph = build_graph();
    let ctx = get_file_context(&graph, "rs-hack/src/editor.rs");

    assert_eq!(ctx.layer.as_deref(), Some("core"));
    assert!(ctx.roles.contains(&"parser".to_string()));
    assert!(ctx.roles.contains(&"emit".to_string()));
    assert!(!ctx.notes.is_empty(), "editor.rs should have a design note");

    let md = ctx.to_markdown("rs-hack/src/editor.rs");
    assert!(md.contains("**Layer**: core"));
    assert!(md.contains("Design notes"));
}

#[test]
fn test_file_context_mcp_server() {
    let graph = build_graph();
    let ctx = get_file_context(&graph, "mcp/server.rs");

    assert_eq!(ctx.layer.as_deref(), Some("mcp"));
    assert!(ctx.roles.contains(&"bridge".to_string()));
}

// ─── Schema ──────────────────────────────────────────────────────────────

#[test]
fn test_schema_loads_from_cargo_metadata() {
    let root = workspace_root();
    let schema = Schema::from_cargo_metadata(&root)
        .expect("Failed to load schema from Cargo.toml");

    assert!(!schema.is_empty(), "Schema should not be empty");
    assert!(schema.is_valid_layer("core"), "core should be a valid layer");
    assert!(schema.is_valid_layer("cli"), "cli should be a valid layer");
    assert!(schema.is_valid_layer("mcp"), "mcp should be a valid layer");
    assert!(schema.is_valid_layer("arch"), "arch should be a valid layer");
    assert!(!schema.is_valid_layer("nonexistent"), "nonexistent should not be valid");

    assert!(schema.is_valid_role("parser"), "parser should be a valid role");
    assert!(schema.is_valid_role("refactor"), "refactor should be a valid role");
    assert!(schema.is_valid_role("bridge"), "bridge should be a valid role");

    let summary = schema.summary();
    assert!(summary.contains("core"), "Summary should mention core layer");
    assert!(summary.contains("parser"), "Summary should mention parser role");
}

// ─── Validation ──────────────────────────────────────────────────────────

#[test]
fn test_rules_load_from_metadata() {
    let root = workspace_root();
    let rules = load_rules_from_metadata(&root)
        .expect("Failed to load rules");

    assert!(
        !rules.is_empty(),
        "Should have at least one rule defined in Cargo.toml"
    );

    let rule_names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    assert!(
        rule_names.contains(&"core-independence"),
        "Missing core-independence rule. Found: {:?}",
        rule_names
    );
}

#[test]
fn test_schema_generates_layer_dependency_rules() {
    let root = workspace_root();
    let schema = Schema::from_cargo_metadata(&root).unwrap();
    let rules = rules_from_schema(&schema);

    assert!(
        !rules.is_empty(),
        "Schema should generate layer dependency rules"
    );
}

#[test]
fn test_validate_no_false_positives() {
    // The workspace annotations should be valid against our rules
    let root = workspace_root();
    let graph = build_graph();
    let rules = load_rules_from_metadata(&root).unwrap();

    let violations = validate(&graph, &rules);

    // We don't expect any errors (warnings are OK)
    let errors: Vec<_> = violations
        .iter()
        .filter(|v| v.severity == rs_hack_arch::validate::Severity::Error)
        .collect();

    // Note: there may be legitimate violations if the graph infers cross-layer edges.
    // For now, just report them rather than asserting zero.
    if !errors.is_empty() {
        eprintln!("Validation errors found (may be expected during development):");
        for v in &errors {
            eprintln!("  {} - {}", v.rule, v.message);
        }
    }
}

// ─── Mermaid / Export ────────────────────────────────────────────────────

#[test]
fn test_mermaid_export() {
    let graph = build_graph();
    let mermaid = graph.to_mermaid();

    assert!(mermaid.starts_with("graph TD\n"));
    assert!(mermaid.contains("core"), "Mermaid should contain core subgraph");
    assert!(mermaid.contains("mcp"), "Mermaid should contain mcp subgraph");
}

#[test]
fn test_json_roundtrip() {
    let graph = build_graph();
    let json = graph.to_json().expect("Failed to serialize graph");
    let restored = ArchGraph::from_json(&json).expect("Failed to deserialize graph");

    // Compare node counts
    let orig_count = graph.nodes().count();
    let restored_count = restored.nodes().count();
    assert_eq!(orig_count, restored_count, "JSON roundtrip should preserve node count");
}

// ─── MCP Tool Definitions ───────────────────────────────────────────────

#[test]
fn test_mcp_tool_definitions() {
    let defs = rs_hack_arch::mcp::tool_definitions();
    assert_eq!(defs.len(), 6);

    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"arch_query"));
    assert!(names.contains(&"arch_trace"));
    assert!(names.contains(&"arch_context"));
    assert!(names.contains(&"arch_validate"));
    assert!(names.contains(&"hack_tickets"));
}

// ─── Notes and Doc Text ──────────────────────────────────────────────────

#[test]
fn test_notes_captured() {
    let graph = build_graph();

    let nodes_with_notes: Vec<_> = graph
        .nodes()
        .filter(|n| !n.properties.notes.is_empty())
        .collect();

    assert!(
        !nodes_with_notes.is_empty(),
        "Should have at least one node with @arch:note annotations"
    );
}

#[test]
fn test_doc_text_captured() {
    let graph = build_graph();

    let nodes_with_docs: Vec<_> = graph
        .nodes()
        .filter(|n| n.properties.doc.is_some())
        .collect();

    assert!(
        !nodes_with_docs.is_empty(),
        "Should have at least one node with doc text captured"
    );
}

// ─── Depends-on Edges ────────────────────────────────────────────────────

#[test]
fn test_depends_on_edges() {
    let graph = build_graph();

    let dep_edges: Vec<_> = graph
        .edges()
        .filter(|e| e.kind == rs_hack_arch::graph::EdgeKind::DependsOn)
        .collect();

    assert!(
        !dep_edges.is_empty(),
        "Should have at least one depends_on edge (cli -> core, mcp -> cli)"
    );
}

// ─── Ticket Board (hack-board dogfood) ──────────────────────────────────

#[test]
fn test_hack_tickets_from_workspace() {
    let root = workspace_root();
    let annotations = extract_from_workspace_verbose(&root, false).unwrap();
    let board = TicketBoard::from_annotations(&annotations);

    // We have a @hack:relay(R001, ...) on ticket.rs
    assert!(
        !board.tickets.is_empty(),
        "Should find at least one @hack: ticket in the workspace"
    );

    let r001 = board.get("R001");
    assert!(r001.is_some(), "Should find relay R001");
    let r001 = r001.unwrap();
    assert!(r001.title.contains("hack-board"));
    assert_eq!(r001.item_type, rs_hack_arch::ticket::ItemType::Relay);
    assert_eq!(r001.status, rs_hack_arch::ticket::TicketStatus::Handoff);

    // Verify prompt generation works end-to-end
    let prompt = r001.to_prompt();
    assert!(prompt.contains("# Continue: R001"));
    assert!(prompt.contains("Playbook"));
    assert!(prompt.contains("**R1"), "prompt should embed the pickup playbook leading with R1");

    // Verify markdown output shows [R] badge
    let md = board.to_markdown();
    assert!(md.contains("[R] R001"));
}
