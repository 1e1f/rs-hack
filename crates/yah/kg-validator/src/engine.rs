//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! Validate-engine: walks every `AnnotationKind::Rule` in the index, parses
//! it into a [`ParsedRule`], and evaluates it against the live `Store`.
//! Returns one [`Violation`] per offending node (per rule).

use crate::parse::parse_rule;
use crate::rule::{ParsedRule, RuleKind};
use crate::selector::{node_has_namespaced_tag, Selector};
use std::collections::{HashSet, VecDeque};
use kg::anno::AnnotationKind;
use kg::edge::EdgeKind;
use kg::ids::NodeId;
use kg::kind::{CommonKind, NodeKind};
use rpc::Direction;
use kg::validate::{Scope, Severity, Violation};
use kg_anno::AnnotationIndex;
use kg_store::Store;

/// Run the validator. Each `AnnotationKind::Rule` in `anno` produces zero
/// or more [`Violation`]s — parse errors and unknown rule kinds also surface
/// as violations rather than aborting the run.
pub fn validate(store: &Store, anno: &AnnotationIndex, scope: Scope) -> Vec<Violation> {
    let mut out = Vec::new();
    let scope_filter = compute_scope_filter(store, &scope);
    for (anchor_id, anns) in anno.iter() {
        if let Some(filter) = &scope_filter {
            if !filter.contains(&anchor_id) {
                continue;
            }
        }
        for ann in anns {
            let AnnotationKind::Rule { rule_kind, args } = &ann.kind else {
                continue;
            };
            let context = RuleContext {
                anchor: anchor_id,
                anchor_file: ann.source_file.clone(),
                anchor_line: ann.source_line,
            };
            match parse_rule(rule_kind, args) {
                Ok(parsed) => evaluate(&parsed, &context, store, &mut out),
                Err(err) => out.push(Violation {
                    rule_kind: rule_kind.clone(),
                    anchor: context.anchor,
                    anchor_file: context.anchor_file.clone(),
                    anchor_line: context.anchor_line,
                    offending: None,
                    offending_file: None,
                    offending_line: None,
                    message: format!("rule arg parse error: {err}"),
                    severity: Severity::Error,
                }),
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
struct RuleContext {
    anchor: NodeId,
    anchor_file: String,
    anchor_line: u32,
}

impl RuleContext {
    fn violate(&self, rule_kind: &str, message: String) -> Violation {
        Violation {
            rule_kind: rule_kind.to_string(),
            anchor: self.anchor,
            anchor_file: self.anchor_file.clone(),
            anchor_line: self.anchor_line,
            offending: None,
            offending_file: None,
            offending_line: None,
            message,
            severity: Severity::Error,
        }
    }

    fn violate_at(
        &self,
        rule_kind: &str,
        node: NodeId,
        node_file: Option<String>,
        node_line: Option<u32>,
        message: String,
    ) -> Violation {
        Violation {
            rule_kind: rule_kind.to_string(),
            anchor: self.anchor,
            anchor_file: self.anchor_file.clone(),
            anchor_line: self.anchor_line,
            offending: Some(node),
            offending_file: node_file,
            offending_line: node_line,
            message,
            severity: Severity::Error,
        }
    }
}

fn evaluate(rule: &ParsedRule, ctx: &RuleContext, store: &Store, out: &mut Vec<Violation>) {
    match &rule.kind {
        RuleKind::NoImportOf { targets } => check_outgoing_to(
            &rule.raw_kind,
            ctx,
            store,
            &[EdgeKind::Imports],
            targets,
            out,
        ),
        RuleKind::NoDependencyOn { targets } => check_outgoing_to(
            &rule.raw_kind,
            ctx,
            store,
            DEPENDENCY_EDGES,
            targets,
            out,
        ),
        RuleKind::MaxDepth { depth } => check_max_depth(&rule.raw_kind, ctx, store, *depth, out),
        RuleKind::MustTag { namespace } => {
            check_must_tag(&rule.raw_kind, ctx, store, namespace, out)
        }
        RuleKind::Unknown { raw } if raw.trim().to_ascii_lowercase().starts_with("agent-") => {
            // Agent-policy rules (`agent-role`, `agent-do`, `agent-dont`,
            // future `agent-*`) live in the CLAUDE.md prelude generator
            // (R028-F10), not the graph validator. They reach this
            // function only when authored at workspace scope (outside a
            // relay/ticket block). Skip silently — emitting a "unknown
            // rule kind" violation would force authors to choose between
            // a working validator and workspace-level agent policy.
            let _ = (raw, out);
        }
        RuleKind::Unknown { raw } => {
            out.push(ctx.violate(
                raw,
                format!(
                    "unknown rule kind {:?} — vocabulary v1 is no-import-of, no-dependency-on, max-depth, must-tag",
                    raw
                ),
            ));
        }
    }
}

/// Edges checked by `no-dependency-on`. Anything that means "this node
/// references that one" — covers Rust `use`, function calls, type
/// references, trait impls, TS extends, re-exports, generic bounds, and
/// Rust impl-for. Not included: `Contains`/`Defines` (structural
/// containment, not a dependency the author chose) and the annotation
/// overlay edges (`Tag`, `Flow`, `Anchors`, `ParentItem`).
const DEPENDENCY_EDGES: &[EdgeKind] = &[
    EdgeKind::Imports,
    EdgeKind::ReExports,
    EdgeKind::Calls,
    EdgeKind::References,
    EdgeKind::Implements,
    EdgeKind::Extends,
    EdgeKind::Bounds,
    EdgeKind::ImplFor,
    EdgeKind::DerivedBy,
    EdgeKind::AttributedBy,
    EdgeKind::ImplOfTrait,
    EdgeKind::RefersTo,
    EdgeKind::ConformsTo,
];

fn check_outgoing_to(
    rule_kind: &str,
    ctx: &RuleContext,
    store: &Store,
    edge_kinds: &[EdgeKind],
    targets: &[Selector],
    out: &mut Vec<Violation>,
) {
    let mut forbidden: HashSet<NodeId> = HashSet::new();
    for sel in targets {
        forbidden.extend(sel.resolve(store));
    }
    if forbidden.is_empty() {
        // Selector matched nothing — emit one informational violation per
        // unmatched selector so authors aren't surprised by silent passes.
        for sel in targets {
            if sel.resolve(store).is_empty() {
                out.push(ctx.violate(
                    rule_kind,
                    format!(
                        "selector {} matched no nodes — typo, or the target hasn't been tagged yet",
                        sel.describe()
                    ),
                ));
            }
        }
        return;
    }

    let scope = subtree(store, ctx.anchor);
    for source_id in &scope {
        for edge in store.neighbors(*source_id, Direction::Out, Some(edge_kinds)) {
            if !forbidden.contains(&edge.to) {
                continue;
            }
            let target_label = store
                .node_ref(edge.to)
                .map(|n| n.qualified.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            let (file, line) = store
                .node_ref(*source_id)
                .map(|n| (Some(n.file.clone()), Some(n.span.start_line)))
                .unwrap_or((None, None));
            out.push(ctx.violate_at(
                rule_kind,
                *source_id,
                file,
                line,
                format!(
                    "{:?} edge to {} violates {}",
                    edge.kind, target_label, rule_kind
                ),
            ));
        }
    }
}

fn check_max_depth(
    rule_kind: &str,
    ctx: &RuleContext,
    store: &Store,
    depth: u32,
    out: &mut Vec<Violation>,
) {
    // BFS via Contains-out from anchor. Anchor is depth 0; emit a violation
    // every time a node sits deeper than `depth` hops.
    let mut visited: HashSet<NodeId> = HashSet::new();
    visited.insert(ctx.anchor);
    let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();
    queue.push_back((ctx.anchor, 0));
    while let Some((id, d)) = queue.pop_front() {
        if d > depth {
            let (file, line) = store
                .node_ref(id)
                .map(|n| (Some(n.file.clone()), Some(n.span.start_line)))
                .unwrap_or((None, None));
            let label = store
                .node_ref(id)
                .map(|n| n.qualified.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            out.push(ctx.violate_at(
                rule_kind,
                id,
                file,
                line,
                format!("{} sits at depth {} (max-depth {})", label, d, depth),
            ));
            // Don't descend further from a violating node — one report per
            // subtree is enough; the caller can pair this with the parent
            // violation to find the root cause.
            continue;
        }
        for edge in store.neighbors(id, Direction::Out, Some(&[EdgeKind::Contains])) {
            if visited.insert(edge.to) {
                queue.push_back((edge.to, d + 1));
            }
        }
    }
}

fn check_must_tag(
    rule_kind: &str,
    ctx: &RuleContext,
    store: &Store,
    namespace: &str,
    out: &mut Vec<Violation>,
) {
    let scope = subtree(store, ctx.anchor);
    // Cache: which scope nodes (or ancestors up to the anchor) carry a tag
    // in the namespace. Walk descendants and check direct-or-inherited.
    for node_id in &scope {
        let Some(node) = store.node_ref(*node_id) else {
            continue;
        };
        // Skip synthetic overlay nodes (Tag / Relay / Ticket) — they don't
        // represent code and weren't part of what the author wrote.
        if matches!(
            node.kind,
            NodeKind::Common(CommonKind::Tag)
                | NodeKind::Common(CommonKind::Relay)
                | NodeKind::Common(CommonKind::Ticket)
        ) {
            continue;
        }
        if has_inherited_tag(store, ctx.anchor, *node_id, namespace) {
            continue;
        }
        out.push(ctx.violate_at(
            rule_kind,
            *node_id,
            Some(node.file.clone()),
            Some(node.span.start_line),
            format!(
                "{} carries no tag in namespace {:?} (must-tag)",
                node.qualified, namespace
            ),
        ));
    }
}

/// Walk Contains-In ancestors from `node` up to `anchor` (inclusive) — if
/// any of them carry a tag in `namespace`, the rule is satisfied for `node`.
/// Stops at the anchor or when the chain runs out.
fn has_inherited_tag(store: &Store, anchor: NodeId, node: NodeId, namespace: &str) -> bool {
    let mut current = node;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current) {
            return false;
        }
        if node_has_namespaced_tag(store, current, namespace) {
            return true;
        }
        if current == anchor {
            return false;
        }
        // Climb to parent via incoming Contains.
        let parents = store.neighbors(current, Direction::In, Some(&[EdgeKind::Contains]));
        let Some(parent_edge) = parents.into_iter().next() else {
            return false;
        };
        current = parent_edge.from;
    }
}

/// Compute the subtree rooted at `anchor` via outgoing `Contains` edges.
/// Includes the anchor itself.
fn subtree(store: &Store, anchor: NodeId) -> HashSet<NodeId> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    seen.insert(anchor);
    let mut queue: VecDeque<NodeId> = VecDeque::new();
    queue.push_back(anchor);
    while let Some(id) = queue.pop_front() {
        for edge in store.neighbors(id, Direction::Out, Some(&[EdgeKind::Contains])) {
            if seen.insert(edge.to) {
                queue.push_back(edge.to);
            }
        }
    }
    seen
}

/// Build the validate-scope filter set (the set of *anchors* the run will
/// consider). `Scope::All` skips the filter entirely.
fn compute_scope_filter(store: &Store, scope: &Scope) -> Option<HashSet<NodeId>> {
    match scope {
        Scope::All => None,
        Scope::Subtree { root } => Some(subtree(store, *root)),
        Scope::File { path } => {
            let mut set = HashSet::new();
            for n in store.all_node_refs() {
                if n.file == *path {
                    set.insert(n.id);
                }
            }
            Some(set)
        }
    }
}
