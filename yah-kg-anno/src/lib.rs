//! @arch:layer(kg_store)
//! @arch:role(extract)
//!
//! `yah-kg-anno` — annotation overlay for the structural knowledge graph.
//!
//! Reads `@yah:` directives from doc strings already attached to nodes
//! (by the language indexers' `push_doc` calls), parses them into typed
//! `AnnotationRef` values, and applies them to the graph:
//!
//! * **Tags** materialize as synthetic `Tag` nodes plus `EdgeKind::Tag`
//!   edges from the annotated structural node to the tag.
//! * **Flows** materialize as `EdgeKind::Flow` edges from the annotated
//!   node to whichever node currently matches the `to_qualified` string
//!   (suffix match on the qualified name; ambiguous matches are dropped
//!   with a warning).
//! * **Rules** are parsed but not yet validated — the v1 contract
//!   reserves the type so authors can start writing them.
//!
//! Pass 4 driver: [`apply_pass`] walks every node in the store, scans
//! its doc for annotations, and writes the result into both the
//! [`AnnotationIndex`] (side index keyed by `NodeId`) and the graph
//! (synthetic Tag nodes + Tag/Flow edges).

pub mod apply;
pub mod index;
pub mod parser;

pub use apply::{apply_pass, apply_to_node, ApplySummary, TouchedWorkItem};
pub use index::{AnnotationIndex, AnnotationIndexSnapshot};
pub use parser::{parse_doc, ParseError, RawAnnotation, WorkItemType};
