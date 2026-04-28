//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Annotation overlay types.
//!
//! Annotations are human-authored decorations on structural KG nodes. They
//! live in source as `@yah:` directives inside doc comments. Three kinds:
//!
//! * **Tag** — set membership. `@yah:tag(audio, hot-path)` adds the
//!   annotated node to the named taxonomies. Stored as `EdgeKind::Tag`
//!   edges from the structural node to a synthetic `Tag` node so the
//!   "show me everything in `audio`" query is a 1-hop subgraph fetch.
//! * **Flow** — curated edge. `@yah:flow(audio::mixer → dispatch::loop,
//!   "shared frame buffer")` declares a meaningful coupling that
//!   `Calls`/`Imports` can't see. Endpoints resolve via qualified-name
//!   lookup; rotted endpoints surface as warnings, not silent drops.
//! * **Rule** — graph constraint. `@yah:rule(no-import-of: tag(view))`
//!   declares an invariant the validator checks. Rule semantics are
//!   reserved in the contract; the v1 extractor parses them but doesn't
//!   yet ship a validator.
//!
//! @yah:ticket(R017-F1, "yah-kg-validator crate + vocabulary (no-import-of, no-dependency-on, max-depth, must-tag)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Walks KG, applies rules from index, returns Vec<Violation>")
//! @yah:next("Cargo.toml [workspace.metadata.arch] section declares legal rule kinds + tag namespaces")
//! @yah:next("Parser already handles @yah:rule(no-import-of: tag(view)); the type exists in the contract — wire validation")

use crate::ids::NodeId;
use serde::{Deserialize, Serialize};

/// One annotation as observed on a node, with provenance back to its
/// source-line so authors can be pointed at the offending comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationRef {
    /// What was annotated.
    pub anchor: NodeId,
    /// Where the annotation lived in source. Both fields are convenience
    /// for tooling — the structural file is also in `anchor`'s NodeRef.
    pub source_file: String,
    pub source_line: u32,
    /// The annotation payload.
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "anno", rename_all = "snake_case")]
pub enum AnnotationKind {
    /// `@yah:tag(name)` or `@yah:tag(ns:name)`.
    Tag(TagRef),
    /// `@yah:flow(<from> → <to>, "<reason>")`. `to_qualified` is the raw
    /// qualified-name string from source; the daemon resolves it against
    /// the structural index when emitting the `Flow` edge.
    Flow {
        to_qualified: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// `@yah:rule(<rule-kind>: <args>)`. Reserved — v1 extractors parse
    /// these into the structure but no validator runs against them yet.
    Rule {
        rule_kind: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    /// `@yah:relay(ID, "title")` plus the modifier directives that
    /// followed it in the same doc block (status, assignee, parent,
    /// handoff, next, gotcha, assumes, verify, cleanup, …). A relay is a
    /// thread of work; pass 2 of R017-F4 will promote these to synthetic
    /// `CommonKind::Relay` nodes with parent / depends_on edges.
    Relay(WorkItemAnno),
    /// `@yah:ticket(ID, "title")` plus its modifier directives. Tickets
    /// are leaf work units parented to a relay.
    Ticket(WorkItemAnno),
}

/// Payload shared by `Relay` and `Ticket` annotations. Field set mirrors
/// `yah::arch::ticket::Ticket` (the CLI extractor's mature model); pass
/// 2 of R017-F4 will unify the two parsers so this stays the only home.
///
/// Modifier directives (`@yah:status(...)`, `@yah:next("...")`, …) attach
/// to the most recent `@yah:relay(...)` or `@yah:ticket(...)` header in
/// the same doc string. A blank line *or* the next header closes a block.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemAnno {
    pub id: String,
    pub title: String,
    /// `@yah:kind(feature|bug|task|epic)` — overrides the natural kind
    /// (relays default to "relay"; tickets default to "task" or whatever
    /// the ID prefix implies in the CLI extractor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TicketStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// `@yah:parent(R001)` — for sub-tickets and zone (relay-of-relays)
    /// hierarchy. Resolved to a graph edge in pass 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// `@yah:handoff("...")` — repeatable. Stored in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handoff: Vec<String>,
    /// `@yah:next("...")` — repeatable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_steps: Vec<String>,
    /// `@yah:gotcha("...")` — repeatable. Pre-existing breakage / traps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gotchas: Vec<String>,
    /// `@yah:assumes("...")` — repeatable. Unverified claims.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumes: Vec<String>,
    /// `@yah:verify("...")` — repeatable. Acceptance commands.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verify: Vec<String>,
    /// `@yah:cleanup("...")` — repeatable. Deferred tech debt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleanup: Vec<String>,
    /// `@arch:see(path)` — repeatable. Architecture-doc references the
    /// pickup prompt surfaces under the "Reference" section. Crosses the
    /// `@yah:` / `@arch:` namespace boundary because the parser collects
    /// these inside the same relay/ticket block — the doc reference is
    /// a property of the work-item, not of the structural anchor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
}

/// Discriminator between the two work-item header kinds. `@yah:relay(...)`
/// produces `Relay`, `@yah:ticket(...)` produces `Ticket`. Used both by
/// the parser (to drive header dispatch) and by the RPC layer (so wire
/// payloads can name the kind without leaking the internal `CommonKind`
/// enum's variants for non-work-item nodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemType {
    Relay,
    Ticket,
}

/// Lifecycle column. Mirrors the kanban transitions enforced by the
/// board server (open → claimed/in-progress → handoff → review → done).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketStatus {
    Open,
    Claimed,
    InProgress,
    Handoff,
    Review,
    Done,
}

impl TicketStatus {
    /// Parse the canonical kebab-case form. Authors sometimes write
    /// `in_progress` or capitalize; accept those too.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "claimed" => Some(Self::Claimed),
            "in-progress" | "in_progress" | "inprogress" => Some(Self::InProgress),
            "handoff" => Some(Self::Handoff),
            "review" => Some(Self::Review),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::InProgress => "in-progress",
            Self::Handoff => "handoff",
            Self::Review => "review",
            Self::Done => "done",
        }
    }
}

/// A tag with optional namespace. `layer:core` parses to `TagRef { ns:
/// Some("layer"), name: "core" }`. `audio` parses to `TagRef { ns: None,
/// name: "audio" }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub name: String,
}

impl TagRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            name: name.into(),
        }
    }

    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Stable canonical string. Always prefixed `tag:` so synthetic Tag
    /// node ids never collide with structural-node qualified names.
    pub fn qualified(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("tag:{}:{}", ns, self.name),
            None => format!("tag:{}", self.name),
        }
    }

    /// Display label — the leaf name, or `ns:name` for namespaced tags.
    pub fn label(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}:{}", ns, self.name),
            None => self.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_ref_qualified_distinguishes_namespace() {
        assert_eq!(TagRef::new("audio").qualified(), "tag:audio");
        assert_eq!(
            TagRef::namespaced("layer", "core").qualified(),
            "tag:layer:core"
        );
    }

    #[test]
    fn tag_ref_label_is_human_readable() {
        assert_eq!(TagRef::new("audio").label(), "audio");
        assert_eq!(TagRef::namespaced("layer", "core").label(), "layer:core");
    }

    #[test]
    fn annotation_kind_serializes_with_tag_field() {
        let a = AnnotationKind::Tag(TagRef::new("audio"));
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"anno\":\"tag\""), "got {json}");
    }
}
