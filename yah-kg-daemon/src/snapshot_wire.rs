//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Snapshot-only wire types for postcard.
//!
//! Postcard is a positional binary format: it neither tolerates
//! `skip_serializing_if` (skipping a field corrupts the field stream)
//! nor adjacently/internally-tagged enums (the discriminator is encoded
//! by serde as a synthetic field that postcard can't round-trip without
//! self-description). Both patterns are pervasive in the canonical
//! domain types because the RPC wire is JSON-shaped.
//!
//! This module mirrors every type that lands inside [`super::KgSnapshot`],
//! re-derives serde with externally-tagged enums and no skip attributes,
//! and provides pack/unpack methods back to the canonical types.
//! The RPC wire is untouched.
//!
//! v4 introduces a top-level [`StringInterner`] table (`strings: Vec<String>`)
//! and replaces high-redundancy String fields (NodeRef.file/qualified/label,
//! property keys, annotation source_file, ticket id/parent/etc.) with `u32`
//! indices into that table. Postcard varint-encodes small u32s in 1-2 bytes
//! vs. the full string's `length-prefix + bytes` cost per occurrence — and
//! ~1900 unique source paths get repeated across ~9000 nodes, so the win
//! compounds. Long unique strings (doc bodies, ticket title/handoff/next text)
//! stay inline since they don't dedup.

use crate::snapshot::{FileFingerprint, KgSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use yah_kg::agent_policy::AgentPolicyRule;
use yah_kg::anno::{
    AnnotationKind, AnnotationRef, EngineRef, TagRef, ThinkBudget, TicketStatus, WorkItemAnno,
};
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut, KodaEdge};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::kind::{
    CommonKind, DocKind, KodaKind, Lang, MacroFlavor, NodeKind, RustKind, TsKind,
};

// ---------- String interning ----------

/// Interned-string id. `u32` is plenty (snapshots have ≤ a few hundred
/// thousand unique strings; postcard varint-encodes small ids in 1-2 bytes).
pub type Sid = u32;

/// Interned NodeId index. Each NodeId is 16 bytes; replacing each
/// occurrence with a 2-3 byte varint into a single `Vec<NodeId>` table
/// is a major space win because the same node ids appear once per node,
/// twice per edge (from/to), once per anno, once per property entry, etc.
pub type Nid = u32;

/// Build-side string table: dedupes strings as they're inserted, hands
/// out stable `Sid`s. Drains into `Vec<String>` for the wire format.
#[derive(Default)]
pub struct StringInterner {
    table: Vec<String>,
    index: HashMap<String, Sid>,
}

impl StringInterner {
    pub fn intern(&mut self, s: String) -> Sid {
        if let Some(&id) = self.index.get(&s) {
            return id;
        }
        let id = self.table.len() as Sid;
        self.index.insert(s.clone(), id);
        self.table.push(s);
        id
    }

    pub fn intern_opt(&mut self, s: Option<String>) -> Option<Sid> {
        s.map(|x| self.intern(x))
    }

    pub fn intern_vec(&mut self, v: Vec<String>) -> Vec<Sid> {
        v.into_iter().map(|x| self.intern(x)).collect()
    }

    pub fn into_table(self) -> Vec<String> {
        self.table
    }
}

/// Read-side string table: clones owned strings out by index. Caller
/// owns the table and lends it for the duration of `unpack`.
pub struct StringResolver<'a> {
    table: &'a [String],
}

impl<'a> StringResolver<'a> {
    pub fn new(table: &'a [String]) -> Self {
        Self { table }
    }

    pub fn get(&self, id: Sid) -> String {
        self.table[id as usize].clone()
    }

    pub fn get_opt(&self, id: Option<Sid>) -> Option<String> {
        id.map(|i| self.get(i))
    }

    pub fn get_vec(&self, ids: Vec<Sid>) -> Vec<String> {
        ids.into_iter().map(|i| self.get(i)).collect()
    }
}

