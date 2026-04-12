//! @arch:layer(arch)
//! @arch:role(parser)
//!
//! Annotation data structures and parsing.
//! Defines ArchAnnotation, AnnotationTarget, ArchKind, and all
//! the typed annotation variants (layer, role, thread, qos, etc.).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single architectural annotation extracted from source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchAnnotation {
    /// Source file path
    pub file: PathBuf,

    /// Line number where annotation appears
    pub line: usize,

    /// The item this annotation is attached to (module, struct, fn, etc.)
    pub target: AnnotationTarget,

    /// The kind of annotation
    pub kind: ArchKind,

    /// Full doc comment text (non-@arch lines), if this is the first annotation for this target
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_text: Option<String>,
}

/// What the annotation is attached to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AnnotationTarget {
    /// Module-level (//! comments at top of file)
    Module { path: String },

    /// Struct definition
    Struct { name: String, module: String },

    /// Enum definition
    Enum { name: String, module: String },

    /// Function definition
    Function { name: String, module: String },

    /// Impl block
    Impl {
        self_ty: String,
        trait_name: Option<String>,
        module: String,
    },
}

/// The kind of architectural annotation and its value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchKind {
    /// @arch:layer(name)
    Layer(String),

    /// @arch:role(name)
    Role(String),

    /// @arch:thread(name) or @arch:thread(name -> name) for flow
    Thread(ThreadSpec),

    /// @arch:qos(class) or @arch:qos(class:latency)
    Qos(QosSpec),

    /// @arch:produces(type:name, type:name, ...)
    Produces(Vec<MessageSpec>),

    /// @arch:consumes(type:name, type:name, ...)
    Consumes(Vec<MessageSpec>),

    /// @arch:provides_context(Name)
    ProvidesContext(String),

    /// @arch:requires_context(Name, Name, ...)
    RequiresContext(Vec<String>),

    /// @arch:depends_on(target, reason = "...")
    DependsOn { target: String, reason: Option<String> },

    /// @arch:implements(TraitName)
    Implements(String),

    /// @arch:pattern(name)
    Pattern(String),

    /// @arch:musical(concept)
    Musical(String),

    /// @arch:gateway - marks network edge protocol translation
    Gateway,

    /// @arch:owns_voices - marks polyphonic state manager
    OwnsVoices,

    /// @arch:bridge(from -> to)
    Bridge { from: String, to: String },

    /// @arch:flow(A -> B -> C) - data flow declaration
    Flow(Vec<String>),

    /// @arch:entity(kind) - DDD-style entity classification
    Entity(String),

    /// @arch:aggregate_root - marks aggregate root
    AggregateRoot,

    /// @arch:note(text) - inline design note
    Note(String),

    /// @arch:see(path) - reference to external documentation
    See(String),

    /// Unknown annotation (preserved for extensibility)
    Unknown { key: String, value: String },
}

/// Thread specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreadSpec {
    /// Runs on specific thread
    On(String),
    /// Routes from one thread to another
    Flow { from: String, to: String },
    /// Can run on any thread
    Any,
}

/// QoS specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QosSpec {
    pub class: String,
    pub max_latency_ms: Option<u32>,
}

/// Message type specification (e.g., impulse:NoteOn).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageSpec {
    pub category: String,
    pub name: String,
}

impl MessageSpec {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() == 2 {
            Some(Self {
                category: parts[0].trim().to_string(),
                name: parts[1].trim().to_string(),
            })
        } else {
            None
        }
    }

    pub fn is_wildcard(&self) -> bool {
        self.name == "*"
    }
}

