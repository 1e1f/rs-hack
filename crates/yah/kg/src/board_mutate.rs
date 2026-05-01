//! @arch:layer(kg)
//! @arch:role(mutation)
//!
//! Pure-string helpers for rewriting `@yah:status(...)` (and other
//! single-key annotations) inside a ticket's contiguous doc-comment block.
//!
//! Two consumers today: the `yah board move` CLI ([`yah/src/main.rs`])
//! and `KgService::move_ticket` (called from the Tauri shell). The CLI
//! still ships its own copy of these helpers — extracting *here* keeps
//! the daemon out of binary-crate-only territory; folding the CLI onto
//! these is a follow-up cleanup, not a load-bearing change.
//!
//! Everything in this module is **pure**: no I/O, no allocation beyond
//! the rewritten `String`. The caller is responsible for reading the
//! source file, calling [`rewrite_status_in_source`], and writing the
//! result back.

use std::fmt;

/// All authorial column buckets the UI surfaces. The same matrix is
/// codified — for now — in `yah/src/main.rs` (CLI) and in
/// `hack-board/src/server.ts` (TS server). Cleanup ticket flagged at
/// `yah/src/arch/extract.rs:8` will fold those onto this module.
pub const COLUMNS: &[&str] = &["open", "active", "handoff", "review"];

/// Map a column bucket (the strings the renderer / drag-and-drop UI
/// emits) to the canonical `@yah:status(...)` value to write into source.
/// Returns `None` for an unrecognized bucket.
pub fn bucket_to_status(bucket: &str) -> Option<&'static str> {
    match bucket {
        "open" => Some("open"),
        "active" => Some("in-progress"),
        "handoff" => Some("handoff"),
        "review" => Some("review"),
        _ => None,
    }
}

/// Reverse of [`bucket_to_status`]: collapse a canonical status to its
/// bucket. `claimed` and `in-progress` both map to `active`; `done`
/// folds in with `review`. Returns `""` when the status is not one we
/// recognize (treat as a hard error at the call site).
pub fn status_to_bucket(status: &str) -> &'static str {
    match status {
        "open" => "open",
        "claimed" | "in-progress" => "active",
        "handoff" => "handoff",
        "review" | "done" => "review",
        _ => "",
    }
}

/// Allowed column-to-column transitions. Mirrors the matrix the kanban
/// UI dims (`hack-board/src/server.ts`'s `TRANSITIONS`):
/// - open → active
/// - active → {open, handoff, review}
/// - handoff → {active, review}
/// - review → handoff
pub fn allowed_transitions(from: &str) -> &'static [&'static str] {
    match from {
        "open" => &["active"],
        "active" => &["open", "handoff", "review"],
        "handoff" => &["active", "review"],
        "review" => &["handoff"],
        _ => &[],
    }
}

/// Errors returned by [`rewrite_status_in_source`]. Caller-facing — the
/// daemon wraps these in `DaemonError::Io` / a domain error of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutateError {
    /// No `@yah:ticket(<id>, ...)` or `@yah:relay(<id>, ...)` declaration
    /// was found in the supplied source.
    NotFound { id: String },
}

impl fmt::Display for MutateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MutateError::NotFound { id } => write!(
                f,
                "no @yah:ticket({}, ...) or @yah:relay({}, ...) declaration in source",
                id, id
            ),
        }
    }
}

impl std::error::Error for MutateError {}

/// Rewrite the `@yah:status(...)` line for `id` inside `content` to
/// `new_status`. If no `@yah:status(...)` line exists in the block,
/// insert one immediately after the declaration.
///
/// `new_status` should be the canonical kebab-case form (e.g.
/// `in-progress`, `review`) — typically what [`bucket_to_status`]
/// returned. The function does **not** validate `new_status`; the
/// caller is responsible for the bucket→status mapping and any
/// transition-matrix check.
///
/// Returns the rewritten source. Returns the input unchanged when the
/// status was already set to `new_status`.
pub fn rewrite_status_in_source(
    content: &str,
    id: &str,
    new_status: &str,
) -> Result<String, MutateError> {
    let block = locate_ticket_block(content, id).ok_or_else(|| MutateError::NotFound {
        id: id.to_string(),
    })?;
    Ok(set_or_insert_annotation(
        content, &block, "status", new_status,
    ))
}

