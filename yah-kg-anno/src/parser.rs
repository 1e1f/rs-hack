//! @arch:layer(kg_store)
//! @arch:role(extract)
//!
//! Doc-string annotation parser.
//!
//! Input: a doc string already extracted by a language indexer (the
//! comment markers `///`, `//!`, `/** */`, `*` have been stripped). Each
//! line of the doc is examined for a `@yah:<kind>(<payload>)` directive.
//!
//! Robust to:
//! * arbitrary leading whitespace,
//! * payloads containing balanced parens (we count nesting),
//! * unicode arrows `→` and ASCII `->` for flows,
//! * commented-out lines and prose around the directive.
//!
//! Permissive on payload form within each kind — authors get a clean
//! parse error message rather than silent dropping.

use yah_kg::anno::TagRef;

/// Pre-resolution annotation: like `AnnotationKind` but without the
/// anchor `NodeId` (which is supplied by the applier when it knows
/// which node owns the doc string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawAnnotation {
    Tag(Vec<TagRef>),
    Flow {
        from_qualified: Option<String>,
        to_qualified: String,
        reason: Option<String>,
    },
    Rule {
        rule_kind: String,
        args: Vec<String>,
    },
}

/// One parsed annotation plus the relative line within the doc string
/// where it was found (1-based; index 1 is the first line of `doc`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAnnotation {
    pub anno: RawAnnotation,
    pub line_offset: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("malformed @yah:{kind} payload at line offset {line}: {message}")]
    Malformed {
        kind: String,
        line: u32,
        message: String,
    },
}

/// Scan a doc string for `@yah:` directives. Returns one `ParsedAnnotation`
/// per directive in source order. Lines that don't contain a directive
/// are silently ignored. Malformed directives become `ParseError`s but
/// don't abort the rest of the scan — the caller decides whether to
/// surface them or skip.
pub fn parse_doc(doc: &str) -> (Vec<ParsedAnnotation>, Vec<ParseError>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();

    for (idx, line) in doc.lines().enumerate() {
        let line_no = (idx + 1) as u32;
        let mut cursor = line;

        loop {
            let Some(rest) = find_directive(cursor) else {
                break;
            };
            // `rest` starts at the first char after `@yah:`.
            let (kind, body, after) = match split_directive(rest) {
                Some(v) => v,
                None => break,
            };
            cursor = after;

            match kind {
                "tag" => match parse_tag_body(&body) {
                    Ok(tags) if !tags.is_empty() => out.push(ParsedAnnotation {
                        anno: RawAnnotation::Tag(tags),
                        line_offset: line_no,
                    }),
                    Ok(_) => errors.push(ParseError::Malformed {
                        kind: "tag".into(),
                        line: line_no,
                        message: "no tags inside the parentheses".into(),
                    }),
                    Err(e) => errors.push(ParseError::Malformed {
                        kind: "tag".into(),
                        line: line_no,
                        message: e,
                    }),
                },
                "flow" => match parse_flow_body(&body) {
                    Ok(flow) => out.push(ParsedAnnotation {
                        anno: flow,
                        line_offset: line_no,
                    }),
                    Err(e) => errors.push(ParseError::Malformed {
                        kind: "flow".into(),
                        line: line_no,
                        message: e,
                    }),
                },
                "rule" => match parse_rule_body(&body) {
                    Ok(rule) => out.push(ParsedAnnotation {
                        anno: rule,
                        line_offset: line_no,
                    }),
                    Err(e) => errors.push(ParseError::Malformed {
                        kind: "rule".into(),
                        line: line_no,
                        message: e,
                    }),
                },
                other => errors.push(ParseError::Malformed {
                    kind: other.to_string(),
                    line: line_no,
                    message: "unknown @yah: directive".into(),
                }),
            }
        }
    }

    (out, errors)
}

/// Find the next `@yah:` substring in `s` and return everything after the
/// colon. Returns `None` if no directive on the line.
fn find_directive(s: &str) -> Option<&str> {
    s.find("@yah:").map(|i| &s[i + 5..])
}

