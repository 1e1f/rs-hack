//! @arch:layer(kg)
//! @arch:role(extract)
//!
//! `LanguageIndexer` is the single extension point per supported language.
//! The daemon dispatches files to indexers based on `extensions()`.
//!
//! Indexers are pure: they read source text and push `NodeRef`/`EdgeOut`
//! values into an `IndexSink`. They do not own storage, do not own the
//! petgraph, and do not perform I/O. This keeps them trivially testable
//! and lets the daemon batch sink writes any way it likes (in-memory
//! petgraph, snapshot file, RPC fan-out).

use crate::edge::EdgeOut;
use crate::ids::{NodeId, NodeRef};
use crate::kind::Lang;
use std::path::Path;

/// One language's extractor. Object-safe; the daemon holds these as
/// `Box<dyn LanguageIndexer>` and dispatches by file extension.
pub trait LanguageIndexer: Send + Sync {
    fn lang(&self) -> Lang;

    /// File extensions this indexer claims, lowercase, without the dot.
    /// First-match wins in the daemon's dispatcher, so order indexers
    /// from most-specific to most-generic at registration time.
    fn extensions(&self) -> &[&'static str];

    /// Parse one file and push every node and edge it produces into
    /// `sink`. `path` is rig-relative; `src` is the file contents.
    ///
    /// Indexers should be deterministic: given the same `(path, src)`
    /// they must emit the same nodes (with identical `NodeId`s, which
    /// follows from using `NodeId::compute` with stable qualified names).
    fn index_file(&self, path: &Path, src: &str, sink: &mut dyn IndexSink) -> Result<(), IndexError>;
}

/// Sink an indexer pushes structural facts into.
///
/// Implementations are owned by the daemon. A typical sink builds a
/// `petgraph::DiGraph` and a `(file, line) → NodeId` lookup index in
/// the same pass.
pub trait IndexSink {
    fn push_node(&mut self, node: NodeRef);
    fn push_edge(&mut self, edge: EdgeOut);

    /// Attach a free-form key/value to a previously pushed node.
    /// Used by indexers to hang language-specific metadata
    /// (`auto = "true"` on a Rust trait, `target_kind = "method"` on
    /// a TS decorator) without bloating `NodeRef`.
    fn push_property(&mut self, node: NodeId, key: &str, value: &str);

    /// Attach a doc-comment block to a previously pushed node.
    fn push_doc(&mut self, node: NodeId, doc: &str);
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("parse error in {path}: {message}")]
    Parse { path: String, message: String },

    #[error("io error: {0}")]
    Io(String),

    #[error("indexer is not yet implemented for this kind")]
    Unimplemented,

    /// Catch-all for indexer-specific failures. The daemon logs this
    /// and skips the file rather than aborting indexing.
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _object_safe(_: &dyn LanguageIndexer) {}
    fn _sink_object_safe(_: &mut dyn IndexSink) {}
}
