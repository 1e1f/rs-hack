//! @arch:layer(arch)
//! @arch:role(query)
//!
//! Query language for the architecture graph.
//! Supports predicate queries like `layer:core AND role:parser`,
//! path tracing, and file context extraction for IDE integration.

use crate::graph::{ArchGraph, ArchNode};
use crate::annotation::ThreadSpec;
use petgraph::algo::all_simple_paths;
use std::collections::HashSet;

/// A parsed query.
#[derive(Debug, Clone)]
pub enum Query {
    /// Match by layer
    Layer(String),

    /// Match by role
    Role(String),

    /// Match by thread
    Thread(String),

    /// Match by QoS class
    Qos(String),

    /// Match if produces message type
    Produces(String, String), // category, name

    /// Match if consumes message type
    Consumes(String, String),

    /// Match gateways
    Gateway,

    /// Match voice allocators
    OwnsVoices,

    /// Match by pattern
    Pattern(String),

    /// Match by musical concept
    Musical(String),

    /// Match if provides context
    ProvidesContext(String),

    /// Match if requires context
    RequiresContext(String),

    /// Match if implements trait
    Implements(String),

    /// Match by entity type
    Entity(String),

    /// Match aggregate roots
    AggregateRoot,

    /// Match by file path contains
    File(String),

    /// Logical AND
    And(Box<Query>, Box<Query>),

    /// Logical OR
    Or(Box<Query>, Box<Query>),

    /// Logical NOT
    Not(Box<Query>),

    /// Match all
    All,
}

/// Result of a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub nodes: Vec<String>,
    pub count: usize,
}

impl Query {
    /// Parse a query string.
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_query(input.trim())
    }

    /// Execute the query on a graph.
    pub fn execute(&self, graph: &ArchGraph) -> QueryResult {
        let matching: Vec<String> = graph
            .nodes()
            .filter(|n| self.matches(n))
            .map(|n| n.id.clone())
            .collect();

        let count = matching.len();
        QueryResult {
            nodes: matching,
            count,
        }
    }

    /// Check if a node matches this query.
    pub fn matches(&self, node: &ArchNode) -> bool {
        match self {
            Query::Layer(l) => node.properties.layer.as_deref() == Some(l.as_str()),
            Query::Role(r) => node.properties.roles.iter().any(|role| role == r),
            Query::Thread(t) => matches!(
                &node.properties.thread,
                Some(ThreadSpec::On(thread)) if thread == t
            ),
            Query::Qos(q) => node.properties.qos.as_deref() == Some(q.as_str()),
            Query::Produces(cat, name) => node.properties.produces.iter().any(|m| {
                &m.category == cat && (&m.name == name || name == "*" || m.name == "*")
            }),
            Query::Consumes(cat, name) => node.properties.consumes.iter().any(|m| {
                &m.category == cat && (&m.name == name || name == "*" || m.name == "*")
            }),
            Query::Gateway => node.properties.is_gateway,
            Query::OwnsVoices => node.properties.owns_voices,
            Query::Pattern(p) => node.properties.patterns.contains(p),
            Query::Musical(m) => node.properties.musical.as_deref() == Some(m.as_str()),
            Query::ProvidesContext(c) => node.properties.provides_context.as_deref() == Some(c.as_str()),
            Query::RequiresContext(c) => node.properties.requires_context.contains(c),
            Query::Implements(t) => node.properties.implements.contains(t),
            Query::Entity(e) => node.properties.entity.as_deref() == Some(e.as_str()),
            Query::AggregateRoot => node.properties.is_aggregate_root,
            Query::File(f) => node.file.to_string_lossy().contains(f),
            Query::And(a, b) => a.matches(node) && b.matches(node),
            Query::Or(a, b) => a.matches(node) || b.matches(node),
            Query::Not(q) => !q.matches(node),
            Query::All => true,
        }
    }
}

/// Parse a query string into a Query.
fn parse_query(input: &str) -> Result<Query, String> {
    let input = input.trim();

    // Handle AND
    if let Some(pos) = find_operator(input, " AND ") {
        let left = parse_query(&input[..pos])?;
        let right = parse_query(&input[pos + 5..])?;
        return Ok(Query::And(Box::new(left), Box::new(right)));
    }

    // Handle OR
    if let Some(pos) = find_operator(input, " OR ") {
        let left = parse_query(&input[..pos])?;
        let right = parse_query(&input[pos + 4..])?;
        return Ok(Query::Or(Box::new(left), Box::new(right)));
    }

    // Handle NOT
    if input.starts_with("NOT ") {
        let inner = parse_query(&input[4..])?;
        return Ok(Query::Not(Box::new(inner)));
    }

    // Handle parentheses
    if input.starts_with('(') && input.ends_with(')') {
        return parse_query(&input[1..input.len() - 1]);
    }

    // Handle simple predicates
    parse_predicate(input)
}

