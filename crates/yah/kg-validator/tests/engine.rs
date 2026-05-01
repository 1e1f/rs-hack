//! Integration tests for the rule engine.
//!
//! Each test hand-builds a small `Store` + `AnnotationIndex` and asserts
//! the violations the validator emits. We bypass the doc-string parser in
//! `yah-kg-anno` to keep failures attributable to the engine rather than
//! to upstream parsing.

use std::collections::HashMap;
use kg::anno::{AnnotationKind, AnnotationRef, TagRef};
use kg::edge::{EdgeId, EdgeKind, EdgeOut};
use kg::ids::{NodeId, NodeRef, Span};
use kg::kind::{CommonKind, Lang, NodeKind};
use kg_anno::AnnotationIndex;
use kg_store::Store;
use kg_validator::{validate, Scope, Severity};

/// Convenience builder for laying out a tiny graph by qualified name.
struct GraphBuilder {
    store: Store,
    ids: HashMap<String, NodeId>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            store: Store::new(),
            ids: HashMap::new(),
        }
    }

    fn node(&mut self, qualified: &str, kind: NodeKind) -> NodeId {
        self.node_in(qualified, kind, &format!("{qualified}.rs"))
    }

    fn node_in(&mut self, qualified: &str, kind: NodeKind, file: &str) -> NodeId {
        let id = NodeId::compute(Lang::Rust, qualified, file);
        let n = NodeRef {
            id,
            lang: Lang::Rust,
            kind,
            label: qualified.split("::").last().unwrap_or(qualified).to_string(),
            qualified: qualified.to_string(),
            file: file.to_string(),
            span: Span {
                start_line: 1,
                start_col: 1,
                end_line: 10,
                end_col: 1,
            },
            synthetic: false,
        };
        self.store.upsert_node(n);
        self.ids.insert(qualified.to_string(), id);
        id
    }

    fn synth_tag(&mut self, tag: TagRef) -> NodeId {
        let qualified = tag.qualified();
        let id = NodeId::compute(Lang::Rust, &qualified, "<tag>");
        let n = NodeRef {
            id,
            lang: Lang::Rust,
            kind: NodeKind::Common(CommonKind::Tag),
            label: tag.label(),
            qualified,
            file: "<tag>".to_string(),
            span: Span::point(0, 0),
            synthetic: true,
        };
        self.store.upsert_node(n);
        id
    }

    fn edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        let id = EdgeId::compute(from, to, &kind);
        self.store.upsert_edge(EdgeOut {
            id,
            from,
            to,
            kind,
            annotations: vec![],
        });
    }
}

fn rule_anno(file: &str, line: u32, anchor: NodeId, rule_kind: &str, args: &[&str]) -> AnnotationRef {
    AnnotationRef {
        anchor,
        source_file: file.to_string(),
        source_line: line,
        kind: AnnotationKind::Rule {
            rule_kind: rule_kind.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        },
    }
}

#[test]
fn no_import_of_flags_disallowed_imports() {
    let mut g = GraphBuilder::new();
    let module = g.node("audio::mod", NodeKind::Common(CommonKind::Module));
    let mixer = g.node_in(
        "audio::mixer",
        NodeKind::Common(CommonKind::Function),
        "audio/mod.rs",
    );
    let view_widget = g.node_in(
        "view::widget",
        NodeKind::Common(CommonKind::Function),
        "view/mod.rs",
    );
    g.edge(module, mixer, EdgeKind::Contains);
    g.edge(mixer, view_widget, EdgeKind::Imports);

    // Tag the view_widget with `view`.
    let tag = g.synth_tag(TagRef::new("view"));
    g.edge(view_widget, tag, EdgeKind::Tag);

    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno(
            "audio/mod.rs",
            5,
            module,
            "no-import-of",
            &["tag(view)"],
        )],
    );

    let violations = validate(&g.store, &anno, Scope::All);
    assert_eq!(violations.len(), 1, "{:?}", violations);
    let v = &violations[0];
    assert_eq!(v.rule_kind, "no-import-of");
    assert_eq!(v.anchor, module);
    assert_eq!(v.offending, Some(mixer));
    assert_eq!(v.severity, Severity::Error);
}