/// Given input that begins right after `@yah:`, parse out
/// `(kind, body, remaining_after_directive)`. Body excludes the outer
/// parentheses. Returns `None` if the directive is malformed beyond
/// recovery (in which case parsing of the rest of the line stops).
fn split_directive(s: &str) -> Option<(&str, String, &str)> {
    // Kind is the run of identifier-like chars before `(`.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let kind = &s[..i];
    if bytes.get(i)? != &b'(' {
        return None;
    }
    let body_start = i + 1;
    let mut depth = 1;
    let mut j = body_start;
    while j < bytes.len() {
        match bytes[j] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let body = s[body_start..j].to_string();
                    return Some((kind, body, &s[j + 1..]));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None // unterminated
}

fn parse_tag_body(body: &str) -> Result<Vec<TagRef>, String> {
    let mut tags = Vec::new();
    for part in body.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !is_valid_tag_string(part) {
            return Err(format!("invalid tag {:?}", part));
        }
        let tag = match part.split_once(':') {
            Some((ns, name)) => TagRef::namespaced(ns.trim(), name.trim()),
            None => TagRef::new(part),
        };
        tags.push(tag);
    }
    Ok(tags)
}

fn is_valid_tag_string(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
}

fn parse_flow_body(body: &str) -> Result<RawAnnotation, String> {
    // Split off the optional ", \"reason\"" tail.
    let (head, reason) = split_off_reason(body);
    // Find `→` (unicode) or `->` (ASCII).
    let (from_part, to_part) = if let Some(idx) = head.find('→') {
        (&head[..idx], &head[idx + '→'.len_utf8()..])
    } else if let Some(idx) = head.find("->") {
        (&head[..idx], &head[idx + 2..])
    } else {
        // Single-target form: flow(target) — implicit "from" is the
        // annotated node itself. The applier supplies the anchor.
        return Ok(RawAnnotation::Flow {
            from_qualified: None,
            to_qualified: head.trim().to_string(),
            reason,
        });
    };
    let from = from_part.trim();
    let to = to_part.trim();
    if to.is_empty() {
        return Err("missing flow target".into());
    }
    Ok(RawAnnotation::Flow {
        from_qualified: if from.is_empty() {
            None
        } else {
            Some(from.to_string())
        },
        to_qualified: to.to_string(),
        reason,
    })
}

fn split_off_reason(body: &str) -> (&str, Option<String>) {
    // Reason is the last `"..."` quoted segment after a comma, when present.
    let Some(comma_idx) = body.rfind(',') else {
        return (body, None);
    };
    let tail = body[comma_idx + 1..].trim();
    if let (Some(stripped), true) = (tail.strip_prefix('"').and_then(|s| s.strip_suffix('"')), tail.starts_with('"')) {
        (&body[..comma_idx], Some(stripped.to_string()))
    } else {
        (body, None)
    }
}

/// Split on top-level `,` only — treats commas inside `(...)` as part of
/// the argument. So `cycle(a, b, c), other` splits to two args, not five.
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = cur.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    let trimmed = cur.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