/// Parse a single predicate.
fn parse_predicate(input: &str) -> Result<Query, String> {
    let input = input.trim();

    // Bare keywords
    match input {
        "gateway" => return Ok(Query::Gateway),
        "owns_voices" => return Ok(Query::OwnsVoices),
        "aggregate_root" => return Ok(Query::AggregateRoot),
        "*" | "all" => return Ok(Query::All),
        _ => {}
    }

    // key:value predicates
    if let Some(pos) = input.find(':') {
        let key = &input[..pos];
        let value = &input[pos + 1..];

        match key {
            "layer" => return Ok(Query::Layer(value.to_string())),
            "role" => return Ok(Query::Role(value.to_string())),
            "thread" => return Ok(Query::Thread(value.to_string())),
            "qos" => return Ok(Query::Qos(value.to_string())),
            "produces" => {
                if let Some(pos) = value.find(':') {
                    return Ok(Query::Produces(
                        value[..pos].to_string(),
                        value[pos + 1..].to_string(),
                    ));
                } else {
                    return Ok(Query::Produces(value.to_string(), "*".to_string()));
                }
            }
            "consumes" => {
                if let Some(pos) = value.find(':') {
                    return Ok(Query::Consumes(
                        value[..pos].to_string(),
                        value[pos + 1..].to_string(),
                    ));
                } else {
                    return Ok(Query::Consumes(value.to_string(), "*".to_string()));
                }
            }
            "pattern" => return Ok(Query::Pattern(value.to_string())),
            "musical" => return Ok(Query::Musical(value.to_string())),
            "provides_context" => return Ok(Query::ProvidesContext(value.to_string())),
            "requires_context" => return Ok(Query::RequiresContext(value.to_string())),
            "implements" => return Ok(Query::Implements(value.to_string())),
            "entity" => return Ok(Query::Entity(value.to_string())),
            "file" => return Ok(Query::File(value.to_string())),
            _ => return Err(format!("Unknown predicate key: {}", key)),
        }
    }

    Err(format!("Invalid query: {}", input))
}

/// Find operator position, respecting parentheses.
fn find_operator(input: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let bytes = input.as_bytes();
    let op_bytes = op.as_bytes();

    for i in 0..input.len() {
        if bytes[i] == b'(' {
            depth += 1;
        } else if bytes[i] == b')' {
            depth -= 1;
        } else if depth == 0 && i + op.len() <= input.len() {
            if &bytes[i..i + op.len()] == op_bytes {
                return Some(i);
            }
        }
    }

    None
}

/// Trace a path between two nodes.
pub fn trace_path(
    graph: &ArchGraph,
    from_query: &str,
    to_query: &str,
) -> Result<Vec<Vec<String>>, String> {
    let from_q = Query::parse(from_query)?;
    let to_q = Query::parse(to_query)?;

    let from_nodes = from_q.execute(graph);
    let to_nodes = to_q.execute(graph);

    if from_nodes.nodes.is_empty() {
        return Err(format!("No nodes match source query: {}", from_query));
    }
    if to_nodes.nodes.is_empty() {
        return Err(format!("No nodes match target query: {}", to_query));
    }

    let (pg, indices) = graph.to_petgraph();
    let mut paths = Vec::new();

    for from_id in &from_nodes.nodes {
        for to_id in &to_nodes.nodes {
            if let (Some(&from_idx), Some(&to_idx)) = (indices.get(from_id), indices.get(to_id)) {
                let found: Vec<Vec<_>> = all_simple_paths(&pg, from_idx, to_idx, 0, Some(10))
                    .collect();

                for path in found {
                    let path_ids: Vec<String> = path
                        .iter()
                        .map(|idx| pg[*idx].clone())
                        .collect();
                    paths.push(path_ids);
                }
            }
        }
    }

    Ok(paths)
}

