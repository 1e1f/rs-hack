//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Side index from `NodeId` to its `AnnotationRef`s.
//!
//! Annotations live both in the graph (as Tag/Flow edges) and in this
//! side index (as typed `AnnotationRef` values). The index powers
//! `arch.node`'s `annotations` field — the UI fetches one node and gets
//! its full overlay in one round-trip without traversing the graph.

use std::collections::HashMap;
use yah_kg::anno::AnnotationRef;
use yah_kg::ids::NodeId;

#[derive(Debug, Default, Clone)]
pub struct AnnotationIndex {
    by_node: HashMap<NodeId, Vec<AnnotationRef>>,
}

impl AnnotationIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wholesale-replace the annotations attached to `node`. Called by the
    /// applier when a file is reindexed — annotations on a node are
    /// always derived from one source location, so atomic replace is
    /// the right semantics.
    pub fn set(&mut self, node: NodeId, anns: Vec<AnnotationRef>) {
        if anns.is_empty() {
            self.by_node.remove(&node);
        } else {
            self.by_node.insert(node, anns);
        }
    }

    pub fn get(&self, node: NodeId) -> &[AnnotationRef] {
        self.by_node
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn remove(&mut self, node: NodeId) {
        self.by_node.remove(&node);
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &[AnnotationRef])> {
        self.by_node.iter().map(|(k, v)| (*k, v.as_slice()))
    }

    pub fn len(&self) -> usize {
        self.by_node.values().map(|v| v.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_node.is_empty()
    }
}
