//! @arch:layer(arch)
//! @arch:role(graph)
//! @hack:ticket(R001-T6, "Multiple @hack:ticket on same AST target only produces one ticket")
//! @hack:kind(bug)
//! @hack:phase(P2)
//! @hack:status(open)
//!
//! Graph data structure for architectural relationships.
//! Uses petgraph to store nodes (code entities) and edges (relationships).

use crate::annotation::{AnnotationTarget, ArchAnnotation, ArchKind, MessageSpec, ThreadSpec};
use crate::schema::Schema;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// The architecture knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchGraph {
    /// Nodes representing code entities
    nodes: HashMap<String, ArchNode>,

    /// Edges representing relationships
    edges: Vec<ArchEdge>,

    /// The schema used for validation
    #[serde(skip)]
    schema: Schema,

    /// Hash of source files when graph was built
    source_hash: Option<String>,
}

/// A node in the architecture graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchNode {
    /// Unique identifier (e.g., "struct:vivarium::impulse::ImpulseHub")
    pub id: String,

    /// Source file
    pub file: PathBuf,

    /// Line number of definition
    pub line: usize,

    /// The target this node represents
    pub target: AnnotationTarget,

    /// Architectural properties
    pub properties: NodeProperties,
}

/// Properties extracted from annotations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeProperties {
    /// @arch:layer
    pub layer: Option<String>,

    /// @arch:role (can have multiple)
    pub roles: Vec<String>,

    /// @arch:thread
    pub thread: Option<ThreadSpec>,

    /// @arch:qos
    pub qos: Option<String>,

    /// @arch:produces
    pub produces: Vec<MessageSpec>,

    /// @arch:consumes
    pub consumes: Vec<MessageSpec>,

    /// @arch:provides_context
    pub provides_context: Option<String>,

    /// @arch:requires_context
    pub requires_context: Vec<String>,

    /// @arch:pattern
    pub patterns: Vec<String>,

    /// @arch:musical
    pub musical: Option<String>,

    /// @arch:gateway
    pub is_gateway: bool,

    /// @arch:owns_voices
    pub owns_voices: bool,

    /// @arch:implements
    pub implements: Vec<String>,

    /// @arch:entity
    pub entity: Option<String>,

    /// @arch:aggregate_root
    pub is_aggregate_root: bool,

    /// Full doc comment text (narrative, rationale, examples)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,

    /// @arch:note (inline design notes)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,

    /// @arch:see (references to external documentation)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub see_also: Vec<String>,
}

/// An edge representing a relationship between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchEdge {
    /// Source node ID
    pub from: String,

    /// Target node ID
    pub to: String,

    /// Type of relationship
    pub kind: EdgeKind,

    /// Optional reason/description
    pub reason: Option<String>,
}

/// Types of edges in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// @arch:depends_on
    DependsOn,

    /// Inferred from produces/consumes matching
    MessageFlow,

    /// @arch:bridge
    Bridge,

    /// @arch:flow
    DataFlow,

    /// Trait implementation
    Implements,

    /// Context provider/consumer relationship
    Context,
}

