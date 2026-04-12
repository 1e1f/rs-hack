//! @arch:layer(arch)
//! @arch:role(validate)
//!
//! Architecture validation rules.
//! Rules are defined in TOML or in Cargo.toml metadata, and checked against the graph.
//!
//! ## Rules in Cargo.toml
//!
//! ```toml
//! [[workspace.metadata.arch.rules]]
//! name = "audio-thread-isolation"
//! description = "Audio thread cannot call into UI layer"
//! severity = "error"
//! type = "thread_isolation"
//! thread = "audio"
//! deny_layers = ["ui"]
//!
//! [[workspace.metadata.arch.rules]]
//! name = "core-independence"
//! description = "Core layer cannot depend on higher layers"
//! severity = "error"
//! type = "layer_dependency"
//! layer = "core"
//! allowed = []
//! ```

use crate::graph::{ArchGraph, EdgeKind};
use crate::query::Query;
use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A validation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Rule name
    pub name: String,

    /// Description
    pub description: Option<String>,

    /// Severity: error (fails build) or warning
    #[serde(default)]
    pub severity: Severity,

    /// The kind of rule
    #[serde(flatten)]
    pub kind: RuleKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

/// Types of validation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleKind {
    /// Deny edges from source to target
    Deny { from: String, to: String },

    /// Require edges from source to target
    Require { from: String, to: String },

    /// Nodes matching query must have property
    RequireProperty { query: String, property: String },

    /// Layer dependency constraint
    LayerDependency {
        layer: String,
        allowed: Vec<String>,
    },

    /// Thread isolation (no cross-thread calls except via channels)
    ThreadIsolation {
        thread: String,
        deny_layers: Vec<String>,
    },

    /// QoS constraint (e.g., realtime can't use heap)
    QosConstraint { qos: String, constraint: String },
}

/// A rule violation.
#[derive(Debug, Clone)]
pub struct Violation {
    /// The rule that was violated
    pub rule: String,

    /// Description of the violation
    pub message: String,

    /// File where violation occurred
    pub file: Option<PathBuf>,

    /// Line number
    pub line: Option<usize>,

    /// Severity
    pub severity: Severity,
}

/// Validate the graph against a set of rules.
pub fn validate(graph: &ArchGraph, rules: &[Rule]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for rule in rules {
        violations.extend(check_rule(graph, rule));
    }

    violations
}

fn check_rule(graph: &ArchGraph, rule: &Rule) -> Vec<Violation> {
    match &rule.kind {
        RuleKind::Deny { from, to } => check_deny(graph, rule, from, to),
        RuleKind::Require { from, to } => check_require(graph, rule, from, to),
        RuleKind::RequireProperty { query, property } => {
            check_require_property(graph, rule, query, property)
        }
        RuleKind::LayerDependency { layer, allowed } => {
            check_layer_dependency(graph, rule, layer, allowed)
        }
        RuleKind::ThreadIsolation { thread, deny_layers } => {
            check_thread_isolation(graph, rule, thread, deny_layers)
        }
        RuleKind::QosConstraint { qos, constraint } => {
            check_qos_constraint(graph, rule, qos, constraint)
        }
    }
}

