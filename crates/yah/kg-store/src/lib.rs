//! @arch:layer(kg)
//! @arch:role(graph)
//!
//! `yah-kg-store` — in-memory knowledge-graph store.
//!
//! Holds a `petgraph::StableDiGraph` keyed by `NodeId`, side maps for full
//! `NodeRef`/`EdgeOut` lookup, doc/property bags, and a `(file, line)`
//! index for `arch.lookup`. Implements [`kg::IndexSink`] so language
//! indexers can write into it directly, and exposes query methods that
//! match the `arch.*` RPC surface.
//!
//! Storage decisions:
//! * `StableDiGraph` so node/edge removal preserves indices for the
//!   incremental update path.
//! * Side maps rather than node weights so cloning a `NodeRef` out of the
//!   graph is one hashmap hit, not a graph walk.
//! * `BTreeMap<String, Vec<(Span, NodeId)>>` per file for `arch.lookup`,
//!   with results returned innermost-first (smallest span containing the
//!   line, descending by nesting depth).

pub mod resolve;
pub mod sink;
pub mod snapshot;
pub mod store;
pub mod walker;

pub use resolve::{drop_imports_from, resolve_rust_imports, resolve_ts_imports};
pub use sink::StoreSink;
pub use snapshot::{SnapshotError, StoreSnapshot, STORE_SNAPSHOT_VERSION};
pub use store::{Store, StoreError};
pub use walker::{reindex_file, walk_and_index, FileDelta, IndexerRegistry, WalkSummary};
