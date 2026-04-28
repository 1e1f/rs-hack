//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Node kind taxonomy. A small `CommonKind` covers things that mean the
//! same thing in every language; per-language enums (`RustKind`, `TsKind`,
//! `DocKind`, `KodaKind`) carry the concepts that don't generalize.
//!
//! Cross-language consumers (UI, search) match on `CommonKind` and treat
//! the language-specific variants as opaque hue/badge data. Language-aware
//! consumers (the Rust trait/macro browser, the TS decorator inspector)
//! match on the corresponding extra.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    Rust,
    Ts,
    Yaml,
    Json,
    Toml,
    Koda,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "lang", content = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Common(CommonKind),
    Rust(RustKind),
    Ts(TsKind),
    Doc(DocKind),
    Koda(KodaKind),
}

impl NodeKind {
    /// True for nodes that act as containers in the structural graph
    /// (directories, files, modules). Useful for graph-pruning queries.
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            NodeKind::Common(CommonKind::Directory)
                | NodeKind::Common(CommonKind::File)
                | NodeKind::Common(CommonKind::Module)
                | NodeKind::Common(CommonKind::Document)
        )
    }
}

/// Universal kinds — meaning is consistent across every supported language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonKind {
    /// Filesystem directory.
    Directory,
    /// Source file. The language is on the parent `NodeRef`.
    File,
    /// Logical grouping: Rust `mod`, TS namespace, etc.
    Module,
    /// Nominal type. The language extra distinguishes struct vs class
    /// vs interface vs type-alias.
    Type,
    /// Free function.
    Function,
    /// Function bound to a type or impl.
    Method,
    /// Field of a struct/class/object schema.
    Field,
    /// Enum variant.
    Variant,
    /// Constant or static.
    Constant,
    /// Whole-file entity (used by JSON/YAML/Koda where the file IS
    /// the addressable unit).
    Document,
    /// Synthetic taxonomy node created by the annotation overlay
    /// (`@yah:tag(...)`). Tag nodes have `synthetic = true`, a fixed
    /// `file = "<tag>"` sentinel, and a stable id derived from the tag's
    /// qualified name (e.g. `tag:layer:core`). They participate in the
    /// graph through `EdgeKind::Tag` edges.
    Tag,
    /// Synthetic work-item node created from a `@yah:relay(...)` header.
    /// Qualified name is `relay:<ID>` (e.g. `relay:R042`); file is the
    /// `"<work-item>"` sentinel. Carries no source span; the source
    /// anchors point at it via `EdgeKind::Anchors`. Pass 2 of R017-F4
    /// promotes relays to first-class graph citizens so the Board UI
    /// can list / traverse them without scanning every doc.
    Relay,
    /// Synthetic work-item node created from a `@yah:ticket(...)` header.
    /// Qualified name is `ticket:<ID>` (e.g. `ticket:R042-T1`). Same
    /// `"<work-item>"` file sentinel as `Relay`. A ticket parents up
    /// to its relay via `EdgeKind::ParentItem`.
    Ticket,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "rust_kind", rename_all = "snake_case")]
pub enum RustKind {
    /// `trait Foo { ... }`. Required and provided methods become
    /// `Method` nodes linked by `Defines`.
    Trait,
    /// `impl Foo` or `impl Foo for Bar`. Modeled as a node so the graph
    /// can answer "show every impl of Iterator". See `EdgeKind::ImplFor`
    /// and `EdgeKind::ImplOfTrait`.
    Impl,
    AssocType,
    AssocConst,
    /// Macro declaration. Invocations are edges (see `EdgeKind`).
    MacroDecl(MacroFlavor),
    Lifetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroFlavor {
    /// `macro_rules!`
    Rules,
    /// `#[proc_macro_derive(Foo)]`
    ProcDerive,
    /// `#[proc_macro_attribute]`
    ProcAttr,
    /// `#[proc_macro]`
    ProcFn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "ts_kind", rename_all = "snake_case")]
pub enum TsKind {
    Interface,
    TypeAlias,
    Enum,
    Decorator,
    JsxComponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "doc_kind", rename_all = "snake_case")]
pub enum DocKind {
    /// YAML `&anchor`.
    Anchor,
    /// Structural property of a JSON/YAML document (key path).
    Property,
    /// `$ref` target.
    SchemaRef,
}

/// Koda DSL kinds — placeholder until the grammar lands.
///
/// Filled in by the `yah-kg-koda` indexer crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "koda_kind", rename_all = "snake_case")]
pub enum KodaKind {
    Placeholder,
}