fn check_deny(graph: &ArchGraph, rule: &Rule, from_query: &str, to_query: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    let from_q = match Query::parse(from_query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };
    let to_q = match Query::parse(to_query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    for edge in graph.edges() {
        let from_node = graph.get_node(&edge.from);
        let to_node = graph.get_node(&edge.to);

        if let (Some(from_node), Some(to_node)) = (from_node, to_node) {
            if from_q.matches(from_node) && to_q.matches(to_node) {
                violations.push(Violation {
                    rule: rule.name.clone(),
                    message: format!(
                        "Denied dependency: {} -> {}",
                        edge.from, edge.to
                    ),
                    file: Some(from_node.file.clone()),
                    line: Some(from_node.line),
                    severity: rule.severity,
                });
            }
        }
    }

    violations
}

fn check_require(
    graph: &ArchGraph,
    rule: &Rule,
    from_query: &str,
    to_query: &str,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    let from_q = match Query::parse(from_query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };
    let to_q = match Query::parse(to_query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    // For each node matching from_q, check if there's an edge to a node matching to_q
    for from_node in graph.nodes().filter(|n| from_q.matches(n)) {
        let has_required = graph.edges_from(&from_node.id).any(|edge| {
            graph
                .get_node(&edge.to)
                .map(|n| to_q.matches(n))
                .unwrap_or(false)
        });

        if !has_required {
            violations.push(Violation {
                rule: rule.name.clone(),
                message: format!(
                    "Missing required dependency: {} should depend on {}",
                    from_node.id, to_query
                ),
                file: Some(from_node.file.clone()),
                line: Some(from_node.line),
                severity: rule.severity,
            });
        }
    }

    violations
}

fn check_require_property(
    graph: &ArchGraph,
    rule: &Rule,
    query: &str,
    property: &str,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    let q = match Query::parse(query) {
        Ok(q) => q,
        Err(_) => return vec![],
    };

    for node in graph.nodes().filter(|n| q.matches(n)) {
        let has_property = match property {
            "layer" => node.properties.layer.is_some(),
            "role" => !node.properties.roles.is_empty(),
            "thread" => node.properties.thread.is_some(),
            "qos" => node.properties.qos.is_some(),
            "gateway" => node.properties.is_gateway,
            "owns_voices" => node.properties.owns_voices,
            _ => true, // Unknown property, skip
        };

        if !has_property {
            violations.push(Violation {
                rule: rule.name.clone(),
                message: format!("{} is missing required property: {}", node.id, property),
                file: Some(node.file.clone()),
                line: Some(node.line),
                severity: rule.severity,
            });
        }
    }

    violations
}

fn check_layer_dependency(
    graph: &ArchGraph,
    rule: &Rule,
    layer: &str,
    allowed: &[String],
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for node in graph.nodes_in_layer(layer) {
        for edge in graph.edges_from(&node.id) {
            if let Some(target) = graph.get_node(&edge.to) {
                if let Some(ref target_layer) = target.properties.layer {
                    if target_layer != layer && !allowed.contains(target_layer) {
                        violations.push(Violation {
                            rule: rule.name.clone(),
                            message: format!(
                                "Layer {} cannot depend on layer {}: {} -> {}",
                                layer, target_layer, node.id, target.id
                            ),
                            file: Some(node.file.clone()),
                            line: Some(node.line),
                            severity: rule.severity,
                        });
                    }
                }
            }
        }
    }

    violations
}

fn check_thread_isolation(
    graph: &ArchGraph,
    rule: &Rule,
    thread: &str,
    deny_layers: &[String],
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for node in graph.nodes_on_thread(thread) {
        for edge in graph.edges_from(&node.id) {
            // Skip message flow edges (these are allowed cross-thread)
            if edge.kind == EdgeKind::MessageFlow {
                continue;
            }

            if let Some(target) = graph.get_node(&edge.to) {
                if let Some(ref target_layer) = target.properties.layer {
                    if deny_layers.contains(target_layer) {
                        violations.push(Violation {
                            rule: rule.name.clone(),
                            message: format!(
                                "Thread {} cannot directly call into layer {}: {} -> {}",
                                thread, target_layer, node.id, target.id
                            ),
                            file: Some(node.file.clone()),
                            line: Some(node.line),
                            severity: rule.severity,
                        });
                    }
                }
            }
        }
    }

    violations
}

fn check_qos_constraint(
    graph: &ArchGraph,
    _rule: &Rule,
    qos: &str,
    constraint: &str,
) -> Vec<Violation> {
    let violations = Vec::new();

    // QoS constraints are more of documentation for now
    // Real heap detection would require more sophisticated analysis

    for node in graph.nodes() {
        if node.properties.qos.as_deref() == Some(qos) {
            // This is a placeholder - real implementation would check code
            // For now, just note that these nodes have constraints
            if constraint == "no_heap" {
                // Would need code analysis here
            }
        }
    }

    violations
}

/// Load rules from workspace Cargo.toml metadata.
pub fn load_rules_from_metadata(workspace_root: impl AsRef<Path>) -> Result<Vec<Rule>> {
    let workspace_root = workspace_root.as_ref();

    let metadata = MetadataCommand::new()
        .manifest_path(workspace_root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .with_context(|| format!("Failed to read cargo metadata from {}", workspace_root.display()))?;

    // Look for workspace metadata first
    if let Some(arch_meta) = metadata.workspace_metadata.get("arch") {
        if let Some(rules) = arch_meta.get("rules") {
            return parse_rules_from_json(rules);
        }
    }

    // Fall back to root package metadata
    if let Some(root_pkg) = metadata.root_package() {
        if let Some(arch_meta) = root_pkg.metadata.get("arch") {
            if let Some(rules) = arch_meta.get("rules") {
                return parse_rules_from_json(rules);
            }
        }
    }

    // No rules found
    Ok(Vec::new())
}

/// Parse rules from JSON value (from Cargo.toml metadata).
fn parse_rules_from_json(value: &serde_json::Value) -> Result<Vec<Rule>> {
    let rules: Vec<Rule> = serde_json::from_value(value.clone())
        .context("Failed to parse arch rules from Cargo.toml metadata")?;
    Ok(rules)
}

/// Load rules from TOML.
pub fn load_rules(toml_content: &str) -> Result<Vec<Rule>, toml::de::Error> {
    #[derive(Deserialize)]
    struct RulesFile {
        rule: Vec<Rule>,
    }

    let file: RulesFile = toml::from_str(toml_content)?;
    Ok(file.rule)
}

/// Generate layer dependency rules from schema.
pub fn rules_from_schema(schema: &crate::schema::Schema) -> Vec<Rule> {
    let mut rules = Vec::new();

    // Generate layer dependency rules
    for (layer_name, layer_def) in &schema.layers {
        rules.push(Rule {
            name: format!("{}-dependencies", layer_name),
            description: Some(format!("Layer {} dependency constraints", layer_name)),
            severity: Severity::Error,
            kind: RuleKind::LayerDependency {
                layer: layer_name.clone(),
                allowed: layer_def.allowed_dependencies.clone(),
            },
        });
    }

    rules
}