/// 1-indexed span of one ticket's contiguous doc-comment annotation
/// block. `decl_line` always lies within `[start_line, end_line]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketBlock {
    /// Line carrying `@yah:ticket(<id>, ...)` or `@yah:relay(<id>, ...)`.
    pub decl_line: usize,
    /// First line of the block (an `@yah:` / `@arch:` line).
    pub start_line: usize,
    /// Last line of the block (an `@yah:` / `@arch:` line).
    pub end_line: usize,
}

/// Locate the contiguous run of `@yah:` / `@arch:` annotation lines
/// that owns `id`. The block is delimited above and below by:
///   - a non-doc-comment line, or
///   - a blank doc-comment line (`//!`, `///`, or `#` with no payload),
///     or
///   - another `@yah:ticket(...)` / `@yah:relay(...)` declaration.
///
/// Mirrors the CLI's `locate_ticket_block` (`yah/src/main.rs`).
pub fn locate_ticket_block(content: &str, id: &str) -> Option<TicketBlock> {
    let lines: Vec<&str> = content.split('\n').collect();

    let decl_ticket = format!("@yah:ticket({},", id);
    let decl_relay = format!("@yah:relay({},", id);
    let decl_ticket_sp = format!("@yah:ticket({} ,", id);
    let decl_relay_sp = format!("@yah:relay({} ,", id);

    let is_this_decl = |line: &str| -> bool {
        line.contains(&decl_ticket)
            || line.contains(&decl_relay)
            || line.contains(&decl_ticket_sp)
            || line.contains(&decl_relay_sp)
    };
    let is_any_decl =
        |line: &str| -> bool { line.contains("@yah:ticket(") || line.contains("@yah:relay(") };

    let is_doc = |i: usize| -> bool { comment_sigil(lines[i]).is_some() };
    let is_blank_doc = |i: usize| -> bool {
        let t = lines[i].trim();
        t == "//!" || t == "///" || t == "#"
    };
    let is_yah_or_arch = |i: usize| -> bool {
        comment_sigil(lines[i]).is_some()
            && (lines[i].contains("@yah:") || lines[i].contains("@arch:"))
    };

    let decl_idx = (0..lines.len()).find(|&i| is_doc(i) && is_this_decl(lines[i]))?;

    let mut start = decl_idx;
    while start > 0 {
        let prev = start - 1;
        if !is_doc(prev) || is_blank_doc(prev) {
            break;
        }
        if prev != decl_idx && is_yah_or_arch(prev) && is_any_decl(lines[prev]) {
            break;
        }
        if !is_yah_or_arch(prev) {
            break;
        }
        start = prev;
    }

    let mut end = decl_idx;
    while end + 1 < lines.len() {
        let nxt = end + 1;
        if !is_doc(nxt) || is_blank_doc(nxt) {
            break;
        }
        if is_yah_or_arch(nxt) && is_any_decl(lines[nxt]) {
            break;
        }
        if !is_yah_or_arch(nxt) {
            break;
        }
        end = nxt;
    }

    Some(TicketBlock {
        decl_line: decl_idx + 1,
        start_line: start + 1,
        end_line: end + 1,
    })
}

/// Set/insert `@yah:at(<rfc3339>)` for `id`. Used by daemon mutation
/// paths (currently `move_ticket`) so each ticket carries a per-ticket
/// "last touched" timestamp, decoupled from the surrounding file's mtime.
pub fn touch_in_source(content: &str, id: &str, at: &str) -> Result<String, MutateError> {
    let block = locate_ticket_block(content, id).ok_or_else(|| MutateError::NotFound {
        id: id.to_string(),
    })?;
    Ok(set_or_insert_annotation(content, &block, "at", at))
}

/// Rewrite `@yah:<key>(<value>)` inside `block` if present; otherwise
/// insert it on the line immediately after the declaration, preserving
/// the declaration's indentation and comment sigil.
pub fn set_or_insert_annotation(
    content: &str,
    block: &TicketBlock,
    key: &str,
    value: &str,
) -> String {
    let mut lines: Vec<String> = content.split('\n').map(|s| s.to_string()).collect();
    let needle = format!("@yah:{}(", key);

    for i in (block.start_line - 1)..block.end_line {
        let Some((indent, sigil, after)) = comment_sigil(&lines[i]) else {
            continue;
        };
        if after.trim_start().starts_with(&needle) {
            lines[i] = format!("{}{} @yah:{}({})", indent, sigil, key, value);
            return lines.join("\n");
        }
    }

    let decl_idx = block.decl_line - 1;
    let prefix = extract_doc_prefix(&lines[decl_idx]);
    let new_line = format!("{} @yah:{}({})", prefix, key, value);
    lines.insert(decl_idx + 1, new_line);
    lines.join("\n")
}

