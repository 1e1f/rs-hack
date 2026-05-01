//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Aggregate graph-view DTO returned by subgraph traversals. Lives in
//! `kg` rather than `rpc` because the prelude assembler and other
//! in-process consumers operate on `Subgraph` independently of any
//! transport.

use crate::edge::EdgeOut;
use crate::ids::{NodeId, NodeRef};
use serde::{Deserialize, Serialize};

/// One bounded slice of the knowledge graph rooted at `root`.
///
/// `truncated` is `true` when the result was capped by the caller's
/// `node_limit` or `depth` — surfacing this honestly lets clients warn
/// the user instead of silently rendering a partial view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subgraph {
    pub root: NodeId,
    pub nodes: Vec<NodeRef>,
    pub edges: Vec<EdgeOut>,
    /// True if the result was capped by `node_limit` or `depth`.
    pub truncated: bool,
}