/// Build-side NodeId table: dedupes NodeIds (each is 16 bytes; many
/// occurrences across nodes/edges/annos/props all collapse to one entry).
#[derive(Default)]
pub struct NodeIdInterner {
    table: Vec<NodeId>,
    index: HashMap<NodeId, Nid>,
}

impl NodeIdInterner {
    pub fn intern(&mut self, n: NodeId) -> Nid {
        if let Some(&id) = self.index.get(&n) {
            return id;
        }
        let id = self.table.len() as Nid;
        self.index.insert(n, id);
        self.table.push(n);
        id
    }

    pub fn into_table(self) -> Vec<NodeId> {
        self.table
    }
}

/// Read-side NodeId table — `NodeId` is `Copy`, so resolution is a
/// cheap memcpy with no allocation (unlike strings).
pub struct NodeIdResolver<'a> {
    table: &'a [NodeId],
}

impl<'a> NodeIdResolver<'a> {
    pub fn new(table: &'a [NodeId]) -> Self {
        Self { table }
    }

    pub fn get(&self, id: Nid) -> NodeId {
        self.table[id as usize]
    }
}

// ---------- NodeKind tree (no strings — plain From) ----------

#[derive(Serialize, Deserialize)]
pub enum NodeKindWire {
    Common(CommonKind),
    Rust(RustKindWire),
    Ts(TsKindWire),
    Doc(DocKindWire),
    Koda(KodaKindWire),
}

impl From<NodeKind> for NodeKindWire {
    fn from(k: NodeKind) -> Self {
        match k {
            NodeKind::Common(v) => NodeKindWire::Common(v),
            NodeKind::Rust(v) => NodeKindWire::Rust(v.into()),
            NodeKind::Ts(v) => NodeKindWire::Ts(v.into()),
            NodeKind::Doc(v) => NodeKindWire::Doc(v.into()),
            NodeKind::Koda(v) => NodeKindWire::Koda(v.into()),
        }
    }
}

