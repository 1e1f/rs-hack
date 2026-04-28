//! @arch:layer(kg)
//! @arch:role(validate)
//!
//! Selector + rule-arg parsing.
//!
//! The annotation parser in `yah-kg-anno` already split a `@yah:rule(...)`
//! body into `(rule_kind, args)` — args are top-level-comma-split strings
//! such as `"tag(view)"` or `"5"`. This module turns those into the typed
//! [`Selector`] and [`ParsedRule`] shapes the engine evaluates.

use crate::rule::{ParsedRule, RuleKind};
use crate::selector::Selector;
use yah_kg::anno::TagRef;

/// Errors surfaced when a rule's args are syntactically wrong. The engine
/// converts these into [`crate::Violation`]s rather than failing the whole
/// validate run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("empty selector")]
    EmptySelector,
    #[error("malformed selector {input:?}: {message}")]
    Malformed { input: String, message: String },
    #[error("rule {rule_kind:?} expects {expected} but got {got:?}")]
    Mismatch {
        rule_kind: String,
        expected: &'static str,
        got: String,
    },
}

/// Parse `(rule_kind, args)` into a typed [`ParsedRule`]. Unknown
/// rule_kinds surface as [`RuleKind::Unknown`] so the caller can decide
/// how to flag the vocabulary error — they aren't a `ParseError` because
/// the rule_kind itself is well-formed (not a syntactic problem).
pub fn parse_rule(rule_kind: &str, args: &[String]) -> Result<ParsedRule, ParseError> {
    let raw_kind = rule_kind.to_string();
    let normalized = rule_kind.trim().to_ascii_lowercase();
    let kind = match normalized.as_str() {
        "no-import-of" => RuleKind::NoImportOf {
            targets: parse_selector_list(rule_kind, args)?,
        },
        "no-dependency-on" => RuleKind::NoDependencyOn {
            targets: parse_selector_list(rule_kind, args)?,
        },
        "max-depth" => RuleKind::MaxDepth {
            depth: parse_single_u32(rule_kind, args)?,
        },
        "must-tag" => RuleKind::MustTag {
            namespace: parse_must_tag_arg(rule_kind, args)?,
        },
        _ => RuleKind::Unknown { raw: raw_kind.clone() },
    };
    Ok(ParsedRule {
        kind,
        raw_kind,
    })
}

fn parse_selector_list(rule_kind: &str, args: &[String]) -> Result<Vec<Selector>, ParseError> {
    if args.is_empty() {
        return Err(ParseError::Mismatch {
            rule_kind: rule_kind.to_string(),
            expected: "at least one selector argument",
            got: "no arguments".to_string(),
        });
    }
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        out.push(parse_selector(arg.trim())?);
    }
    Ok(out)
}

fn parse_single_u32(rule_kind: &str, args: &[String]) -> Result<u32, ParseError> {
    let [arg] = args else {
        return Err(ParseError::Mismatch {
            rule_kind: rule_kind.to_string(),
            expected: "exactly one integer argument",
            got: format!("{} arguments", args.len()),
        });
    };
    arg.trim().parse::<u32>().map_err(|_| ParseError::Mismatch {
        rule_kind: rule_kind.to_string(),
        expected: "an unsigned integer",
        got: arg.clone(),
    })
}

fn parse_must_tag_arg(rule_kind: &str, args: &[String]) -> Result<String, ParseError> {
    let [arg] = args else {
        return Err(ParseError::Mismatch {
            rule_kind: rule_kind.to_string(),
            expected: "exactly one namespace argument",
            got: format!("{} arguments", args.len()),
        });
    };
    let arg = arg.trim();
    // Accept `ns(layer)` (explicit) or bare `layer` (sugar).
    if let Some(inner) = call_form(arg, "ns") {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(ParseError::EmptySelector);
        }
        return Ok(inner.to_string());
    }
    if arg.is_empty() {
        return Err(ParseError::EmptySelector);
    }
    if !is_bare_identifier(arg) {
        return Err(ParseError::Malformed {
            input: arg.to_string(),
            message: "expected `ns(name)` or a bare namespace identifier".to_string(),
        });
    }
    Ok(arg.to_string())
}