impl ArchKind {
    /// Parse an annotation from key=value or key(value) format.
    pub fn parse(key: &str, value: &str) -> Self {
        match key {
            "layer" => Self::Layer(value.to_string()),
            "role" => Self::Role(value.to_string()),
            "thread" => {
                if value.contains("->") {
                    let parts: Vec<&str> = value.split("->").map(|s| s.trim()).collect();
                    if parts.len() == 2 {
                        if parts[0] == "any" {
                            Self::Thread(ThreadSpec::Flow {
                                from: "any".into(),
                                to: parts[1].to_string(),
                            })
                        } else {
                            Self::Thread(ThreadSpec::Flow {
                                from: parts[0].to_string(),
                                to: parts[1].to_string(),
                            })
                        }
                    } else {
                        Self::Thread(ThreadSpec::On(value.to_string()))
                    }
                } else if value == "any" {
                    Self::Thread(ThreadSpec::Any)
                } else {
                    Self::Thread(ThreadSpec::On(value.to_string()))
                }
            }
            "qos" => {
                if value.contains(':') {
                    let parts: Vec<&str> = value.splitn(2, ':').collect();
                    let latency = parts.get(1).and_then(|s| {
                        s.trim_end_matches("ms").parse().ok()
                    });
                    Self::Qos(QosSpec {
                        class: parts[0].to_string(),
                        max_latency_ms: latency,
                    })
                } else {
                    Self::Qos(QosSpec {
                        class: value.to_string(),
                        max_latency_ms: None,
                    })
                }
            }
            "produces" => {
                let specs = value
                    .split(',')
                    .filter_map(|s| MessageSpec::parse(s.trim()))
                    .collect();
                Self::Produces(specs)
            }
            "consumes" => {
                let specs = value
                    .split(',')
                    .filter_map(|s| MessageSpec::parse(s.trim()))
                    .collect();
                Self::Consumes(specs)
            }
            "provides_context" => Self::ProvidesContext(value.to_string()),
            "requires_context" => {
                let contexts = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                Self::RequiresContext(contexts)
            }
            "depends_on" => {
                // Parse: "target" or "target, reason = \"...\""
                if value.contains("reason") {
                    let parts: Vec<&str> = value.splitn(2, ',').collect();
                    let target = parts[0].trim().trim_matches('"').to_string();
                    let reason = parts.get(1).and_then(|s| {
                        s.split('=').nth(1).map(|r| {
                            r.trim().trim_matches('"').to_string()
                        })
                    });
                    Self::DependsOn { target, reason }
                } else {
                    Self::DependsOn {
                        target: value.trim_matches('"').to_string(),
                        reason: None,
                    }
                }
            }
            "implements" => Self::Implements(value.to_string()),
            "pattern" => Self::Pattern(value.to_string()),
            "musical" => Self::Musical(value.to_string()),
            "gateway" => Self::Gateway,
            "owns_voices" => Self::OwnsVoices,
            "bridge" => {
                let parts: Vec<&str> = value.split("->").map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    Self::Bridge {
                        from: parts[0].to_string(),
                        to: parts[1].to_string(),
                    }
                } else {
                    Self::Unknown {
                        key: key.to_string(),
                        value: value.to_string(),
                    }
                }
            }
            "flow" => {
                let steps: Vec<String> = value
                    .split("->")
                    .map(|s| s.trim().to_string())
                    .collect();
                Self::Flow(steps)
            }
            "entity" => Self::Entity(value.to_string()),
            "aggregate_root" => Self::AggregateRoot,
            "note" => Self::Note(value.to_string()),
            "see" => Self::See(value.to_string()),
            _ => Self::Unknown {
                key: key.to_string(),
                value: value.to_string(),
            },
        }
    }
}

impl AnnotationTarget {
    /// Get a unique identifier for this target.
    pub fn id(&self) -> String {
        match self {
            Self::Module { path } => format!("mod:{}", path),
            Self::Struct { name, module } => format!("struct:{}::{}", module, name),
            Self::Enum { name, module } => format!("enum:{}::{}", module, name),
            Self::Function { name, module } => format!("fn:{}::{}", module, name),
            Self::Impl {
                self_ty,
                trait_name,
                module,
            } => {
                if let Some(trait_name) = trait_name {
                    format!("impl:{}::{}::{}", module, trait_name, self_ty)
                } else {
                    format!("impl:{}::{}", module, self_ty)
                }
            }
        }
    }

    /// Get the module path.
    pub fn module(&self) -> &str {
        match self {
            Self::Module { path } => path,
            Self::Struct { module, .. } => module,
            Self::Enum { module, .. } => module,
            Self::Function { module, .. } => module,
            Self::Impl { module, .. } => module,
        }
    }
}
