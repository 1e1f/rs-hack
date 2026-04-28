//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Edge taxonomy. Same hybrid shape as `NodeKind`: a small core of
//! universal edges plus per-language extras for concepts that don't
//! generalize (Rust impls/macros, TS extends/decorators, JSON/YAML
//! refs, Koda).
//!
//! Edge identity is a content hash of (from, to, kind) so duplicate
//! emits dedupe naturally.

use crate::ids::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub [u8; 16]);

impl EdgeId {
    pub fn compute(from: NodeId, to: NodeId, kind: &EdgeKind) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&from.0);
        hasher.update(&to.0);
        let kind_tag = serde_json::to_vec(kind).unwrap_or_default();
        hasher.update(&kind_tag);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        EdgeId(bytes)
    }
}

impl std::fmt::Debug for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::with_capacity(32);
        for byte in self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", byte);
        }
        write!(f, "EdgeId({})", s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "edge", content = "extra", rename_all = "snake_case")]
pub enum EdgeKind {
    // ---------- Universal structural ----------
    /// dir → file, file → module, module → type/fn, type → field.
    Contains,
    /// type → method, trait → method, impl → method.
    Defines,
    /// Rust `use`, TS `import`.
    Imports,
    /// Rust `pub use`, TS `export ... from`.
    ReExports,
    /// fn/method → fn/method invocation site.
    Calls,
    /// type used as parameter, return type, or field type.
    References,
    /// Materialized convenience edge: type → trait it implements.
    /// Derived from `ImplFor` + `ImplOfTrait`; emit for query speed.
    Implements,

    // ---------- Rust-specific ----------
    /// Impl → Type: the type the impl block applies to.
    ImplFor,
    /// Impl → Trait: the trait being implemented (absent for inherent impls).
    ImplOfTrait,
    /// Site → MacroDecl: function-like macro invocation.
    MacroInvokes,
    /// Type → MacroDecl: `#[derive(Foo)]`.
    DerivedBy,
    /// Item → MacroDecl: attribute macro applied.
    AttributedBy,
    /// Generic param → Trait: trait bound.
    Bounds,
    /// Synthesized item → MacroDecl that produced it. Reserved for v2
    /// when the indexer integrates `cargo expand`-style provenance.
    GeneratedBy,

    // ---------- TS-specific ----------
    /// Class extends class, interface extends interface.
    Extends,
    /// Item → Decorator.
    DecoratedBy,

    // ---------- Doc-specific (JSON / YAML) ----------
    /// `$ref`, YAML `*alias`, kustomize base, helm values lookup.
    RefersTo,
    /// Document → schema it conforms to.
    ConformsTo,

    // ---------- Annotation overlay ----------
    /// Structural node → synthetic `Tag` node. Powers the `@yah:tag(...)`
    /// taxonomy: layer membership, role assignment, aspect grouping.
    Tag,
    /// Curated relation between two structural nodes that the AST can't
    /// derive. `@yah:flow(audio::mixer → dispatch::loop)` is the canonical
    /// example — declares a meaningful coupling (shared state, planned
    /// future call, observed runtime path) the human knows about.
    Flow,
    /// Structural node → synthetic `Relay`/`Ticket` node it carries. Drawn
    /// by the annotation applier when a doc string contains `@yah:relay`
    /// or `@yah:ticket` directives. Lets the Board UI ask "which file/
    /// item hosts this ticket?" by walking incoming `Anchors` edges on
    /// the synthetic node.
    Anchors,
    /// Work-item parent: synthetic `Ticket` → its parent `Relay`, or
    /// `Relay` → parent `Relay` (zone hierarchy). Drawn from the
    /// `@yah:parent(...)` directive on the work item.
    ParentItem,

    // ---------- Koda extension slot ----------
    Koda(KodaEdge),
}

/// Koda-specific edge kinds. Placeholder until the DSL grammar lands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "koda_edge", rename_all = "snake_case")]
pub enum KodaEdge {
    Placeholder,
}

/// Shape returned by subgraph and neighbor queries. Carries the edge
/// id (so the UI can request removal / inspect annotations) and any
/// annotation overlay refs from `rs-hack-arch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeOut {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::Lang;

    #[test]
    fn edge_id_dedupes_same_triple() {
        let a = NodeId::compute(Lang::Rust, "A", "a.rs");
        let b = NodeId::compute(Lang::Rust, "B", "b.rs");
        let e1 = EdgeId::compute(a, b, &EdgeKind::Calls);
        let e2 = EdgeId::compute(a, b, &EdgeKind::Calls);
        assert_eq!(e1, e2);
    }

    #[test]
    fn edge_id_distinguishes_kind() {
        let a = NodeId::compute(Lang::Rust, "A", "a.rs");
        let b = NodeId::compute(Lang::Rust, "B", "b.rs");
        let e1 = EdgeId::compute(a, b, &EdgeKind::Calls);
        let e2 = EdgeId::compute(a, b, &EdgeKind::Imports);
        assert_ne!(e1, e2);
    }
}
