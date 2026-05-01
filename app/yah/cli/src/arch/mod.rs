//! @arch:layer(arch)
//! @arch:role(extract)
//! @arch:role(graph)
//! @arch:role(query)
//!
//! # Architecture Knowledge Graph
//!
//! A queryable knowledge graph of Rust codebase architecture, extracted from
//! in-source annotations. Annotations are the source of truth; the graph is
//! a cached, queryable representation.
//!
//! Authoring vocabulary is documented in `[workspace.metadata.arch.tag_namespaces]`
//! and `[workspace.metadata.arch.rule_vocabulary]` in the workspace Cargo.toml.
//! Rule enforcement lives in the `yah-kg-validator` crate, which evaluates
//! `@yah:rule(...)` directives against the live KG.

pub mod annotation;
pub mod archive;
pub mod comment;
pub mod extract;
pub mod graph;
pub mod mcp;
pub mod promote;
pub mod query;
pub mod sdlc;
pub mod smell;
pub mod status;
pub mod summary;
pub mod ticket;

