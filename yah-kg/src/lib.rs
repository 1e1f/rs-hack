//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! yah-kg — knowledge-graph contract crate.
//!
//! This crate defines the wire-shape of the yah architecture knowledge graph:
//! identity (`NodeId`, `EdgeId`, `Span`), node/edge taxonomies (`NodeKind`,
//! `EdgeKind`), the `LanguageIndexer` trait that per-language extractors
//! implement, the RPC request/response shapes the daemon exposes under
//! `arch.*`, and the `ArchEvent` stream consumed by the Tauri shell.
//!
//! Nothing here implements parsing, storage, or networking. It is a contract
//! crate so the daemon, individual indexers, and the frontend bindings can
//! depend on a stable type surface without dragging in `petgraph`, `syn`,
//! `tree-sitter`, or transport machinery.
//!
//! @yah:relay(R016, "Indexer Pass 3: TS docs + cross-file Imports/Calls + JSON/YAML")
//! @yah:status(open)
//! @yah:phase(P1)
//! @yah:parent(R013)
//! @arch:see(architecture/yah-roadmap-2026Q2.md)
//!
//! @yah:relay(R017, "KG features: validator + snapshot persistence + relay/ticket nodes")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R013)
//! @arch:see(architecture/yah-roadmap-2026Q2.md)

pub mod anno;
pub mod board;
pub mod edge;
pub mod event;
pub mod ids;
pub mod indexer;
pub mod kind;
pub mod prompt;
pub mod rpc;
pub mod validate;

pub use anno::{AnnotationKind, AnnotationRef, TagRef};
pub use board::{Board, BoardItem, ChildLiveCounts, EpicStatus, FieldConflict};
pub use edge::{EdgeId, EdgeKind, EdgeOut, KodaEdge};
pub use event::{ArchEvent, ChangedField, IndexReason, IndexScope};
pub use ids::{NodeFull, NodeId, NodeRef, Span};
pub use indexer::{IndexError, IndexSink, LanguageIndexer};
pub use kind::{CommonKind, DocKind, KodaKind, Lang, MacroFlavor, NodeKind, RustKind, TsKind};
pub use validate::{Scope, Severity, Violation};
