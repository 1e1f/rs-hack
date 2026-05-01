//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! Selector — a description of "which set of structural nodes" a rule
//! argument refers to. Resolved against a `Store` at validate-time.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use kg::anno::TagRef;
use kg::edge::EdgeKind;
use kg::ids::NodeId;
use kg::kind::{CommonKind, Lang, NodeKind};
use rpc::Direction;
use kg_store::Store;

/// One node-set descriptor as written inside a rule argument.
///
/// The grammar is intentionally narrow — selectors live in source comments
/// and authors shouldn't have to learn a query language to write a rule.
/// All forms resolve to a `HashSet<NodeId>` against the current `Store`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "selector", rename_all = "snake_case")]
pub enum Selector {
    /// `tag(name)` or `tag(ns:name)` — the set of nodes carrying that
    /// `EdgeKind::Tag` to the synthetic Tag node.
    Tag(TagRef),
    /// `ns(name)` — every node carrying any tag whose namespace is `name`.
    /// Useful for `must-tag` arguments and for "any layer at all" deny lists.
    Namespace(String),
    /// `node(qualified::name)` — exact qualified-name match. The match is
    /// case-sensitive and does not interpret `::` as a wildcard.
    QualifiedName(String),
    /// `kind(common::module)` — every node of the given kind. The
    /// stringified form mirrors the lowercased Debug shape used by
    /// `Store::stats` (`common::module`, `rust::trait_`, etc.).
    Kind(String),
}

impl Selector {
    /// Resolve to a node-id set against the current store. Synthetic Tag /
    /// Relay / Ticket nodes are *included* when they happen to match —
    /// rule authors rarely care about the distinction, and excluding them
    /// would make `kind(common::tag)` selectors silently empty.
    pub fn resolve(&self, store: &Store) -> HashSet<NodeId> {
        match self {
            Selector::Tag(tag) => resolve_tag(store, tag),
            Selector::Namespace(ns) => resolve_namespace(store, ns),
            Selector::QualifiedName(q) => resolve_qualified(store, q),
            Selector::Kind(k) => resolve_kind(store, k),
        }
    }

    /// Human-readable form for violation messages — round-trips back to the
    /// canonical written shape so authors see what they typed.
    pub fn describe(&self) -> String {
        match self {
            Selector::Tag(t) => format!("tag({})", t.label()),
            Selector::Namespace(n) => format!("ns({})", n),
            Selector::QualifiedName(q) => format!("node({})", q),
            Selector::Kind(k) => format!("kind({})", k),
        }
    }
}

fn resolve_tag(store: &Store, tag: &TagRef) -> HashSet<NodeId> {
    // Synthetic Tag node id is derived from the tag's qualified name.
    // Look it up by qualified — the apply pass uses the same rule.
    let target = store
        .all_node_refs()
        .find(|n| {
            matches!(n.kind, NodeKind::Common(CommonKind::Tag)) && n.qualified == tag.qualified()
        })
        .map(|n| n.id);
    let Some(tag_node_id) = target else {
        return HashSet::new();
    };
    store
        .neighbors(tag_node_id, Direction::In, Some(&[EdgeKind::Tag]))
        .into_iter()
        .map(|edge| edge.from)
        .collect()
}

fn resolve_namespace(store: &Store, ns: &str) -> HashSet<NodeId> {
    // Every Tag node whose qualified begins with `tag:<ns>:` — then walk
    // incoming Tag edges from each.
    let prefix = format!("tag:{ns}:");
    let mut out = HashSet::new();
    for tag_node in store
        .all_node_refs()
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::Tag)))
        .filter(|n| n.qualified.starts_with(&prefix))
    {
        for edge in store.neighbors(tag_node.id, Direction::In, Some(&[EdgeKind::Tag])) {
            out.insert(edge.from);
        }
    }
    out
}

fn resolve_qualified(store: &Store, q: &str) -> HashSet<NodeId> {
    store
        .all_node_refs()
        .filter(|n| n.qualified == q)
        .map(|n| n.id)
        .collect()
}

fn resolve_kind(store: &Store, k: &str) -> HashSet<NodeId> {
    let needle = k.trim().to_ascii_lowercase();
    store
        .all_node_refs()
        .filter(|n| short_kind_label(&n.kind) == needle)
        .map(|n| n.id)
        .collect()
}

/// Mirrors `kg_store::Store::stats`'s short-kind format. Duplicated
/// because that helper is private; both versions must stay in sync.
fn short_kind_label(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Common(c) => format!("common::{:?}", c).to_lowercase(),
        NodeKind::Rust(r) => format!("rust::{:?}", r).to_lowercase(),
        NodeKind::Ts(t) => format!("ts::{:?}", t).to_lowercase(),
        NodeKind::Doc(d) => format!("doc::{:?}", d).to_lowercase(),
        NodeKind::Koda(k) => format!("koda::{:?}", k).to_lowercase(),
    }
}

/// True when the given node carries any tag with namespace `ns` (direct
/// outgoing `EdgeKind::Tag` to a Tag node whose qualified begins with
/// `tag:<ns>:`). Used by `must-tag` to avoid materializing the full
/// namespace set when checking one node at a time.
pub(crate) fn node_has_namespaced_tag(store: &Store, node: NodeId, ns: &str) -> bool {
    let prefix = format!("tag:{ns}:");
    for edge in store.neighbors(node, Direction::Out, Some(&[EdgeKind::Tag])) {
        let Some(target) = store.node_ref(edge.to) else {
            continue;
        };
        if target.qualified.starts_with(&prefix) {
            return true;
        }
    }
    false
}

/// Sentinel — Tag node ids are computed with `Lang::Rust` regardless of
/// the structural source language; matches the apply-pass convention.
#[allow(dead_code)]
const TAG_LANG: Lang = Lang::Rust;
