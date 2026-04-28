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
//!
//! @yah:ticket(R017-T6, "Skip @yah: annotations inside backtick/code-fence spans in doc comments")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R017)

use yah_kg::anno::{TagRef, TicketStatus, WorkItemAnno};

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
    /// `@yah:relay(...)` or `@yah:ticket(...)` header plus the modifier
    /// directives (`@yah:status`, `@yah:next`, …) that followed it in
    /// the same block. Block boundaries are blank lines or the next
    /// header. The applier writes these into the AnnotationIndex as
    /// `AnnotationKind::Relay` or `AnnotationKind::Ticket`.
    WorkItem {
        item_type: WorkItemType,
        anno: WorkItemAnno,
    },
}

pub use yah_kg::anno::WorkItemType;

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
/// per directive in source order — except for `@yah:relay` /
/// `@yah:ticket` headers and their modifier directives (status,
/// assignee, parent, phase, severity, kind, handoff, next, gotcha,
/// assumes, verify, cleanup), which are grouped into a single
/// `RawAnnotation::WorkItem` per block. Block boundaries are blank
/// lines or the next header.
///
/// Lines that don't contain a directive are silently ignored. Malformed
/// directives become `ParseError`s but don't abort the rest of the
/// scan — the caller decides whether to surface them or skip.
pub fn parse_doc(doc: &str) -> (Vec<ParsedAnnotation>, Vec<ParseError>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut work_item: Option<WorkItemBuilder> = None;
    // Markdown fenced-block toggle. Lines whose first non-whitespace is
    // ``` (with optional language tag) flip this; while true the body of
    // the fence is treated as code and any `@yah:` directives inside are
    // example syntax, not real annotations.
    let mut in_fenced_block = false;

    for (idx, line) in doc.lines().enumerate() {
        let line_no = (idx + 1) as u32;

        if line.trim_start().starts_with("```") {
            in_fenced_block = !in_fenced_block;
            continue;
        }
        if in_fenced_block {
            continue;
        }

        // Blank-or-whitespace-only line closes any open work-item block.
        if line.trim().is_empty() {
            if let Some(builder) = work_item.take() {
                builder.flush(&mut out);
            }
            continue;
        }

        let mut cursor = line;
        loop {
            let Some((prefix, rest)) = find_directive(cursor) else {
                break;
            };
            let (kind, body, after) = match split_directive(rest) {
                Some(v) => v,
                None => break,
            };
            cursor = after;

            // `@arch:` is a different namespace; we only consume `@arch:see`
            // here because it attaches to the current work-item block as a
            // doc reference. `@arch:layer` / `@arch:role` / etc. live on
            // structural nodes — leave them for the structural extractors.
            if prefix == "arch" {
                if kind == "see" {
                    if let Some(builder) = work_item.as_mut() {
                        // see_also is a vec, never errors; reuse modifier path.
                        let _ = builder.apply_modifier("see", &body);
                    }
                    // No work-item header yet → silently ignore (mirrors
                    // free-floating @arch:see on a module top — those are
                    // handled by the structural pass, not us).
                }
                continue;
            }

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
                "relay" | "ticket" => {
                    // Header — flush any open block first, then start fresh.
                    if let Some(builder) = work_item.take() {
                        builder.flush(&mut out);
                    }
                    let item_type = if kind == "relay" {
                        WorkItemType::Relay
                    } else {
                        WorkItemType::Ticket
                    };
                    match parse_header_body(&body) {
                        Ok((id, title)) => {
                            work_item = Some(WorkItemBuilder::new(
                                item_type, id, title, line_no,
                            ));
                        }
                        Err(e) => errors.push(ParseError::Malformed {
                            kind: kind.to_string(),
                            line: line_no,
                            message: e,
                        }),
                    }
                }
                "status" | "assignee" | "parent" | "phase" | "severity" | "kind"
                | "handoff" | "next" | "gotcha" | "gotchas" | "assumes" | "verify"
                | "cleanup" => match work_item.as_mut() {
                    Some(builder) => {
                        if let Err(e) = builder.apply_modifier(kind, &body) {
                            errors.push(ParseError::Malformed {
                                kind: kind.to_string(),
                                line: line_no,
                                message: e,
                            });
                        }
                    }
                    None => errors.push(ParseError::Malformed {
                        kind: kind.to_string(),
                        line: line_no,
                        message: "modifier without preceding @yah:relay or @yah:ticket".into(),
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

    if let Some(builder) = work_item.take() {
        builder.flush(&mut out);
    }

    (out, errors)
}

/// Accumulator for a single relay/ticket block. Modifier directives
/// hit `apply_modifier`; on `flush` it materializes a `WorkItemAnno`
/// and pushes a `RawAnnotation::WorkItem` at the header's line offset.
struct WorkItemBuilder {
    item_type: WorkItemType,
    line_offset: u32,
    anno: WorkItemAnno,
}

impl WorkItemBuilder {
    fn new(item_type: WorkItemType, id: String, title: String, line_offset: u32) -> Self {
        Self {
            item_type,
            line_offset,
            anno: WorkItemAnno {
                id,
                title,
                ..Default::default()
            },
        }
    }

    fn apply_modifier(&mut self, kind: &str, body: &str) -> Result<(), String> {
        let stripped = strip_quotes(body.trim());
        match kind {
            "status" => {
                self.anno.status = Some(
                    TicketStatus::parse(&stripped)
                        .ok_or_else(|| format!("unknown status {:?}", stripped))?,
                );
            }
            "assignee" => self.anno.assignee = Some(stripped),
            "parent" => self.anno.parent = Some(stripped),
            "phase" => self.anno.phase = Some(stripped),
            "severity" => self.anno.severity = Some(stripped),
            "kind" => self.anno.kind = Some(stripped),
            "handoff" => self.anno.handoff.push(stripped),
            "next" => self.anno.next_steps.push(stripped),
            // Accept both "gotcha" (singular, what the docs prescribe) and
            // "gotchas" (plural, occasionally written).
            "gotcha" | "gotchas" => self.anno.gotchas.push(stripped),
            "assumes" => self.anno.assumes.push(stripped),
            "verify" => self.anno.verify.push(stripped),
            "cleanup" => self.anno.cleanup.push(stripped),
            "see" => self.anno.see_also.push(stripped),
            other => return Err(format!("unhandled modifier {:?}", other)),
        }
        Ok(())
    }

    fn flush(self, out: &mut Vec<ParsedAnnotation>) {
        out.push(ParsedAnnotation {
            anno: RawAnnotation::WorkItem {
                item_type: self.item_type,
                anno: self.anno,
            },
            line_offset: self.line_offset,
        });
    }
}

/// Parse `@yah:relay(ID, "title")` body. The first comma-separated
/// segment is the bare ID; the rest, joined back with commas, is the
/// title (trimmed and de-quoted).
fn parse_header_body(body: &str) -> Result<(String, String), String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("empty header — expected `ID, \"title\"`".into());
    }
    let (id_part, title_part) = match trimmed.split_once(',') {
        Some((id, rest)) => (id.trim().to_string(), rest.trim().to_string()),
        None => (trimmed.to_string(), String::new()),
    };
    if id_part.is_empty() {
        return Err("missing ID before comma".into());
    }
    Ok((id_part, strip_quotes(&title_part)))
}

/// Strip a single matching pair of surrounding ASCII double-quotes.
/// Authors sometimes drop quotes for short single-token bodies — accept
/// the raw value in that case.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Find the next `@yah:` or `@arch:` substring in `s` outside of inline
/// backtick code spans. Returns the namespace prefix (`"yah"` or `"arch"`)
/// alongside everything after the colon. `None` if no directive is found.
/// Backtick state resets per call (per line) — an unmatched backtick at
/// end of line doesn't bleed into the next.
fn find_directive(s: &str) -> Option<(&'static str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_code = false;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code {
            if bytes[i..].starts_with(b"@yah:") {
                return Some(("yah", &s[i + 5..]));
            }
            if bytes[i..].starts_with(b"@arch:") {
                return Some(("arch", &s[i + 6..]));
            }
        }
        i += 1;
    }
    None
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

    // ---------- Relay / Ticket parsing ----------

    fn unwrap_work_item(p: &ParsedAnnotation) -> (WorkItemType, &WorkItemAnno) {
        match &p.anno {
            RawAnnotation::WorkItem { item_type, anno } => (*item_type, anno),
            other => panic!("expected WorkItem, got {:?}", other),
        }
    }

    #[test]
    fn parses_bare_relay_header() {
        let (out, errs) = parse_doc("@yah:relay(R007, \"Auth refactor\")");
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1);
        let (kind, anno) = unwrap_work_item(&out[0]);
        assert_eq!(kind, WorkItemType::Relay);
        assert_eq!(anno.id, "R007");
        assert_eq!(anno.title, "Auth refactor");
        assert_eq!(out[0].line_offset, 1);
    }

    #[test]
    fn relay_with_modifiers_groups_into_one_workitem() {
        let doc = r#"@yah:relay(R024, "Multi-rig host")
@yah:status(in-progress)
@yah:assignee(agent:claude)
@yah:parent(R013)
@yah:phase(P2)
@yah:handoff("Built three sub-tickets.")
@yah:next("Land T1 first")
@yah:next("Then T2/T3 in parallel")
@yah:gotcha("watcher races on save")
@yah:verify("cargo build -p yah-tauri")"#;
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1, "all directives collapse into one block");
        let (kind, anno) = unwrap_work_item(&out[0]);
        assert_eq!(kind, WorkItemType::Relay);
        assert_eq!(anno.id, "R024");
        assert_eq!(anno.title, "Multi-rig host");
        assert_eq!(anno.status, Some(TicketStatus::InProgress));
        assert_eq!(anno.assignee.as_deref(), Some("agent:claude"));
        assert_eq!(anno.parent.as_deref(), Some("R013"));
        assert_eq!(anno.phase.as_deref(), Some("P2"));
        assert_eq!(anno.handoff, vec!["Built three sub-tickets.".to_string()]);
        assert_eq!(
            anno.next_steps,
            vec!["Land T1 first".to_string(), "Then T2/T3 in parallel".to_string()]
        );
        assert_eq!(anno.gotchas, vec!["watcher races on save".to_string()]);
        assert_eq!(anno.verify, vec!["cargo build -p yah-tauri".to_string()]);
    }

    #[test]
    fn blank_line_closes_block_and_starts_new_one() {
        let doc = r#"@yah:relay(R001, "First")
@yah:status(open)

@yah:ticket(R001-T1, "Sub task")
@yah:status(in-progress)
@yah:parent(R001)"#;
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 2);
        let (k1, a1) = unwrap_work_item(&out[0]);
        let (k2, a2) = unwrap_work_item(&out[1]);
        assert_eq!(k1, WorkItemType::Relay);
        assert_eq!(a1.id, "R001");
        assert_eq!(a1.status, Some(TicketStatus::Open));
        assert_eq!(k2, WorkItemType::Ticket);
        assert_eq!(a2.id, "R001-T1");
        assert_eq!(a2.status, Some(TicketStatus::InProgress));
        assert_eq!(a2.parent.as_deref(), Some("R001"));
    }

    #[test]
    fn back_to_back_headers_close_each_other_without_blank_line() {
        let doc = r#"@yah:relay(R001, "First")
@yah:status(open)
@yah:ticket(R001-T1, "Sub")
@yah:parent(R001)"#;
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 2);
        let (_, a1) = unwrap_work_item(&out[0]);
        let (_, a2) = unwrap_work_item(&out[1]);
        assert_eq!(a1.id, "R001");
        assert!(a2.parent.as_deref() == Some("R001"));
        // Modifier landed on the right block, not the previous one.
        assert!(a1.parent.is_none());
    }

    #[test]
    fn modifier_without_header_is_an_error() {
        let (out, errs) = parse_doc("@yah:status(open)");
        assert!(out.is_empty());
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn ticket_with_kind_override_records_kind() {
        let doc = r#"@yah:ticket(R007-T2, "Crash on cold start")
@yah:kind(bug)
@yah:severity(high)
@yah:parent(R007)"#;
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        let (_, anno) = unwrap_work_item(&out[0]);
        assert_eq!(anno.kind.as_deref(), Some("bug"));
        assert_eq!(anno.severity.as_deref(), Some("high"));
    }

    #[test]
    fn relay_alongside_tag_emits_both() {
        let doc = r#"@yah:tag(layer:core)
@yah:relay(R001, "Title")
@yah:status(open)"#;
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0].anno, RawAnnotation::Tag(_)));
        assert!(matches!(out[1].anno, RawAnnotation::WorkItem { .. }));
    }

    #[test]
    fn skips_directive_inside_inline_backticks() {
        // The ID/title example here is documentation, not a real annotation.
        let (out, errs) =
            parse_doc("Header form is `@yah:relay(ID, \"title\")` plus modifiers.");
        assert!(out.is_empty(), "phantom relay leaked: {:?}", out);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn skips_directive_inside_fenced_code_block() {
        let doc = "Use it like this:\n```\n@yah:relay(R001, \"Demo\")\n@yah:status(open)\n```\nEnd of example.";
        let (out, errs) = parse_doc(doc);
        assert!(out.is_empty(), "phantom relay leaked: {:?}", out);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn fenced_block_with_language_tag_still_skipped() {
        let doc = "Example:\n```rust\n@yah:tag(audio)\n```\nReal: @yah:tag(real)";
        let (out, errs) = parse_doc(doc);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1, "only the post-fence tag should land");
        let RawAnnotation::Tag(tags) = &out[0].anno else {
            panic!("expected tag, got {:?}", out[0]);
        };
        assert_eq!(tags, &vec![TagRef::new("real")]);
    }

    #[test]
    fn directive_outside_backticks_on_same_line_still_parses() {
        // `@yah:relay(...)` is example syntax; the trailing real one lands.
        let (out, errs) = parse_doc(
            "See `@yah:relay(...)` form. @yah:tag(audio)",
        );
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(out.len(), 1);
        let RawAnnotation::Tag(tags) = &out[0].anno else {
            panic!("expected tag, got {:?}", out[0]);
        };
        assert_eq!(tags, &vec![TagRef::new("audio")]);
    }

    #[test]
    fn unknown_status_value_is_an_error_but_block_continues() {
        let doc = r#"@yah:relay(R001, "T")
@yah:status(bogus)
@yah:next("ok step")"#;
        let (out, errs) = parse_doc(doc);
        assert_eq!(errs.len(), 1, "bogus status is the only error");
        assert_eq!(out.len(), 1);
        let (_, anno) = unwrap_work_item(&out[0]);
        assert_eq!(anno.status, None);
        assert_eq!(anno.next_steps, vec!["ok step".to_string()]);
    }
}
