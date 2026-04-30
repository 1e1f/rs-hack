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
//!
//! @yah:ticket(R033-F15, "yah-kg-anno-wasm: wasm-bindgen target on annotation parser")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P5)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:handoff("yah-kg-anno-wasm crate landed: wasm-bindgen wrapper around yah_kg_anno::parse_doc, exposed as parseDoc(doc:string) -> {annotations, errors}. To keep the wasm bundle tight, yah-kg-anno gained an apply feature (default-on) that gates the apply/index modules + yah-kg-store dep; wasm crate consumes default-features=false so it pulls only yah-kg + parser.rs. Added Serialize/Deserialize to RawAnnotation + ParsedAnnotation so they cross the bindgen boundary via serde_wasm_bindgen. ParseError flattens into a wire DTO {kind, line, message} for ergonomic TS consumption. wasm-opt disabled in package.metadata.wasm-pack (bundled wasm-opt predates bulk-memory ops emitted by wasm-bindgen 0.2.105). Native tests green: yah-kg-anno (16/16 pass), yah-kg-anno-wasm (2/2 pass), workspace cargo check clean. wasm-pack build --target web produces 63KB wasm + JS glue + .d.ts in pkg/.")
//! @yah:next("R033-T16: wire pkg/ output as DiagnosticCollection in the KG-overlay extension — that ticket is the consumer, this one just ships the crate")
//! @yah:next("Optional follow-up: add wasm-bindgen-test integration once a browser/Node test runner lands in the workspace; native tests already cover the wrapper logic")
//! @yah:verify("cargo test -p yah-kg-anno-wasm")
//! @yah:verify("cargo test -p yah-kg-anno")
//! @yah:verify("cargo check --workspace")
//! @yah:verify("cd yah-kg-anno-wasm && wasm-pack build --target web --out-dir pkg")

#[cfg(feature = "apply")]
pub mod apply;
#[cfg(feature = "apply")]
pub mod index;
pub mod parser;

#[cfg(feature = "apply")]
pub use apply::{apply_pass, apply_to_node, ApplySummary, TouchedWorkItem};
#[cfg(feature = "apply")]
pub use index::{AnnotationIndex, AnnotationIndexSnapshot};
pub use parser::{parse_doc, ParseError, ParsedAnnotation, RawAnnotation, WorkItemType};
