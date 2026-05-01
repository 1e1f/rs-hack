//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! Typed rule shape. The parser converts an `AnnotationKind::Rule { rule_kind,
//! args }` into a [`ParsedRule`]; the engine matches on [`RuleKind`] to pick
//! the evaluator.

use crate::selector::Selector;
use serde::{Deserialize, Serialize};

/// Discriminator over the v1 vocabulary. `Unknown` carries the raw rule
/// kind string so the engine can emit a "vocabulary error" violation
/// without losing the author's intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    /// `@yah:rule(no-import-of: tag(view))`.
    NoImportOf { targets: Vec<Selector> },
    /// `@yah:rule(no-dependency-on: tag(other-layer))`.
    NoDependencyOn { targets: Vec<Selector> },
    /// `@yah:rule(max-depth: 5)`.
    MaxDepth { depth: u32 },
    /// `@yah:rule(must-tag: layer)` or `@yah:rule(must-tag: ns(layer))`.
    /// `namespace` is the tag namespace that every node in scope must
    /// carry (directly or via a `Contains` ancestor).
    MustTag { namespace: String },
    /// Authored kind that didn't parse to anything in the v1 vocabulary.
    /// The engine emits a "unknown rule kind" violation rather than
    /// silently dropping, so typos surface.
    Unknown { raw: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedRule {
    pub kind: RuleKind,
    /// The raw rule_kind string as authored. Preserved through `Unknown`
    /// for visibility, and through known kinds for diagnostic messages.
    pub raw_kind: String,
}
