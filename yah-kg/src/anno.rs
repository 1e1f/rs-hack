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
