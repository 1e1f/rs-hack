//! @arch:layer(kg)
//! @arch:role(extract)
//!
//! `yah-kg-rust` — `LanguageIndexer` for Rust source.
//!
//! Pass 1+2 within-file indexer. Given a `.rs` file, parses it with
//! `syn::parse_file` and emits:
//!
//! * **Nodes:** `File`, `Module`, `Type` (struct/enum/union/type-alias),
//!   `Field`, `Variant`, `Function`, `Method`, `Trait`, `Impl`,
//!   `Constant`, `MacroDecl(Rules)`.
//! * **Edges:** `Contains` (file→item, module→item, type→field/variant,
//!   trait→method, impl→method), `Defines` (trait/impl→method),
//!   `ImplFor` (impl→type), `ImplOfTrait` (impl→trait), `DerivedBy`
//!   (type→derive macro), `AttributedBy` (item→attr macro), `Calls`
//!   (fn/method → same-module fn for unambiguous simple-ident calls).
//!
//! Cross-file resolution remains in the daemon: this crate stops at the
//! file boundary and emits raw `use` paths onto the file node's
//! `imports` property (newline-joined). The store's
//! `resolve_rust_imports` pass (Pass 3) walks every file with that
//! property, expands `crate::`/`super::`/`self::` against the on-disk
//! module tree, and emits `Imports` edges between File nodes.
//! Item-level `Imports` and cross-crate `Calls` resolution are still
//! future Pass 3+ work.
//!
//! `Calls` edges are emitted only for unambiguous in-file resolutions:
//! a single-ident path call (`foo()`) resolves to a same-module top-
//! level `fn foo`, and the store drops the edge if no such node was
//! emitted. Method calls (`x.foo()`) and multi-segment paths
//! (`Foo::new()`) need type inference we don't have, so they're
//! deliberately skipped rather than guessed at.
//!
//! Within-file simple-name lookups (`impl Foo` where `struct Foo` lives
//! in the same module) resolve correctly because `NodeId::compute` is
//! deterministic over `(lang, qualified, file)`.
//!
//! @yah:ticket(R016-F4, "Best-effort Calls edges (Rust + TS): walk function bodies, emit when callee resolves unambiguously")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R016)
//! @yah:next("Skip ambiguous resolves rather than emit wrong edges")
//! @yah:next("Test fixture: hand-pick a few clear single-resolution call sites and assert edge presence")

pub mod indexer;
pub mod visit;

pub use indexer::RustIndexer;