fn extract_doc_prefix(line: &str) -> String {
    match comment_sigil(line) {
        Some((indent, sigil, _)) => format!("{}{}", indent, sigil),
        None => {
            let ws_end = line.find(|c: char| !c.is_whitespace()).unwrap_or(0);
            format!("{}//!", &line[..ws_end])
        }
    }
}

/// Recognize a yah/arch annotation comment sigil at the start of `line`.
/// Returns `(leading_indent, sigil, rest_after_sigil)` for `//!`, `///`,
/// or TOML/YAML `#` (excluding Rust attributes `#[...]` / `#![...]`).
pub fn comment_sigil(line: &str) -> Option<(&str, &'static str, &str)> {
    let trimmed = line.trim_start();
    let prefix_end = line.len() - trimmed.len();
    let indent = &line[..prefix_end];
    if let Some(rest) = trimmed.strip_prefix("//!") {
        return Some((indent, "//!", rest));
    }
    if let Some(rest) = trimmed.strip_prefix("///") {
        return Some((indent, "///", rest));
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        if rest.starts_with('[') || rest.starts_with('!') {
            return None;
        }
        return Some((indent, "#", rest));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_match_ui_matrix() {
        assert_eq!(allowed_transitions("open"), &["active"]);
        assert_eq!(
            allowed_transitions("active"),
            &["open", "handoff", "review"]
        );
        assert_eq!(allowed_transitions("handoff"), &["active", "review"]);
        assert_eq!(allowed_transitions("review"), &["handoff"]);
        assert_eq!(allowed_transitions("nope"), &[] as &[&str]);
    }

    #[test]
    fn bucket_status_round_trip() {
        for col in ["open", "active", "handoff", "review"] {
            let s = bucket_to_status(col).unwrap();
            assert_eq!(status_to_bucket(s), col, "column {col} round-trip");
        }
        // Synonyms still resolve to the right bucket.
        assert_eq!(status_to_bucket("claimed"), "active");
        assert_eq!(status_to_bucket("done"), "review");
    }

    #[test]
    fn rewrite_existing_status_in_rust_doc_block() {
        let src = "//! @yah:ticket(R001-T1, \"x\")\n//! @yah:status(open)\n//! @yah:next(\"go\")\n";
        let out = rewrite_status_in_source(src, "R001-T1", "in-progress").unwrap();
        let expected =
            "//! @yah:ticket(R001-T1, \"x\")\n//! @yah:status(in-progress)\n//! @yah:next(\"go\")\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn insert_status_when_missing_preserves_indent_and_sigil() {
        let src = "    /// @yah:ticket(R001-T1, \"x\")\n    /// @yah:next(\"go\")\n";
        let out = rewrite_status_in_source(src, "R001-T1", "review").unwrap();
        let expected = "    /// @yah:ticket(R001-T1, \"x\")\n    /// @yah:status(review)\n    /// @yah:next(\"go\")\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn rewrite_only_targets_requested_id() {
        let src = "//! @yah:ticket(R001-T1, \"x\")\n//! @yah:status(open)\n//!\n//! @yah:ticket(R002, \"y\")\n//! @yah:status(open)\n";
        let out = rewrite_status_in_source(src, "R002", "handoff").unwrap();
        let expected = "//! @yah:ticket(R001-T1, \"x\")\n//! @yah:status(open)\n//!\n//! @yah:ticket(R002, \"y\")\n//! @yah:status(handoff)\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn rewrite_in_toml_style_block() {
        let src = "# @yah:ticket(B042, \"toml-bug\")\n# @yah:status(open)\n";
        let out = rewrite_status_in_source(src, "B042", "active").unwrap();
        // bucket_to_status("active") → "in-progress"; here we pass the
        // canonical status directly (caller's responsibility).
        let expected = "# @yah:ticket(B042, \"toml-bug\")\n# @yah:status(active)\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn missing_id_is_not_found_error() {
        let src = "//! @yah:ticket(R001-T1, \"x\")\n//! @yah:status(open)\n";
        let err = rewrite_status_in_source(src, "R999", "review").unwrap_err();
        assert!(matches!(err, MutateError::NotFound { .. }));
    }
}