#[test]
fn no_import_of_passes_when_no_offending_edge_exists() {
    let mut g = GraphBuilder::new();
    let module = g.node("audio::mod", NodeKind::Common(CommonKind::Module));
    let mixer = g.node("audio::mixer", NodeKind::Common(CommonKind::Function));
    g.edge(module, mixer, EdgeKind::Contains);

    let view_widget = g.node("view::widget", NodeKind::Common(CommonKind::Function));
    let tag = g.synth_tag(TagRef::new("view"));
    g.edge(view_widget, tag, EdgeKind::Tag);

    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno("audio/mod.rs", 5, module, "no-import-of", &["tag(view)"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert!(violations.is_empty(), "{:?}", violations);
}

#[test]
fn no_dependency_on_catches_calls_not_just_imports() {
    let mut g = GraphBuilder::new();
    let module = g.node("audio::mod", NodeKind::Common(CommonKind::Module));
    let mixer = g.node("audio::mixer", NodeKind::Common(CommonKind::Function));
    let view_widget = g.node("view::widget", NodeKind::Common(CommonKind::Function));
    g.edge(module, mixer, EdgeKind::Contains);
    g.edge(mixer, view_widget, EdgeKind::Calls);
    let tag = g.synth_tag(TagRef::new("view"));
    g.edge(view_widget, tag, EdgeKind::Tag);

    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno(
            "audio/mod.rs",
            5,
            module,
            "no-dependency-on",
            &["tag(view)"],
        )],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule_kind, "no-dependency-on");
    assert_eq!(violations[0].offending, Some(mixer));
}