/// Parse a single selector token. Recognized forms:
///
/// * `tag(name)` / `tag(ns:name)`
/// * `ns(name)`
/// * `node(qualified::name)`
/// * `kind(common::module)`
pub fn parse_selector(input: &str) -> Result<Selector, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParseError::EmptySelector);
    }
    if let Some(inner) = call_form(input, "tag") {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(ParseError::EmptySelector);
        }
        let tag = match inner.split_once(':') {
            Some((ns, name)) => TagRef::namespaced(ns.trim(), name.trim()),
            None => TagRef::new(inner),
        };
        return Ok(Selector::Tag(tag));
    }
    if let Some(inner) = call_form(input, "ns") {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(ParseError::EmptySelector);
        }
        return Ok(Selector::Namespace(inner.to_string()));
    }
    if let Some(inner) = call_form(input, "node") {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(ParseError::EmptySelector);
        }
        return Ok(Selector::QualifiedName(inner.to_string()));
    }
    if let Some(inner) = call_form(input, "kind") {
        let inner = inner.trim();
        if inner.is_empty() {
            return Err(ParseError::EmptySelector);
        }
        return Ok(Selector::Kind(inner.to_ascii_lowercase()));
    }
    Err(ParseError::Malformed {
        input: input.to_string(),
        message: "expected one of tag(...), ns(...), node(...), kind(...)".to_string(),
    })
}

/// Strip a single matched `name(...)` wrapper. Returns the inner contents
/// or `None` if the input doesn't match `name(<balanced>)`.
fn call_form<'a>(input: &'a str, name: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(name)?;
    let rest = rest.strip_prefix('(')?;
    let rest = rest.strip_suffix(')')?;
    Some(rest)
}

fn is_bare_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_no_import_of_with_one_tag() {
        let rule = parse_rule("no-import-of", &["tag(view)".to_string()]).unwrap();
        assert_eq!(rule.raw_kind, "no-import-of");
        match rule.kind {
            RuleKind::NoImportOf { targets } => {
                assert_eq!(targets, vec![Selector::Tag(TagRef::new("view"))]);
            }
            other => panic!("expected NoImportOf, got {:?}", other),
        }
    }

    #[test]
    fn parses_no_dependency_on_with_namespaced_tag() {
        let rule =
            parse_rule("no-dependency-on", &["tag(layer:view)".to_string()]).unwrap();
        match rule.kind {
            RuleKind::NoDependencyOn { targets } => {
                assert_eq!(
                    targets,
                    vec![Selector::Tag(TagRef::namespaced("layer", "view"))]
                );
            }
            other => panic!("expected NoDependencyOn, got {:?}", other),
        }
    }

    #[test]
    fn parses_max_depth_integer_arg() {
        let rule = parse_rule("max-depth", &["5".to_string()]).unwrap();
        assert!(matches!(rule.kind, RuleKind::MaxDepth { depth: 5 }));
    }

    #[test]
    fn parses_must_tag_with_bare_identifier() {
        let rule = parse_rule("must-tag", &["layer".to_string()]).unwrap();
        match rule.kind {
            RuleKind::MustTag { namespace } => assert_eq!(namespace, "layer"),
            other => panic!("expected MustTag, got {:?}", other),
        }
    }

    #[test]
    fn parses_must_tag_with_explicit_ns_form() {
        let rule = parse_rule("must-tag", &["ns(layer)".to_string()]).unwrap();
        match rule.kind {
            RuleKind::MustTag { namespace } => assert_eq!(namespace, "layer"),
            other => panic!("expected MustTag, got {:?}", other),
        }
    }

    #[test]
    fn unknown_rule_kind_preserves_raw_string() {
        let rule = parse_rule("no-cycle", &["tag(audio)".to_string()]).unwrap();
        assert_eq!(rule.raw_kind, "no-cycle");
        assert!(matches!(rule.kind, RuleKind::Unknown { .. }));
    }

    #[test]
    fn empty_args_for_no_import_of_is_an_error() {
        let err = parse_rule("no-import-of", &[]).unwrap_err();
        assert!(matches!(err, ParseError::Mismatch { .. }));
    }

    #[test]
    fn malformed_selector_is_an_error() {
        let err = parse_selector("nope[)").unwrap_err();
        assert!(matches!(err, ParseError::Malformed { .. }));
    }

    #[test]
    fn must_tag_rejects_call_form_other_than_ns() {
        let err = parse_rule("must-tag", &["tag(layer)".to_string()]).unwrap_err();
        assert!(matches!(err, ParseError::Malformed { .. }));
    }

    #[test]
    fn parses_node_and_kind_selectors() {
        let n = parse_selector("node(audio::mixer::Mixer)").unwrap();
        assert_eq!(n, Selector::QualifiedName("audio::mixer::Mixer".to_string()));
        let k = parse_selector("kind(common::Module)").unwrap();
        assert_eq!(k, Selector::Kind("common::module".to_string()));
    }

    #[test]
    fn max_depth_rejects_non_integer() {
        let err = parse_rule("max-depth", &["five".to_string()]).unwrap_err();
        assert!(matches!(err, ParseError::Mismatch { .. }));
    }
}