impl ArchGraph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self::with_schema(Schema::default())
    }

    /// Create a graph with a custom schema.
    pub fn with_schema(schema: Schema) -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            schema,
            source_hash: None,
        }
    }

    /// Build a graph from extracted annotations.
    pub fn from_annotations(annotations: Vec<ArchAnnotation>) -> Self {
        let mut graph = Self::new();
        graph.add_annotations(annotations);
        graph.infer_edges();
        graph
    }

    /// Add annotations to the graph.
    pub fn add_annotations(&mut self, annotations: Vec<ArchAnnotation>) {
        // Group annotations by target, also collecting doc_text
        let mut by_target: HashMap<String, (AnnotationTarget, PathBuf, usize, Vec<ArchKind>, Option<String>)> =
            HashMap::new();

        for ann in annotations {
            let id = ann.target.id();
            let entry = by_target
                .entry(id)
                .or_insert_with(|| (ann.target.clone(), ann.file.clone(), ann.line, Vec::new(), None));
            entry.3.push(ann.kind);
            // Capture doc_text from the first annotation that has it
            if entry.4.is_none() && ann.doc_text.is_some() {
                entry.4 = ann.doc_text;
            }
        }

        // Create nodes
        for (id, (target, file, line, kinds, doc_text)) in by_target {
            let mut props = NodeProperties::default();
            props.doc = doc_text;

            for kind in kinds {
                match kind {
                    ArchKind::Layer(l) => props.layer = Some(l),
                    ArchKind::Role(r) => props.roles.push(r),
                    ArchKind::Thread(t) => props.thread = Some(t),
                    ArchKind::Qos(q) => props.qos = Some(q.class),
                    ArchKind::Produces(m) => props.produces.extend(m),
                    ArchKind::Consumes(m) => props.consumes.extend(m),
                    ArchKind::ProvidesContext(c) => props.provides_context = Some(c),
                    ArchKind::RequiresContext(c) => props.requires_context.extend(c),
                    ArchKind::Pattern(p) => props.patterns.push(p),
                    ArchKind::Musical(m) => props.musical = Some(m),
                    ArchKind::Gateway => props.is_gateway = true,
                    ArchKind::OwnsVoices => props.owns_voices = true,
                    ArchKind::Implements(t) => props.implements.push(t),
                    ArchKind::Entity(e) => props.entity = Some(e),
                    ArchKind::AggregateRoot => props.is_aggregate_root = true,
                    ArchKind::Note(n) => props.notes.push(n),
                    ArchKind::See(s) => props.see_also.push(s),
                    ArchKind::DependsOn { target, reason } => {
                        self.edges.push(ArchEdge {
                            from: id.clone(),
                            to: target,
                            kind: EdgeKind::DependsOn,
                            reason,
                        });
                    }
                    ArchKind::Bridge { from, to } => {
                        self.edges.push(ArchEdge {
                            from: from.clone(),
                            to: to.clone(),
                            kind: EdgeKind::Bridge,
                            reason: Some(format!("{} -> {}", from, to)),
                        });
                    }
                    ArchKind::Flow(steps) => {
                        for window in steps.windows(2) {
                            self.edges.push(ArchEdge {
                                from: window[0].clone(),
                                to: window[1].clone(),
                                kind: EdgeKind::DataFlow,
                                reason: None,
                            });
                        }
                    }
                    ArchKind::Unknown { .. } => {}

                    // @hack: annotations are handled by TicketBoard, not the arch graph
                    ArchKind::Ticket { .. }
                    | ArchKind::Relay { .. }
                    | ArchKind::Kind(_)
                    | ArchKind::Status(_)
                    | ArchKind::Assignee(_)
                    | ArchKind::Phase(_)
                    | ArchKind::Parent(_)
                    | ArchKind::HackSeverity(_)
                    | ArchKind::Handoff(_)
                    | ArchKind::Next(_)
                    | ArchKind::Cleanup(_)
                    | ArchKind::Verify(_)
                    | ArchKind::Gotcha(_)
                    | ArchKind::Assumes(_) => {}
                }
            }

            self.nodes.insert(
                id.clone(),
                ArchNode {
                    id,
                    file,
                    line,
                    target,
                    properties: props,
                },
            );
        }
    }

    /// Infer edges from produces/consumes relationships.
    fn infer_edges(&mut self) {
        let producers: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, n)| !n.properties.produces.is_empty())
            .map(|(id, n)| (id.clone(), n.properties.produces.clone()))
            .collect();

        let consumers: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, n)| !n.properties.consumes.is_empty())
            .map(|(id, n)| (id.clone(), n.properties.consumes.clone()))
            .collect();

        for (producer_id, produces) in &producers {
            for (consumer_id, consumes) in &consumers {
                if producer_id == consumer_id {
                    continue;
                }

                for prod in produces {
                    for cons in consumes {
                        if messages_match(prod, cons) {
                            self.edges.push(ArchEdge {
                                from: producer_id.clone(),
                                to: consumer_id.clone(),
                                kind: EdgeKind::MessageFlow,
                                reason: Some(format!(
                                    "{}:{} -> {}:{}",
                                    prod.category, prod.name, cons.category, cons.name
                                )),
                            });
                        }
                    }
                }
            }
        }

        // Infer context relationships
        let providers: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, n)| {
                n.properties
                    .provides_context
                    .as_ref()
                    .map(|c| (id.clone(), c.clone()))
            })
            .collect();

        for (node_id, node) in &self.nodes {
            for required in &node.properties.requires_context {
                for (provider_id, provides) in &providers {
                    if provides == required {
                        self.edges.push(ArchEdge {
                            from: provider_id.clone(),
                            to: node_id.clone(),
                            kind: EdgeKind::Context,
                            reason: Some(format!("provides {}", provides)),
                        });
                    }
                }
            }
        }
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&ArchNode> {
        self.nodes.get(id)
    }

    /// Get all nodes.
    pub fn nodes(&self) -> impl Iterator<Item = &ArchNode> {
        self.nodes.values()
    }

    /// Get all edges.
    pub fn edges(&self) -> impl Iterator<Item = &ArchEdge> {
        self.edges.iter()
    }

    /// Find nodes by layer.
    pub fn nodes_in_layer<'a>(&'a self, layer: &'a str) -> impl Iterator<Item = &'a ArchNode> {
        self.nodes
            .values()
            .filter(move |n| n.properties.layer.as_deref() == Some(layer))
    }

    /// Find nodes by role.
    pub fn nodes_with_role<'a>(&'a self, role: &'a str) -> impl Iterator<Item = &'a ArchNode> {
        self.nodes
            .values()
            .filter(move |n| n.properties.roles.iter().any(|r| r == role))
    }

    /// Find nodes by thread.
    pub fn nodes_on_thread<'a>(&'a self, thread: &'a str) -> impl Iterator<Item = &'a ArchNode> {
        self.nodes.values().filter(move |n| {
            matches!(&n.properties.thread, Some(ThreadSpec::On(t)) if t == thread)
        })
    }

    /// Find gateways.
    pub fn gateways(&self) -> impl Iterator<Item = &ArchNode> {
        self.nodes.values().filter(|n| n.properties.is_gateway)
    }

    /// Find voice allocators.
    pub fn voice_allocators(&self) -> impl Iterator<Item = &ArchNode> {
        self.nodes.values().filter(|n| n.properties.owns_voices)
    }

    /// Get edges from a node.
    pub fn edges_from<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a ArchEdge> {
        self.edges.iter().filter(move |e| e.from == id)
    }

    /// Get edges to a node.
    pub fn edges_to<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a ArchEdge> {
        self.edges.iter().filter(move |e| e.to == id)
    }

    /// Convert to a petgraph for path queries.
    pub fn to_petgraph(&self) -> (DiGraph<String, EdgeKind>, HashMap<String, NodeIndex>) {
        let mut graph = DiGraph::new();
        let mut indices = HashMap::new();

        // Add all nodes
        for id in self.nodes.keys() {
            let idx = graph.add_node(id.clone());
            indices.insert(id.clone(), idx);
        }

        // Add edges
        for edge in &self.edges {
            if let (Some(&from_idx), Some(&to_idx)) = (indices.get(&edge.from), indices.get(&edge.to))
            {
                graph.add_edge(from_idx, to_idx, edge.kind.clone());
            }
        }

        (graph, indices)
    }

    /// Export to Mermaid diagram format.
    pub fn to_mermaid(&self) -> String {
        let mut output = String::from("graph TD\n");

        // Group nodes by layer
        let mut by_layer: HashMap<Option<&str>, Vec<&ArchNode>> = HashMap::new();
        for node in self.nodes.values() {
            by_layer
                .entry(node.properties.layer.as_deref())
                .or_default()
                .push(node);
        }

        // Add nodes
        for (layer, nodes) in by_layer {
            if let Some(layer_name) = layer {
                output.push_str(&format!("    subgraph {}\n", layer_name));
            }
            for node in nodes {
                let label = node.target.id().replace("::", "_");
                output.push_str(&format!("        {}[\"{}\"]\n", label, short_name(&node.id)));
            }
            if layer.is_some() {
                output.push_str("    end\n");
            }
        }

        // Add edges
        for edge in &self.edges {
            let from = edge.from.replace("::", "_");
            let to = edge.to.replace("::", "_");
            let arrow = match edge.kind {
                EdgeKind::DependsOn => "-->",
                EdgeKind::MessageFlow => "-.->",
                EdgeKind::Bridge => "==>",
                EdgeKind::DataFlow => "-->",
                EdgeKind::Implements => "-.->|impl|",
                EdgeKind::Context => "-.->|ctx|",
            };
            output.push_str(&format!("    {} {} {}\n", from, arrow, to));
        }

        output
    }

    /// Set the source hash for caching.
    pub fn set_source_hash(&mut self, hash: String) {
        self.source_hash = Some(hash);
    }

    /// Get the source hash.
    pub fn source_hash(&self) -> Option<&str> {
        self.source_hash.as_deref()
    }

    /// Save graph to JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Load graph from JSON.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Get the schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

impl Default for ArchGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if two message specs match (accounting for wildcards).
fn messages_match(producer: &MessageSpec, consumer: &MessageSpec) -> bool {
    if producer.category != consumer.category {
        return false;
    }
    producer.name == consumer.name || consumer.is_wildcard() || producer.is_wildcard()
}

/// Get a short name for display.
fn short_name(id: &str) -> String {
    id.split("::").last().unwrap_or(id).to_string()
}