#[test]
fn no_import_of_emits_warning_when_selector_matches_nothing() {
    let mut g = GraphBuilder::new();
    let module = g.node("audio::mod", NodeKind::Common(CommonKind::Module));

    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno("audio/mod.rs", 5, module, "no-import-of", &["tag(nope)"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("matched no nodes"));
}

#[test]
fn max_depth_flags_deeper_than_n() {
    let mut g = GraphBuilder::new();
    // Chain: root → a → b → c → d.  max-depth(2) flags c (depth 3).
    let root = g.node("root", NodeKind::Common(CommonKind::Module));
    let a = g.node("a", NodeKind::Common(CommonKind::Module));
    let b = g.node("b", NodeKind::Common(CommonKind::Module));
    let c = g.node("c", NodeKind::Common(CommonKind::Module));
    let d = g.node("d", NodeKind::Common(CommonKind::Module));
    g.edge(root, a, EdgeKind::Contains);
    g.edge(a, b, EdgeKind::Contains);
    g.edge(b, c, EdgeKind::Contains);
    g.edge(c, d, EdgeKind::Contains);

    let mut anno = AnnotationIndex::new();
    anno.set(
        root,
        vec![rule_anno("root.rs", 1, root, "max-depth", &["2"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    // c is the first node past depth 2; d sits below c but we don't descend
    // further once we report. So exactly one violation expected.
    assert_eq!(violations.len(), 1, "{:?}", violations);
    assert_eq!(violations[0].offending, Some(c));
    assert!(violations[0].message.contains("max-depth 2"));
}

#[test]
fn max_depth_passes_when_tree_is_shallow() {
    let mut g = GraphBuilder::new();
    let root = g.node("root", NodeKind::Common(CommonKind::Module));
    let a = g.node("a", NodeKind::Common(CommonKind::Module));
    g.edge(root, a, EdgeKind::Contains);

    let mut anno = AnnotationIndex::new();
    anno.set(
        root,
        vec![rule_anno("root.rs", 1, root, "max-depth", &["3"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert!(violations.is_empty(), "{:?}", violations);
}

#[test]
fn must_tag_inherits_from_ancestor() {
    let mut g = GraphBuilder::new();
    // anchor → mid → leaf.  anchor carries tag layer:audio; leaf has nothing
    // of its own. `must-tag(layer)` should pass because mid + leaf inherit
    // through Contains-In ancestry.
    let anchor = g.node("crate", NodeKind::Common(CommonKind::Module));
    let mid = g.node("crate::mid", NodeKind::Common(CommonKind::Module));
    let leaf = g.node("crate::mid::Leaf", NodeKind::Common(CommonKind::Function));
    g.edge(anchor, mid, EdgeKind::Contains);
    g.edge(mid, leaf, EdgeKind::Contains);
    let tag = g.synth_tag(TagRef::namespaced("layer", "audio"));
    g.edge(anchor, tag, EdgeKind::Tag);

    let mut anno = AnnotationIndex::new();
    anno.set(
        anchor,
        vec![rule_anno("crate.rs", 1, anchor, "must-tag", &["layer"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert!(violations.is_empty(), "{:?}", violations);
}

#[test]
fn must_tag_flags_uncovered_subtree() {
    let mut g = GraphBuilder::new();
    // anchor has a layer tag; sibling subtree under anchor lacks one.
    let anchor = g.node("crate", NodeKind::Common(CommonKind::Module));
    let tagged_branch = g.node("crate::tagged", NodeKind::Common(CommonKind::Module));
    let untagged_branch = g.node("crate::untagged", NodeKind::Common(CommonKind::Module));
    let untagged_leaf = g.node(
        "crate::untagged::Leaf",
        NodeKind::Common(CommonKind::Function),
    );
    g.edge(anchor, tagged_branch, EdgeKind::Contains);
    g.edge(anchor, untagged_branch, EdgeKind::Contains);
    g.edge(untagged_branch, untagged_leaf, EdgeKind::Contains);

    let layer_tag = g.synth_tag(TagRef::namespaced("layer", "core"));
    g.edge(tagged_branch, layer_tag, EdgeKind::Tag);

    let mut anno = AnnotationIndex::new();
    anno.set(
        anchor,
        vec![rule_anno("crate.rs", 1, anchor, "must-tag", &["layer"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    // anchor itself + untagged_branch + untagged_leaf each fail.
    let offending: Vec<_> = violations.iter().map(|v| v.offending).collect();
    assert!(offending.contains(&Some(anchor)));
    assert!(offending.contains(&Some(untagged_branch)));
    assert!(offending.contains(&Some(untagged_leaf)));
    assert!(!offending.contains(&Some(tagged_branch)));
}

#[test]
fn unknown_rule_kind_emits_violation_naming_the_typo() {
    let mut g = GraphBuilder::new();
    let module = g.node("foo", NodeKind::Common(CommonKind::Module));
    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno(
            "foo.rs",
            1,
            module,
            "no-cycle",
            &["tag(audio)"],
        )],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].rule_kind, "no-cycle");
    assert!(violations[0].message.contains("unknown rule kind"));
}

#[test]
fn agent_policy_rule_kinds_are_silently_skipped() {
    // Free-floating `@yah:rule(agent-*: ...)` is an authoring shape for
    // workspace-level CLAUDE.md prelude policy (R028-F10), not a graph
    // constraint. It must not surface as an "unknown rule kind"
    // violation, otherwise authors can't keep both a clean validator
    // run and workspace-level policy.
    let mut g = GraphBuilder::new();
    let module = g.node("foo", NodeKind::Common(CommonKind::Module));
    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![
            rule_anno("foo.rs", 1, module, "agent-do", &["\"Run cargo test.\""]),
            rule_anno(
                "foo.rs",
                2,
                module,
                "agent-role",
                &["\"Reviewer\"", "\"You verify, you don't write.\""],
            ),
            rule_anno(
                "foo.rs",
                3,
                module,
                "agent-future-shape",
                &["\"forward-compat\""],
            ),
        ],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert!(
        violations.is_empty(),
        "expected no violations from agent-* rules, got {:?}",
        violations
    );
}

#[test]
fn parse_error_in_args_surfaces_as_violation() {
    let mut g = GraphBuilder::new();
    let module = g.node("foo", NodeKind::Common(CommonKind::Module));
    let mut anno = AnnotationIndex::new();
    anno.set(
        module,
        vec![rule_anno("foo.rs", 1, module, "max-depth", &["five"])],
    );
    let violations = validate(&g.store, &anno, Scope::All);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].message.contains("rule arg parse error"));
}

#[test]
fn scope_file_filters_anchors() {
    let mut g = GraphBuilder::new();
    let in_scope = g.node_in("foo", NodeKind::Common(CommonKind::Module), "wanted.rs");
    let out_of_scope = g.node_in("bar", NodeKind::Common(CommonKind::Module), "other.rs");
    let mut anno = AnnotationIndex::new();
    // Both nodes carry an unknown rule — only the in-scope one should
    // surface.
    anno.set(
        in_scope,
        vec![rule_anno("wanted.rs", 1, in_scope, "no-cycle", &["tag(x)"])],
    );
    anno.set(
        out_of_scope,
        vec![rule_anno("other.rs", 1, out_of_scope, "no-cycle", &["tag(x)"])],
    );
    let violations = validate(
        &g.store,
        &anno,
        Scope::File {
            path: "wanted.rs".into(),
        },
    );
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].anchor_file, "wanted.rs");
}