/// Get architectural context for a file (for IDE/Claude integration).
pub fn get_file_context(graph: &ArchGraph, file_path: &str) -> FileContext {
    let mut context = FileContext::default();
    let mut doc_texts = Vec::new();

    for node in graph.nodes() {
        if node.file.to_string_lossy().contains(file_path) {
            if let Some(ref layer) = node.properties.layer {
                context.layer = Some(layer.clone());
            }
            context.roles.extend(node.properties.roles.clone());
            if let Some(ref thread) = node.properties.thread {
                context.thread = Some(format!("{:?}", thread));
            }
            if let Some(ref qos) = node.properties.qos {
                context.qos = Some(qos.clone());
            }
            context.produces.extend(
                node.properties
                    .produces
                    .iter()
                    .map(|m| format!("{}:{}", m.category, m.name)),
            );
            context.consumes.extend(
                node.properties
                    .consumes
                    .iter()
                    .map(|m| format!("{}:{}", m.category, m.name)),
            );
            context.patterns.extend(node.properties.patterns.clone());

            if node.properties.is_gateway {
                context.constraints.push("This is a gateway - protocol translation point".into());
            }
            if node.properties.owns_voices {
                context.constraints.push("Manages polyphonic voice state".into());
            }

            // Collect doc text, notes, and see_also
            if let Some(ref doc) = node.properties.doc {
                doc_texts.push(doc.clone());
            }
            context.notes.extend(node.properties.notes.clone());
            context.see_also.extend(node.properties.see_also.clone());
        }
    }

    // Combine doc texts from all nodes in this file
    if !doc_texts.is_empty() {
        context.doc = Some(doc_texts.join("\n\n---\n\n"));
    }

    // Add constraints based on layer/thread
    if context.layer.as_deref() == Some("vivarium") && context.thread.is_some() {
        if context.qos.as_deref() == Some("realtime") {
            context.constraints.push("NO heap allocation in this file".into());
            context.constraints.push("NO blocking operations".into());
        }
    }

    // Deduplicate
    let produces: HashSet<_> = context.produces.drain(..).collect();
    context.produces = produces.into_iter().collect();

    let consumes: HashSet<_> = context.consumes.drain(..).collect();
    context.consumes = consumes.into_iter().collect();

    let notes: HashSet<_> = context.notes.drain(..).collect();
    context.notes = notes.into_iter().collect();

    let see_also: HashSet<_> = context.see_also.drain(..).collect();
    context.see_also = see_also.into_iter().collect();

    context
}

/// Architectural context for a file (for IDE/Claude).
#[derive(Debug, Default)]
pub struct FileContext {
    pub layer: Option<String>,
    pub roles: Vec<String>,
    pub thread: Option<String>,
    pub qos: Option<String>,
    pub produces: Vec<String>,
    pub consumes: Vec<String>,
    pub patterns: Vec<String>,
    pub constraints: Vec<String>,
    /// Full doc comment text (narrative, rationale, examples)
    pub doc: Option<String>,
    /// Inline design notes (@arch:note)
    pub notes: Vec<String>,
    /// External documentation references (@arch:see)
    pub see_also: Vec<String>,
}

impl FileContext {
    /// Format as markdown for Claude context injection.
    pub fn to_markdown(&self, file_path: &str) -> String {
        let mut output = format!("## You are editing: {}\n\n", file_path);

        if let Some(ref layer) = self.layer {
            output.push_str(&format!("**Layer**: {}\n", layer));
        }
        if !self.roles.is_empty() {
            output.push_str(&format!("**Roles**: {}\n", self.roles.join(", ")));
        }
        if let Some(ref thread) = self.thread {
            output.push_str(&format!("**Thread**: {}\n", thread));
        }
        if let Some(ref qos) = self.qos {
            output.push_str(&format!("**QoS**: {}\n", qos));
        }

        if !self.produces.is_empty() {
            output.push_str(&format!("**Produces**: {}\n", self.produces.join(", ")));
        }
        if !self.consumes.is_empty() {
            output.push_str(&format!("**Consumes**: {}\n", self.consumes.join(", ")));
        }
        if !self.patterns.is_empty() {
            output.push_str(&format!("**Patterns**: {}\n", self.patterns.join(", ")));
        }

        if !self.constraints.is_empty() {
            output.push_str("\n**Architectural constraints**:\n");
            for constraint in &self.constraints {
                output.push_str(&format!("- {}\n", constraint));
            }
        }

        if !self.notes.is_empty() {
            output.push_str("\n**Design notes**:\n");
            for note in &self.notes {
                output.push_str(&format!("- {}\n", note));
            }
        }

        if !self.see_also.is_empty() {
            output.push_str("\n**See also**:\n");
            for ref_path in &self.see_also {
                output.push_str(&format!("- {}\n", ref_path));
            }
        }

        // Add doc text at the end for full context
        if let Some(ref doc) = self.doc {
            output.push_str("\n---\n\n");
            output.push_str("### Documentation\n\n");
            output.push_str(doc);
            output.push('\n');
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let q = Query::parse("layer:vivarium").unwrap();
        assert!(matches!(q, Query::Layer(l) if l == "vivarium"));
    }

    #[test]
    fn test_parse_and() {
        let q = Query::parse("layer:vivarium AND role:synthesis").unwrap();
        assert!(matches!(q, Query::And(_, _)));
    }

    #[test]
    fn test_parse_produces() {
        let q = Query::parse("produces:impulse:NoteOn").unwrap();
        assert!(matches!(q, Query::Produces(c, n) if c == "impulse" && n == "NoteOn"));
    }

    #[test]
    fn test_parse_gateway() {
        let q = Query::parse("gateway").unwrap();
        assert!(matches!(q, Query::Gateway));
    }
}
