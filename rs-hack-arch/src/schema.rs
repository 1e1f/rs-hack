//! Schema definition for architectural vocabulary.
//!
//! The schema defines valid values for each annotation kind, enabling
//! validation and autocomplete in tooling.
//!
//! ## Loading from Cargo.toml
//!
//! Schema is loaded from `[workspace.metadata.arch]` in your workspace's Cargo.toml:
//!
//! ```toml
//! [workspace.metadata.arch]
//! [workspace.metadata.arch.layers]
//! core = { description = "Core library", allowed_dependencies = [] }
//! app = { description = "Application", allowed_dependencies = ["core"] }
//!
//! [workspace.metadata.arch.roles]
//! compiler = "Transforms source to executable form"
//! runtime = "Manages execution state"
//!
//! [workspace.metadata.arch.threads]
//! audio = { priority = "realtime", description = "Audio processing" }
//! main = { priority = "normal", description = "Main thread" }
//!
//! [workspace.metadata.arch.qos]
//! realtime = { max_latency_ms = 20, description = "Strict timing" }
//! best_effort = { description = "Non-critical" }
//!
//! [workspace.metadata.arch.messages]
//! impulse = ["NoteOn", "NoteOff", "SetParam"]
//! state = ["SystemState"]
//! ```

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Complete schema for architectural annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Valid architectural layers (e.g., "koda", "vivarium", "society", "spill")
    pub layers: HashMap<String, LayerDef>,

    /// Valid roles (e.g., "compiler", "synthesis", "transport", "ui")
    pub roles: HashMap<String, RoleDef>,

    /// Thread contexts with priorities and affinities
    pub threads: HashMap<String, ThreadDef>,

    /// QoS classes with latency constraints
    pub qos_classes: HashMap<String, QosDef>,

    /// Message types that can be produced/consumed
    pub message_types: HashMap<String, Vec<String>>,

    /// Named architectural patterns
    pub patterns: HashMap<String, String>,

    /// Domain-specific concepts (e.g., musical semantics)
    pub domain_concepts: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerDef {
    pub description: String,
    #[serde(default)]
    pub allowed_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDef {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDef {
    pub priority: ThreadPriority,
    #[serde(default)]
    pub affinity: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadPriority {
    Realtime,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosDef {
    pub max_latency_ms: Option<u32>,
    pub description: String,
}

impl Default for Schema {
    fn default() -> Self {
        Self::empty()
    }
}

impl Schema {
    /// Create an empty schema for custom vocabularies.
    pub fn empty() -> Self {
        Self {
            layers: HashMap::new(),
            roles: HashMap::new(),
            threads: HashMap::new(),
            qos_classes: HashMap::new(),
            message_types: HashMap::new(),
            patterns: HashMap::new(),
            domain_concepts: HashMap::new(),
        }
    }

    /// Load schema from workspace Cargo.toml metadata.
    ///
    /// Looks for `[workspace.metadata.arch]` section in the workspace's Cargo.toml.
    /// Falls back to empty schema if not found.
    pub fn from_cargo_metadata(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let workspace_root = workspace_root.as_ref();

        let metadata = MetadataCommand::new()
            .manifest_path(workspace_root.join("Cargo.toml"))
            .no_deps()
            .exec()
            .with_context(|| format!("Failed to read cargo metadata from {}", workspace_root.display()))?;

        // Look for workspace metadata first
        if let Some(arch_meta) = metadata.workspace_metadata.get("arch") {
            return Self::from_json_value(arch_meta.clone());
        }

        // Fall back to root package metadata if no workspace metadata
        if let Some(root_pkg) = metadata.root_package() {
            if let Some(arch_meta) = root_pkg.metadata.get("arch") {
                return Self::from_json_value(arch_meta.clone());
            }
        }

        // No arch metadata found, return empty schema
        Ok(Self::empty())
    }

    /// Parse schema from a serde_json::Value (from Cargo.toml metadata).
    fn from_json_value(value: serde_json::Value) -> Result<Self> {
        let mut schema = Self::empty();

        // Parse layers
        if let Some(layers) = value.get("layers") {
            if let Some(layers_obj) = layers.as_object() {
                for (name, def) in layers_obj {
                    let layer_def = if let Some(obj) = def.as_object() {
                        LayerDef {
                            description: obj.get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            allowed_dependencies: obj.get("allowed_dependencies")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect())
                                .unwrap_or_default(),
                        }
                    } else if let Some(desc) = def.as_str() {
                        LayerDef {
                            description: desc.to_string(),
                            allowed_dependencies: vec![],
                        }
                    } else {
                        continue;
                    };
                    schema.layers.insert(name.clone(), layer_def);
                }
            }
        }

        // Parse roles
        if let Some(roles) = value.get("roles") {
            if let Some(roles_obj) = roles.as_object() {
                for (name, def) in roles_obj {
                    let description = if let Some(obj) = def.as_object() {
                        obj.get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string()
                    } else if let Some(desc) = def.as_str() {
                        desc.to_string()
                    } else {
                        continue;
                    };
                    schema.roles.insert(name.clone(), RoleDef { description });
                }
            }
        }

        // Parse threads
        if let Some(threads) = value.get("threads") {
            if let Some(threads_obj) = threads.as_object() {
                for (name, def) in threads_obj {
                    if let Some(obj) = def.as_object() {
                        let priority = obj.get("priority")
                            .and_then(|v| v.as_str())
                            .map(|s| match s {
                                "realtime" => ThreadPriority::Realtime,
                                "high" => ThreadPriority::High,
                                "low" => ThreadPriority::Low,
                                _ => ThreadPriority::Normal,
                            })
                            .unwrap_or(ThreadPriority::Normal);

                        schema.threads.insert(name.clone(), ThreadDef {
                            priority,
                            affinity: obj.get("affinity").and_then(|v| v.as_str()).map(String::from),
                            description: obj.get("description").and_then(|v| v.as_str()).map(String::from),
                        });
                    }
                }
            }
        }

        // Parse QoS classes
        if let Some(qos) = value.get("qos") {
            if let Some(qos_obj) = qos.as_object() {
                for (name, def) in qos_obj {
                    if let Some(obj) = def.as_object() {
                        schema.qos_classes.insert(name.clone(), QosDef {
                            max_latency_ms: obj.get("max_latency_ms")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32),
                            description: obj.get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        });
                    } else if let Some(desc) = def.as_str() {
                        schema.qos_classes.insert(name.clone(), QosDef {
                            max_latency_ms: None,
                            description: desc.to_string(),
                        });
                    }
                }
            }
        }

        // Parse message types
        if let Some(messages) = value.get("messages") {
            if let Some(messages_obj) = messages.as_object() {
                for (category, types) in messages_obj {
                    if let Some(arr) = types.as_array() {
                        let type_names: Vec<String> = arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        schema.message_types.insert(category.clone(), type_names);
                    }
                }
            }
        }

        // Parse patterns
        if let Some(patterns) = value.get("patterns") {
            if let Some(patterns_obj) = patterns.as_object() {
                for (name, desc) in patterns_obj {
                    if let Some(desc_str) = desc.as_str() {
                        schema.patterns.insert(name.clone(), desc_str.to_string());
                    }
                }
            }
        }

        // Parse domain concepts
        if let Some(domain) = value.get("domain_concepts") {
            if let Some(domain_obj) = domain.as_object() {
                for (name, desc) in domain_obj {
                    if let Some(desc_str) = desc.as_str() {
                        schema.domain_concepts.insert(name.clone(), desc_str.to_string());
                    }
                }
            }
        }

        Ok(schema)
    }

    /// Load schema from TOML file.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Serialize schema to TOML.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Check if a layer name is valid.
    pub fn is_valid_layer(&self, name: &str) -> bool {
        self.layers.contains_key(name)
    }

    /// Check if a role name is valid.
    pub fn is_valid_role(&self, name: &str) -> bool {
        self.roles.contains_key(name)
    }

    /// Check if a thread name is valid.
    pub fn is_valid_thread(&self, name: &str) -> bool {
        self.threads.contains_key(name)
    }

    /// Check if a message type is valid.
    pub fn is_valid_message(&self, category: &str, name: &str) -> bool {
        self.message_types
            .get(category)
            .map(|types| types.iter().any(|t| t == name || t == "*"))
            .unwrap_or(false)
    }

    /// Check if schema has any definitions.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
            && self.roles.is_empty()
            && self.threads.is_empty()
            && self.qos_classes.is_empty()
            && self.message_types.is_empty()
    }

    /// Format schema as a human-readable summary.
    pub fn summary(&self) -> String {
        let mut output = String::new();

        if !self.layers.is_empty() {
            output.push_str("Layers:\n");
            for (name, def) in &self.layers {
                output.push_str(&format!("  {} - {}\n", name, def.description));
                if !def.allowed_dependencies.is_empty() {
                    output.push_str(&format!("    allowed deps: {}\n", def.allowed_dependencies.join(", ")));
                }
            }
        }

        if !self.roles.is_empty() {
            output.push_str("\nRoles:\n");
            for (name, def) in &self.roles {
                output.push_str(&format!("  {} - {}\n", name, def.description));
            }
        }

        if !self.threads.is_empty() {
            output.push_str("\nThreads:\n");
            for (name, def) in &self.threads {
                output.push_str(&format!("  {} ({:?})", name, def.priority));
                if let Some(ref desc) = def.description {
                    output.push_str(&format!(" - {}", desc));
                }
                output.push('\n');
            }
        }

        if !self.qos_classes.is_empty() {
            output.push_str("\nQoS Classes:\n");
            for (name, def) in &self.qos_classes {
                output.push_str(&format!("  {}", name));
                if let Some(latency) = def.max_latency_ms {
                    output.push_str(&format!(" ({}ms)", latency));
                }
                output.push_str(&format!(" - {}\n", def.description));
            }
        }

        if !self.message_types.is_empty() {
            output.push_str("\nMessage Types:\n");
            for (category, types) in &self.message_types {
                output.push_str(&format!("  {}: {}\n", category, types.join(", ")));
            }
        }

        if output.is_empty() {
            output.push_str("(empty schema - no architecture metadata defined)\n");
        }

        output
    }
}