fn parse_rule_body(body: &str) -> Result<RawAnnotation, String> {
    // Form: rule_kind: arg1, arg2, ...
    let (kind_part, arg_part) = body.split_once(':').ok_or_else(|| {
        "expected `rule_kind: args` form (e.g. `no-import-of: tag(view)`)".to_string()
    })?;
    let rule_kind = kind_part.trim();
    if rule_kind.is_empty() {
        return Err("empty rule kind".into());
    }
    let args = split_top_level_commas(arg_part);
    Ok(RawAnnotation::Rule {
        rule_kind: rule_kind.to_string(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tag() {
        let (out, errs) = parse_doc("@yah:tag(audio)");
        assert!(errs.is_empty());
        assert_eq!(
            out,
            vec![ParsedAnnotation {
                anno: RawAnnotation::Tag(vec![TagRef::new("audio")]),
                line_offset: 1,
            }]
        );
    }

    #[test]
    fn parses_multiple_tags_in_one_directive() {
        let (out, errs) = parse_doc("@yah:tag(audio, hot-path, layer:core)");
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1);
        let RawAnnotation::Tag(tags) = &out[0].anno else {
            panic!("expected tag, got {:?}", out[0]);
        };
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0], TagRef::new("audio"));
        assert_eq!(tags[1], TagRef::new("hot-path"));
        assert_eq!(tags[2], TagRef::namespaced("layer", "core"));
    }

    #[test]
    fn parses_flow_with_arrow_and_reason() {
        let (out, errs) = parse_doc(
            "Doc text first.\n@yah:flow(audio::mixer → dispatch::loop, \"shared frame buffer\")",
        );
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].line_offset, 2);
        let RawAnnotation::Flow {
            from_qualified,
            to_qualified,
            reason,
        } = &out[0].anno
        else {
            panic!("expected flow, got {:?}", out[0]);
        };
        assert_eq!(from_qualified.as_deref(), Some("audio::mixer"));
        assert_eq!(to_qualified, "dispatch::loop");
        assert_eq!(reason.as_deref(), Some("shared frame buffer"));
    }

    #[test]
    fn parses_flow_with_ascii_arrow() {
        let (out, errs) = parse_doc("@yah:flow(a -> b)");
        assert!(errs.is_empty(), "{errs:?}");
        let RawAnnotation::Flow {
            to_qualified,
            from_qualified,
            ..
        } = &out[0].anno
        else {
            panic!()
        };
        assert_eq!(to_qualified, "b");
        assert_eq!(from_qualified.as_deref(), Some("a"));
    }

    #[test]
    fn parses_implicit_from_flow() {
        let (out, errs) = parse_doc("@yah:flow(target_module::Thing)");
        assert!(errs.is_empty(), "{errs:?}");
        let RawAnnotation::Flow {
            from_qualified,
            to_qualified,
            ..
        } = &out[0].anno
        else {
            panic!()
        };
        assert_eq!(from_qualified, &None);
        assert_eq!(to_qualified, "target_module::Thing");
    }

    #[test]
    fn parses_rule() {
        let (out, errs) = parse_doc("@yah:rule(no-import-of: tag(view), tag(io))");
        assert!(errs.is_empty(), "{errs:?}");
        let RawAnnotation::Rule { rule_kind, args } = &out[0].anno else {
            panic!()
        };
        assert_eq!(rule_kind, "no-import-of");
        assert_eq!(args, &vec!["tag(view)".to_string(), "tag(io)".to_string()]);
    }

    #[test]
    fn rejects_unknown_directive() {
        let (out, errs) = parse_doc("@yah:wat(stuff)");
        assert!(out.is_empty());
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn handles_multi_line_doc_with_prose() {
        let doc = "Top-level mixer.\n\nBack story prose.\n@yah:tag(audio)\nMore prose.";
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].line_offset, 4);
    }

    #[test]
    fn ignores_doc_without_directives() {
        let (out, errs) = parse_doc("just prose, no @anno here");
        assert!(out.is_empty());
        assert!(errs.is_empty());
    }

    #[test]
    fn handles_balanced_parens_in_payload() {
        let (out, errs) = parse_doc("@yah:rule(no-cycle: cycle(a, b, c))");
        assert!(errs.is_empty(), "{errs:?}");
        let RawAnnotation::Rule { args, .. } = &out[0].anno else {
            panic!()
        };
        assert_eq!(args, &vec!["cycle(a, b, c)".to_string()]);
    }

    #[test]
    fn surfaces_invalid_tag_chars() {
        let (out, errs) = parse_doc("@yah:tag(bad space)");
        assert!(out.is_empty(), "should not emit invalid tag");
        assert_eq!(errs.len(), 1);
    }
}
