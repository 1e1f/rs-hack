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
//!   (type→derive macro), `AttributedBy` (item→attr macro).
//!
//! Cross-file resolution is **not** done here: `Imports`, `Calls`, and
//! resolution of `super::Foo` / `crate::other::Foo` paths are Pass 3+
//! daemon work. Within-file simple-name lookups (`impl Foo` where
//! `struct Foo` lives in the same module) do resolve correctly because
//! `NodeId::compute` is deterministic over `(lang, qualified, file)`.

pub mod indexer;
pub mod visit;

pub use indexer::RustIndexer;
