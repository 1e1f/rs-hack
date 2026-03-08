//! # Architecture Knowledge Graph
//!
//! A queryable knowledge graph of Rust codebase architecture, extracted from
//! in-source annotations. Annotations are the source of truth; the graph is
//! a cached, queryable representation.
//!
//! ## Annotation Format
//!
//! Annotations use doc comments with the `@arch:` prefix:
//!
//! ```rust,ignore
//! //! @arch:layer(vivarium)
//! //! @arch:role(synthesis)
//! //! @arch:thread(audio)
//! //! @arch:qos(realtime:20ms)
//! //! @arch:produces(impulse:NoteOn, impulse:SetParam)
//! //! @arch:consumes(impulse:*, state:SystemState)
//! ```
//!
//! ## Schema Configuration
//!
//! Schema is loaded from `[workspace.metadata.arch]` in your workspace's Cargo.toml:
//!
//! ```toml,ignore
//! [workspace.metadata.arch]
//! [workspace.metadata.arch.layers]
//! core = { description = "Core library", allowed_dependencies = [] }
//! app = { description = "Application layer", allowed_dependencies = ["core"] }
//!
//! [workspace.metadata.arch.roles]
//! compiler = "Transforms source to executable form"
//! runtime = "Manages execution state"
//!
//! [[workspace.metadata.arch.rules]]
//! name = "core-independence"
//! description = "Core layer cannot depend on higher layers"
//! severity = "error"
//! type = "layer_dependency"
//! layer = "core"
//! allowed = []
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rs_hack_arch::{ArchGraph, extract_from_workspace};
//!
//! let graph = extract_from_workspace(".")?;
//! let results = graph.query("layer:core AND role:compiler")?;
//! let violations = graph.validate()?;
//! ```

pub mod annotation;
pub mod extract;
pub mod graph;
pub mod mcp;
pub mod query;
pub mod schema;
pub mod validate;

pub use annotation::{ArchAnnotation, ArchKind};
pub use extract::{extract_from_workspace, extract_from_workspace_verbose};
pub use graph::ArchGraph;
pub use query::{Query, QueryResult};
pub use schema::Schema;
pub use validate::{Rule, Violation};
