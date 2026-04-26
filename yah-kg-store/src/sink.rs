//! @arch:layer(kg)
//! @arch:role(graph)
//!
//! `StoreSink` — `IndexSink` impl that writes into a [`Store`].
//!
//! Indexers see the trait surface from `yah-kg`; this is the concrete
//! implementation the daemon owns. It dedupes nodes/edges by id and
//! flushes properties/docs into the same side maps the queries read.

use crate::store::Store;
use yah_kg::edge::EdgeOut;
use yah_kg::ids::{NodeId, NodeRef};
use yah_kg::indexer::IndexSink;

pub struct StoreSink<'a> {
    store: &'a mut Store,
}

impl<'a> StoreSink<'a> {
    pub fn new(store: &'a mut Store) -> Self {
        Self { store }
    }
}

impl<'a> IndexSink for StoreSink<'a> {
    fn push_node(&mut self, node: NodeRef) {
        self.store.upsert_node(node);
    }

    fn push_edge(&mut self, edge: EdgeOut) {
        self.store.upsert_edge(edge);
    }

    fn push_property(&mut self, node: NodeId, key: &str, value: &str) {
        self.store
            .set_property(node, key.to_string(), value.to_string());
    }

    fn push_doc(&mut self, node: NodeId, doc: &str) {
        self.store.set_doc(node, doc.to_string());
    }
}
