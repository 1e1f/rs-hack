//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! yah-kg — knowledge-graph contract crate.
//!
//! This crate defines the wire-shape of the yah architecture knowledge graph:
//! identity (`NodeId`, `EdgeId`, `Span`), node/edge taxonomies (`NodeKind`,
//! `EdgeKind`), the `LanguageIndexer` trait that per-language extractors
//! implement, and the `ArchEvent` stream consumed by the Tauri shell. The
//! RPC request/response shapes the daemon exposes under `arch.*` /
//! `file.*` / `dir.*` live in the [`rpc`] crate, which composes the
//! identity + edge types from this crate into wire shapes.
//!
//! Nothing here implements parsing, storage, or networking. It is a contract
//! crate so the daemon, individual indexers, and the frontend bindings can
//! depend on a stable type surface without dragging in `petgraph`, `syn`,
//! `tree-sitter`, or transport machinery.
//!
//! @yah:relay(R016, "Indexer Pass 3: TS docs + cross-file Imports/Calls + JSON/YAML")
//! @yah:assignee(agent:claude)
//! @yah:status(in-progress)
//! @yah:phase(P1)
//! @yah:parent(R013)
//! @arch:see(.yah/arch/authored/yah-roadmap-2026Q2.md)
//!
//! @yah:relay(R017, "KG features: validator + snapshot persistence + relay/ticket nodes")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R013)
//! @arch:see(.yah/arch/authored/yah-roadmap-2026Q2.md)
//!
//! @yah:ticket(R028-F2, "Per-ticket prelude assembler: (ticket, kg, arch) -> Prelude")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R028)
//! @yah:next("Pure fn assembling: ticket block + parent chain walk + KG slice + @arch:see inlines + skill list")
//! @yah:next("Output structured Prelude type with cache-control hints")
//! @yah:next("Bound KG slice by depth + token budget")
//! @yah:verify("Snapshot test: assembling Prelude for a known ticket produces stable bytes")
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//!
//! @yah:ticket(R028-F6, "Multi-sink emission: CLAUDE.md + AGENTS.md generators from Prelude")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R028)
//! @yah:next("Prelude -> CLAUDE.md renderer (Claude Code CLI fallback)")
//! @yah:next("Prelude -> AGENTS.md renderer (Cursor / Codex CLI / others)")
//! @yah:next("yah board agent-context --ticket <ID> --format claude|agents|json — CLI emit verb")
//! @yah:next("Auto-write on session open (configurable; default on)")
//! @yah:verify("yah board agent-context --ticket R028-T1 --format claude prints a CLAUDE.md with the same prelude content the SDK saw")
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//!
//! @yah:ticket(R037-F3, "Portrait manifest schema + loader (rig-local + shared merge, normalized crops)")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R037)

pub mod agent;
pub mod agent_policy;
pub mod anno;
pub mod board;
pub mod board_mutate;
pub mod edge;
pub mod event;
pub mod ids;
pub mod indexer;
pub mod kind;
pub mod prelude;
pub mod prompt;
pub mod subgraph;
pub mod timefmt;
pub mod validate;

pub use agent::{AgentEvent, Message, Role, SessionId};
pub use agent_policy::{AgentPolicyKind, AgentPolicyRule, SCHEMA_VERSION as AGENT_POLICY_SCHEMA_VERSION};
pub use anno::{AnnotationKind, AnnotationRef, TagRef};
pub use board::{
    apply_derived_relay_fields, Board, BoardItem, ChildLiveCounts, EpicStatus, FieldConflict,
    WorkItem, WorkItemAnchor,
};
pub use edge::{Direction, EdgeId, EdgeKind, EdgeOut, KodaEdge};
pub use event::{ArchEvent, ChangedField, IndexReason, IndexScope};
pub use ids::{NodeFull, NodeId, NodeRef, Span};
pub use indexer::{IndexError, IndexSink, LanguageIndexer};
pub use kind::{CommonKind, DocKind, KodaKind, Lang, MacroFlavor, NodeKind, RustKind, TsKind};
pub use prelude::{
    assemble, CacheControl, CacheTtl, Prelude, PreludeInputs, PreludeOptions, PreludeSection,
    PreludeSectionKind,
};
pub use subgraph::Subgraph;
pub use validate::{Scope, Severity, Violation};