impl From<NodeKindWire> for NodeKind {
    fn from(k: NodeKindWire) -> Self {
        match k {
            NodeKindWire::Common(v) => NodeKind::Common(v),
            NodeKindWire::Rust(v) => NodeKind::Rust(v.into()),
            NodeKindWire::Ts(v) => NodeKind::Ts(v.into()),
            NodeKindWire::Doc(v) => NodeKind::Doc(v.into()),
            NodeKindWire::Koda(v) => NodeKind::Koda(v.into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum TsKindWire {
    Interface,
    TypeAlias,
    Enum,
    Decorator,
    JsxComponent,
}

impl From<TsKind> for TsKindWire {
    fn from(k: TsKind) -> Self {
        match k {
            TsKind::Interface => TsKindWire::Interface,
            TsKind::TypeAlias => TsKindWire::TypeAlias,
            TsKind::Enum => TsKindWire::Enum,
            TsKind::Decorator => TsKindWire::Decorator,
            TsKind::JsxComponent => TsKindWire::JsxComponent,
        }
    }
}

impl From<TsKindWire> for TsKind {
    fn from(k: TsKindWire) -> Self {
        match k {
            TsKindWire::Interface => TsKind::Interface,
            TsKindWire::TypeAlias => TsKind::TypeAlias,
            TsKindWire::Enum => TsKind::Enum,
            TsKindWire::Decorator => TsKind::Decorator,
            TsKindWire::JsxComponent => TsKind::JsxComponent,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum DocKindWire {
    Anchor,
    Property,
    SchemaRef,
}

impl From<DocKind> for DocKindWire {
    fn from(k: DocKind) -> Self {
        match k {
            DocKind::Anchor => DocKindWire::Anchor,
            DocKind::Property => DocKindWire::Property,
            DocKind::SchemaRef => DocKindWire::SchemaRef,
        }
    }
}

impl From<DocKindWire> for DocKind {
    fn from(k: DocKindWire) -> Self {
        match k {
            DocKindWire::Anchor => DocKind::Anchor,
            DocKindWire::Property => DocKind::Property,
            DocKindWire::SchemaRef => DocKind::SchemaRef,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum KodaKindWire {
    Placeholder,
}

impl From<KodaKind> for KodaKindWire {
    fn from(k: KodaKind) -> Self {
        match k {
            KodaKind::Placeholder => KodaKindWire::Placeholder,
        }
    }
}

impl From<KodaKindWire> for KodaKind {
    fn from(k: KodaKindWire) -> Self {
        match k {
            KodaKindWire::Placeholder => KodaKind::Placeholder,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum RustKindWire {
    Trait,
    Impl,
    AssocType,
    AssocConst,
    MacroDecl(MacroFlavor),
    Lifetime,
}

impl From<RustKind> for RustKindWire {
    fn from(k: RustKind) -> Self {
        match k {
            RustKind::Trait => RustKindWire::Trait,
            RustKind::Impl => RustKindWire::Impl,
            RustKind::AssocType => RustKindWire::AssocType,
            RustKind::AssocConst => RustKindWire::AssocConst,
            RustKind::MacroDecl(f) => RustKindWire::MacroDecl(f),
            RustKind::Lifetime => RustKindWire::Lifetime,
        }
    }
}

impl From<RustKindWire> for RustKind {
    fn from(k: RustKindWire) -> Self {
        match k {
            RustKindWire::Trait => RustKind::Trait,
            RustKindWire::Impl => RustKind::Impl,
            RustKindWire::AssocType => RustKind::AssocType,
            RustKindWire::AssocConst => RustKind::AssocConst,
            RustKindWire::MacroDecl(f) => RustKind::MacroDecl(f),
            RustKindWire::Lifetime => RustKind::Lifetime,
        }
    }
}

// ---------- EdgeKind tree (no strings — plain From) ----------

#[derive(Serialize, Deserialize)]
pub enum EdgeKindWire {
    Contains,
    Defines,
    Imports,
    ReExports,
    Calls,
    References,
    Implements,
    ImplFor,
    ImplOfTrait,
    MacroInvokes,
    DerivedBy,
    AttributedBy,
    Bounds,
    GeneratedBy,
    Extends,
    DecoratedBy,
    RefersTo,
    ConformsTo,
    Tag,
    Flow,
    Anchors,
    ParentItem,
    Koda(KodaEdgeWire),
}

impl From<EdgeKind> for EdgeKindWire {
    fn from(k: EdgeKind) -> Self {
        match k {
            EdgeKind::Contains => EdgeKindWire::Contains,
            EdgeKind::Defines => EdgeKindWire::Defines,
            EdgeKind::Imports => EdgeKindWire::Imports,
            EdgeKind::ReExports => EdgeKindWire::ReExports,
            EdgeKind::Calls => EdgeKindWire::Calls,
            EdgeKind::References => EdgeKindWire::References,
            EdgeKind::Implements => EdgeKindWire::Implements,
            EdgeKind::ImplFor => EdgeKindWire::ImplFor,
            EdgeKind::ImplOfTrait => EdgeKindWire::ImplOfTrait,
            EdgeKind::MacroInvokes => EdgeKindWire::MacroInvokes,
            EdgeKind::DerivedBy => EdgeKindWire::DerivedBy,
            EdgeKind::AttributedBy => EdgeKindWire::AttributedBy,
            EdgeKind::Bounds => EdgeKindWire::Bounds,
            EdgeKind::GeneratedBy => EdgeKindWire::GeneratedBy,
            EdgeKind::Extends => EdgeKindWire::Extends,
            EdgeKind::DecoratedBy => EdgeKindWire::DecoratedBy,
            EdgeKind::RefersTo => EdgeKindWire::RefersTo,
            EdgeKind::ConformsTo => EdgeKindWire::ConformsTo,
            EdgeKind::Tag => EdgeKindWire::Tag,
            EdgeKind::Flow => EdgeKindWire::Flow,
            EdgeKind::Anchors => EdgeKindWire::Anchors,
            EdgeKind::ParentItem => EdgeKindWire::ParentItem,
            EdgeKind::Koda(v) => EdgeKindWire::Koda(v.into()),
        }
    }
}

impl From<EdgeKindWire> for EdgeKind {
    fn from(k: EdgeKindWire) -> Self {
        match k {
            EdgeKindWire::Contains => EdgeKind::Contains,
            EdgeKindWire::Defines => EdgeKind::Defines,
            EdgeKindWire::Imports => EdgeKind::Imports,
            EdgeKindWire::ReExports => EdgeKind::ReExports,
            EdgeKindWire::Calls => EdgeKind::Calls,
            EdgeKindWire::References => EdgeKind::References,
            EdgeKindWire::Implements => EdgeKind::Implements,
            EdgeKindWire::ImplFor => EdgeKind::ImplFor,
            EdgeKindWire::ImplOfTrait => EdgeKind::ImplOfTrait,
            EdgeKindWire::MacroInvokes => EdgeKind::MacroInvokes,
            EdgeKindWire::DerivedBy => EdgeKind::DerivedBy,
            EdgeKindWire::AttributedBy => EdgeKind::AttributedBy,
            EdgeKindWire::Bounds => EdgeKind::Bounds,
            EdgeKindWire::GeneratedBy => EdgeKind::GeneratedBy,
            EdgeKindWire::Extends => EdgeKind::Extends,
            EdgeKindWire::DecoratedBy => EdgeKind::DecoratedBy,
            EdgeKindWire::RefersTo => EdgeKind::RefersTo,
            EdgeKindWire::ConformsTo => EdgeKind::ConformsTo,
            EdgeKindWire::Tag => EdgeKind::Tag,
            EdgeKindWire::Flow => EdgeKind::Flow,
            EdgeKindWire::Anchors => EdgeKind::Anchors,
            EdgeKindWire::ParentItem => EdgeKind::ParentItem,
            EdgeKindWire::Koda(v) => EdgeKind::Koda(v.into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum KodaEdgeWire {
    Placeholder,
}

impl From<KodaEdge> for KodaEdgeWire {
    fn from(k: KodaEdge) -> Self {
        match k {
            KodaEdge::Placeholder => KodaEdgeWire::Placeholder,
        }
    }
}

impl From<KodaEdgeWire> for KodaEdge {
    fn from(k: KodaEdgeWire) -> Self {
        match k {
            KodaEdgeWire::Placeholder => KodaEdge::Placeholder,
        }
    }
}

// ---------- NodeRef ----------
//
// `file` interns very effectively (~1900 unique paths × ~5 nodes/file).
// `label` and `qualified` are mostly unique (per-symbol names), so
// keep them inline — interning unique strings adds Sid-varint overhead
// per occurrence and forces an allocation+clone on unpack.

#[derive(Serialize, Deserialize)]
pub struct NodeRefWire {
    pub id: Nid,
    pub lang: Lang,
    pub kind: NodeKindWire,
    pub label: String,
    pub qualified: String,
    pub file: Sid,
    pub span: Span,
    pub synthetic: bool,
}

impl NodeRefWire {
    pub fn pack(n: NodeRef, ix: &mut StringInterner, nx: &mut NodeIdInterner) -> Self {
        Self {
            id: nx.intern(n.id),
            lang: n.lang,
            kind: n.kind.into(),
            label: n.label,
            qualified: n.qualified,
            file: ix.intern(n.file),
            span: n.span,
            synthetic: n.synthetic,
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>, nr: &NodeIdResolver<'_>) -> NodeRef {
        NodeRef {
            id: nr.get(self.id),
            lang: self.lang,
            kind: self.kind.into(),
            label: self.label,
            qualified: self.qualified,
            file: r.get(self.file),
            span: self.span,
            synthetic: self.synthetic,
        }
    }
}

// ---------- EdgeOut ----------

#[derive(Serialize, Deserialize)]
pub struct EdgeOutWire {
    pub id: EdgeId,
    pub from: Nid,
    pub to: Nid,
    pub kind: EdgeKindWire,
    pub annotations: Vec<Sid>,
}

impl EdgeOutWire {
    pub fn pack(
        e: EdgeOut,
        ix: &mut StringInterner,
        nx: &mut NodeIdInterner,
    ) -> Self {
        Self {
            id: e.id,
            from: nx.intern(e.from),
            to: nx.intern(e.to),
            kind: e.kind.into(),
            annotations: ix.intern_vec(e.annotations),
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>, nr: &NodeIdResolver<'_>) -> EdgeOut {
        EdgeOut {
            id: self.id,
            from: nr.get(self.from),
            to: nr.get(self.to),
            kind: self.kind.into(),
            annotations: r.get_vec(self.annotations),
        }
    }
}

// ---------- AnnotationKind tree ----------

#[derive(Serialize, Deserialize)]
pub enum AnnotationKindWire {
    Tag(TagRefWire),
    Flow {
        to_qualified: Sid,
        reason: Option<Sid>,
    },
    Rule {
        rule_kind: Sid,
        args: Vec<Sid>,
    },
    Relay(WorkItemAnnoWire),
    Ticket(WorkItemAnnoWire),
}

impl AnnotationKindWire {
    pub fn pack(k: AnnotationKind, ix: &mut StringInterner, _nx: &mut NodeIdInterner) -> Self {
        match k {
            AnnotationKind::Tag(t) => AnnotationKindWire::Tag(TagRefWire::pack(t, ix)),
            AnnotationKind::Flow {
                to_qualified,
                reason,
            } => AnnotationKindWire::Flow {
                to_qualified: ix.intern(to_qualified),
                reason: ix.intern_opt(reason),
            },
            AnnotationKind::Rule { rule_kind, args } => AnnotationKindWire::Rule {
                rule_kind: ix.intern(rule_kind),
                args: ix.intern_vec(args),
            },
            AnnotationKind::Relay(w) => AnnotationKindWire::Relay(WorkItemAnnoWire::pack(w, ix)),
            AnnotationKind::Ticket(w) => AnnotationKindWire::Ticket(WorkItemAnnoWire::pack(w, ix)),
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>, _nr: &NodeIdResolver<'_>) -> AnnotationKind {
        match self {
            AnnotationKindWire::Tag(t) => AnnotationKind::Tag(t.unpack(r)),
            AnnotationKindWire::Flow {
                to_qualified,
                reason,
            } => AnnotationKind::Flow {
                to_qualified: r.get(to_qualified),
                reason: r.get_opt(reason),
            },
            AnnotationKindWire::Rule { rule_kind, args } => AnnotationKind::Rule {
                rule_kind: r.get(rule_kind),
                args: r.get_vec(args),
            },
            AnnotationKindWire::Relay(w) => AnnotationKind::Relay(w.unpack(r)),
            AnnotationKindWire::Ticket(w) => AnnotationKind::Ticket(w.unpack(r)),
        }
    }
}

// ---------- TagRef ----------

#[derive(Serialize, Deserialize)]
pub struct TagRefWire {
    pub namespace: Option<Sid>,
    pub name: Sid,
}

impl TagRefWire {
    pub fn pack(t: TagRef, ix: &mut StringInterner) -> Self {
        Self {
            namespace: ix.intern_opt(t.namespace),
            name: ix.intern(t.name),
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>) -> TagRef {
        TagRef {
            namespace: r.get_opt(self.namespace),
            name: r.get(self.name),
        }
    }
}

// ---------- WorkItemAnno ----------
//
// id/kind/assignee/parent/phase/severity are short, repeated across many
// tickets — interned. title/handoff/next_steps/gotchas/assumes/verify/
// cleanup/see_also are typically long and ticket-specific — keep inline.

#[derive(Serialize, Deserialize)]
pub struct WorkItemAnnoWire {
    pub id: Sid,
    pub title: String,
    pub kind: Option<Sid>,
    pub status: Option<TicketStatus>,
    /// `@yah:at(<rfc3339>)` — wall-clock timestamp; not interned (high
    /// cardinality, low repetition). Defaulted on read for back-compat
    /// with snapshots written before the field landed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    pub assignee: Option<Sid>,
    pub parent: Option<Sid>,
    pub phase: Option<Sid>,
    pub severity: Option<Sid>,
    pub handoff: Vec<String>,
    pub next_steps: Vec<String>,
    pub gotchas: Vec<String>,
    pub assumes: Vec<String>,
    pub verify: Vec<String>,
    pub cleanup: Vec<String>,
    pub see_also: Vec<Sid>,
    pub think: Option<ThinkBudgetWire>,
    pub engine: Option<EngineRefWire>,
    /// Agent-policy rules folded onto this work-item by the parser.
    /// Round-tripped inline — rules are short, don't repeat across many
    /// tickets, and ride the schema_version field on each rule for
    /// forward compat (see [`yah_kg::agent_policy::SCHEMA_VERSION`]).
    /// Defaulted on read so older snapshots without the field still
    /// load; serialized only when non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_policy: Vec<AgentPolicyRule>,
}

impl WorkItemAnnoWire {
    pub fn pack(w: WorkItemAnno, ix: &mut StringInterner) -> Self {
        Self {
            id: ix.intern(w.id),
            title: w.title,
            kind: ix.intern_opt(w.kind),
            status: w.status,
            at: w.at,
            assignee: ix.intern_opt(w.assignee),
            parent: ix.intern_opt(w.parent),
            phase: ix.intern_opt(w.phase),
            severity: ix.intern_opt(w.severity),
            handoff: w.handoff,
            next_steps: w.next_steps,
            gotchas: w.gotchas,
            assumes: w.assumes,
            verify: w.verify,
            cleanup: w.cleanup,
            see_also: ix.intern_vec(w.see_also),
            think: w.think.map(Into::into),
            engine: w.engine.map(|e| EngineRefWire::pack(e, ix)),
            agent_policy: w.agent_policy,
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>) -> WorkItemAnno {
        WorkItemAnno {
            id: r.get(self.id),
            title: self.title,
            kind: r.get_opt(self.kind),
            status: self.status,
            at: self.at,
            assignee: r.get_opt(self.assignee),
            parent: r.get_opt(self.parent),
            phase: r.get_opt(self.phase),
            severity: r.get_opt(self.severity),
            handoff: self.handoff,
            next_steps: self.next_steps,
            gotchas: self.gotchas,
            assumes: self.assumes,
            verify: self.verify,
            cleanup: self.cleanup,
            see_also: r.get_vec(self.see_also),
            think: self.think.map(Into::into),
            engine: self.engine.map(|e| e.unpack(r)),
            agent_policy: self.agent_policy,
        }
    }
}

// ---------- ThinkBudget (no strings — plain From) ----------

#[derive(Serialize, Deserialize)]
pub enum ThinkBudgetWire {
    Deep,
    Standard,
    Fast,
    Budget { tokens: u32 },
}

impl From<ThinkBudget> for ThinkBudgetWire {
    fn from(t: ThinkBudget) -> Self {
        match t {
            ThinkBudget::Deep => ThinkBudgetWire::Deep,
            ThinkBudget::Standard => ThinkBudgetWire::Standard,
            ThinkBudget::Fast => ThinkBudgetWire::Fast,
            ThinkBudget::Budget { tokens } => ThinkBudgetWire::Budget { tokens },
        }
    }
}

impl From<ThinkBudgetWire> for ThinkBudget {
    fn from(t: ThinkBudgetWire) -> Self {
        match t {
            ThinkBudgetWire::Deep => ThinkBudget::Deep,
            ThinkBudgetWire::Standard => ThinkBudget::Standard,
            ThinkBudgetWire::Fast => ThinkBudget::Fast,
            ThinkBudgetWire::Budget { tokens } => ThinkBudget::Budget { tokens },
        }
    }
}

// ---------- EngineRef ----------

#[derive(Serialize, Deserialize)]
pub struct EngineRefWire {
    pub provider: Sid,
    pub model: Option<Sid>,
}

impl EngineRefWire {
    pub fn pack(e: EngineRef, ix: &mut StringInterner) -> Self {
        Self {
            provider: ix.intern(e.provider),
            model: ix.intern_opt(e.model),
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>) -> EngineRef {
        EngineRef {
            provider: r.get(self.provider),
            model: r.get_opt(self.model),
        }
    }
}

// ---------- AnnotationRef ----------

#[derive(Serialize, Deserialize)]
pub struct AnnotationRefWire {
    pub anchor: Nid,
    pub source_file: Sid,
    pub source_line: u32,
    pub kind: AnnotationKindWire,
}

impl AnnotationRefWire {
    pub fn pack(
        a: AnnotationRef,
        ix: &mut StringInterner,
        nx: &mut NodeIdInterner,
    ) -> Self {
        Self {
            anchor: nx.intern(a.anchor),
            source_file: ix.intern(a.source_file),
            source_line: a.source_line,
            kind: AnnotationKindWire::pack(a.kind, ix, nx),
        }
    }

    pub fn unpack(self, r: &StringResolver<'_>, nr: &NodeIdResolver<'_>) -> AnnotationRef {
        AnnotationRef {
            anchor: nr.get(self.anchor),
            source_file: r.get(self.source_file),
            source_line: self.source_line,
            kind: self.kind.unpack(r, nr),
        }
    }
}

// ---------- Top-level snapshot containers ----------

#[derive(Serialize, Deserialize)]
pub struct StoreSnapshotWire {
    pub version: u32,
    pub nodes: Vec<NodeRefWire>,
    pub edges: Vec<EdgeOutWire>,
    pub docs: Vec<(Nid, String)>,
    pub properties: Vec<(Nid, BTreeMap<Sid, Sid>)>,
}

impl StoreSnapshotWire {
    pub fn pack(
        s: yah_kg_store::StoreSnapshot,
        ix: &mut StringInterner,
        nx: &mut NodeIdInterner,
    ) -> Self {
        Self {
            version: s.version,
            nodes: s
                .nodes
                .into_iter()
                .map(|n| NodeRefWire::pack(n, ix, nx))
                .collect(),
            edges: s
                .edges
                .into_iter()
                .map(|e| EdgeOutWire::pack(e, ix, nx))
                .collect(),
            docs: s
                .docs
                .into_iter()
                .map(|(id, body)| (nx.intern(id), body))
                .collect(),
            properties: s
                .properties
                .into_iter()
                .map(|(id, map)| {
                    let interned: BTreeMap<Sid, Sid> = map
                        .into_iter()
                        .map(|(k, v)| (ix.intern(k), ix.intern(v)))
                        .collect();
                    (nx.intern(id), interned)
                })
                .collect(),
        }
    }

    pub fn unpack(
        self,
        r: &StringResolver<'_>,
        nr: &NodeIdResolver<'_>,
    ) -> yah_kg_store::StoreSnapshot {
        yah_kg_store::StoreSnapshot {
            version: self.version,
            nodes: self.nodes.into_iter().map(|n| n.unpack(r, nr)).collect(),
            edges: self.edges.into_iter().map(|e| e.unpack(r, nr)).collect(),
            docs: self
                .docs
                .into_iter()
                .map(|(id, body)| (nr.get(id), body))
                .collect(),
            properties: self
                .properties
                .into_iter()
                .map(|(id, map)| {
                    let resolved: BTreeMap<String, String> = map
                        .into_iter()
                        .map(|(k, v)| (r.get(k), r.get(v)))
                        .collect();
                    (nr.get(id), resolved)
                })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AnnotationIndexSnapshotWire {
    pub entries: Vec<(Nid, Vec<AnnotationRefWire>)>,
}

impl AnnotationIndexSnapshotWire {
    pub fn pack(
        s: yah_kg_anno::AnnotationIndexSnapshot,
        ix: &mut StringInterner,
        nx: &mut NodeIdInterner,
    ) -> Self {
        Self {
            entries: s
                .entries
                .into_iter()
                .map(|(id, anns)| {
                    let packed: Vec<AnnotationRefWire> = anns
                        .into_iter()
                        .map(|a| AnnotationRefWire::pack(a, ix, nx))
                        .collect();
                    (nx.intern(id), packed)
                })
                .collect(),
        }
    }

    pub fn unpack(
        self,
        r: &StringResolver<'_>,
        nr: &NodeIdResolver<'_>,
    ) -> yah_kg_anno::AnnotationIndexSnapshot {
        yah_kg_anno::AnnotationIndexSnapshot {
            entries: self
                .entries
                .into_iter()
                .map(|(id, anns)| {
                    let unpacked: Vec<AnnotationRef> =
                        anns.into_iter().map(|a| a.unpack(r, nr)).collect();
                    (nr.get(id), unpacked)
                })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KgSnapshotWire {
    pub version: u32,
    pub rig_root: PathBuf,
    pub fingerprints: HashMap<String, FileFingerprint>,
    pub strings: Vec<String>,
    pub node_ids: Vec<NodeId>,
    pub store: StoreSnapshotWire,
    pub annotations: AnnotationIndexSnapshotWire,
}

impl KgSnapshotWire {
    pub fn pack(s: KgSnapshot) -> Self {
        let mut ix = StringInterner::default();
        let mut nx = NodeIdInterner::default();
        let store = StoreSnapshotWire::pack(s.store, &mut ix, &mut nx);
        let annotations = AnnotationIndexSnapshotWire::pack(s.annotations, &mut ix, &mut nx);
        Self {
            version: s.version,
            rig_root: s.rig_root,
            fingerprints: s.fingerprints,
            strings: ix.into_table(),
            node_ids: nx.into_table(),
            store,
            annotations,
        }
    }

    pub fn unpack(self) -> KgSnapshot {
        let resolver = StringResolver::new(&self.strings);
        let nresolver = NodeIdResolver::new(&self.node_ids);
        let store = self.store.unpack(&resolver, &nresolver);
        let annotations = self.annotations.unpack(&resolver, &nresolver);
        KgSnapshot {
            version: self.version,
            rig_root: self.rig_root,
            fingerprints: self.fingerprints,
            store,
            annotations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yah_kg_store::StoreSnapshot;

    #[test]
    fn fingerprints_round_trip_through_postcard() {
        let mut fps = HashMap::new();
        for i in 0..400 {
            fps.insert(
                format!("src/file_{:04}.rs", i),
                FileFingerprint {
                    mtime_secs: 1_700_000_000 + i as u64,
                    mtime_nanos: (i as u32) * 7919,
                    size: i as u64 * 13,
                },
            );
        }
        let snap = KgSnapshot {
            version: super::super::snapshot::SNAPSHOT_VERSION,
            rig_root: PathBuf::from("/tmp/rig"),
            fingerprints: fps.clone(),
            store: StoreSnapshot {
                version: 1,
                nodes: vec![],
                edges: vec![],
                docs: vec![],
                properties: vec![],
            },
            annotations: yah_kg_anno::AnnotationIndexSnapshot { entries: vec![] },
        };

        let wire = KgSnapshotWire::pack(snap);
        let bytes = postcard::to_stdvec(&wire).unwrap();
        let restored: KgSnapshotWire = postcard::from_bytes(&bytes).unwrap();
        let restored = restored.unpack();

        assert_eq!(restored.fingerprints, fps);
    }

    #[test]
    fn string_interner_dedupes() {
        let mut ix = StringInterner::default();
        let a1 = ix.intern("hello".into());
        let a2 = ix.intern("hello".into());
        let b = ix.intern("world".into());
        assert_eq!(a1, a2);
        assert_ne!(a1, b);
        let table = ix.into_table();
        assert_eq!(table, vec!["hello".to_string(), "world".to_string()]);
    }
}
