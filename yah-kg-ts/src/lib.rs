//! @arch:layer(kg_lang)
//! @arch:role(extract)
//!
//! `yah-kg-ts` — `LanguageIndexer` for TypeScript and TSX (with React JSX).
//!
//! Pass 1+2 within-file indexer built on `tree-sitter-typescript`. For each
//! source file we emit:
//!
//! * **Nodes:** `File`, `Module` (TS namespace / `module {}` block), `Type`
//!   (class), `Field`, `Variant` (enum member), `Function`, `Method`,
//!   `Constant` (top-level `const` / `let`), plus the TS-specific
//!   `Interface`, `TypeAlias`, `Enum`, `JsxComponent`.
//! * **Edges:** `Contains` (always), `Defines` (class/interface → method),
//!   `Extends` (class extends class, interface extends interface),
//!   `DecoratedBy` (item → decorator) when an in-file decorator is found,
//!   and `Calls` (fn/method → same-file fn for unambiguous bare-ident
//!   call sites; method calls and property-callee call sites need
//!   inference we don't have, so they're skipped).
//! * **Properties:** `type_kind`, `decorators`, `abstract`, `async`,
//!   `default_export`, `tsx`, `extends_target` (the unresolved name of an
//!   inherited class/interface — Pass 3 cross-file resolution will turn
//!   this into proper edges).
//!
//! Cross-file resolution (`Imports` between files, `import { Foo } from "./bar"`
//! → `Foo` node lookup, cross-file `Calls`) is intentionally Pass 3
//! daemon work.
//!
//! Choice of parser: `tree-sitter-typescript` over `swc_ecma_parser`. The
//! tree-sitter substrate is the same one we'll use for Python, Go, etc.,
//! so investing in the walker shape now pays off across languages. The
//! cost is matching on `&'static str` node kinds rather than typed enums;
//! we centralize those strings in `node_kind` constants below.
//!
//!

pub mod indexer;
pub mod visit;

pub use indexer::TsIndexer;
