//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! `yah-kg-validator` — runs `@yah:rule(...)` annotations against the live
//! knowledge graph. The parser in `yah-kg-anno` already converts a doc-string
//! `@yah:rule(no-import-of: tag(view))` directive into an
//! [`AnnotationKind::Rule`] entry on the node it was authored on; this crate
//! evaluates those rules and returns a [`Vec<Violation>`].
//!
//! ## Vocabulary v1
//!
//! Every rule names a `rule_kind` and one or more selector args. A *selector*
//! describes a set of structural nodes:
//!
//! * `tag(name)` / `tag(ns:name)` — every node with that tag (resolved by
//!   walking incoming `EdgeKind::Tag` to the synthetic Tag node).
//! * `node(qualified::name)` — exact qualified-name match.
//! * `kind(common::module)` — every node of the named kind. Lowercased
//!   `common::*` / `rust::*` / `ts::*` / `doc::*` / `koda::*`.
//!
//! Supported `rule_kind`s:
//!
//! | Kind | Form | Meaning |
//! |------|------|---------|
//! | `no-import-of` | `tag(view)` | The anchor's subtree must not have outgoing `Imports` edges to any node in the selector set. |
//! | `no-dependency-on` | `tag(view)` | Same scope as above, but checks every structural-reference edge (`Imports`, `Calls`, `References`, `Implements`, `Extends`, `ReExports`, `Bounds`, `ImplFor`). |
//! | `max-depth` | `5` (integer) | The anchor's subtree must not nest deeper than N hops via `Contains`. |
//! | `must-tag` | `layer` (namespace) or `ns(layer)` | Every non-synthetic node in the anchor's subtree must carry — or inherit via a `Contains` ancestor — at least one tag in that namespace. |
//!
//! ## Scope
//!
//! [`Scope`] determines which annotated anchors the validator considers in
//! one run. `Scope::All` walks every anchor in the index; `Scope::Subtree`
//! restricts to anchors reachable from a given root via outgoing `Contains`
//! edges; `Scope::File` restricts to anchors authored in one rig-relative
//! file. The *rule's* internal scope (the subtree the rule itself covers) is
//! always derived from the anchor and is independent of the validate-scope.
//!
//! ## Cargo.toml metadata
//!
//! The workspace declares the legal rule_kinds and tag namespaces under
//! `[workspace.metadata.arch.rule_vocabulary]`. v1 ships those as documentation
//! only — the validator emits a violation for an unknown rule_kind regardless,
//! and unknown tag namespaces are accepted (a `must-tag` against an unknown
//! namespace will simply find no matches and flag every node).

pub mod parse;
pub mod rule;
pub mod selector;
pub mod violation;

mod engine;

pub use engine::validate;
pub use parse::ParseError;
pub use rule::{ParsedRule, RuleKind};
pub use selector::Selector;
pub use violation::{Severity, Violation};
pub use yah_kg::validate::Scope;
