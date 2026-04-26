//! SDLC-compliance heuristics: write-time warnings (when an annotation is
//! about to grow into a journal) and read-time smells (when an existing
//! ticket already shows the pattern).
//!
//! These are *suggestions*, never refusals — a human or agent with context
//! can ignore them. The goal is to surface Rule03 (replace-in-place) and
//! Rule09 (concrete chunks belong on sub-tickets) at the moment they're
//! being violated, instead of after the relay has accumulated 1.5kB of
//! dated handoff prose.

use crate::ticket::{Ticket, TicketBoard, TicketStatus};

/// Total handoff bytes above which we warn (combined existing + incoming).
const HANDOFF_LENGTH_THRESHOLD: usize = 500;

/// Total next-step bytes above which we warn.
const NEXT_LENGTH_THRESHOLD: usize = 600;

/// Count occurrences of `YYYY-MM-DD` substrings in `s`.
///
/// Hand-rolled to avoid pulling in regex; the pattern is fixed-shape.
fn count_dated_markers(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut n = 0;
    let mut i = 0;
    while i + 10 <= bytes.len() {
        let w = &bytes[i..i + 10];
        let shape_ok = w[0].is_ascii_digit()
            && w[1].is_ascii_digit()
            && w[2].is_ascii_digit()
            && w[3].is_ascii_digit()
            && w[4] == b'-'
            && w[5].is_ascii_digit()
            && w[6].is_ascii_digit()
            && w[7] == b'-'
            && w[8].is_ascii_digit()
            && w[9].is_ascii_digit();
        let left_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
        let right_ok = i + 10 == bytes.len() || !bytes[i + 10].is_ascii_digit();
        if shape_ok && left_ok && right_ok {
            n += 1;
            i += 10;
        } else {
            i += 1;
        }
    }
    n
}

/// Heuristic: does a `--next` line read like a concrete chunk of work
/// (Rule09 violation) rather than cross-ticket guidance? Conservative —
/// we'd rather miss a real violation than false-positive on legitimate
/// strategy notes.
fn next_looks_like_concrete_chunk(s: &str) -> bool {
    let lower = s.to_lowercase();
    let mentions_alloc_verb = lower.contains("rs-hack board claim")
        || lower.contains("rs-hack board open")
        || lower.contains("board claim --kind")
        || lower.contains("board open --kind");
    let has_numbered_recipe =
        s.contains("(1)") && (s.contains("(2)") || s.contains("(3)"));
    let starts_with_action = matches!(
        lower.split_whitespace().next(),
        Some("open" | "add" | "implement" | "wire" | "create" | "write" | "fix")
    ) && s.len() > 200;
    mentions_alloc_verb || has_numbered_recipe || starts_with_action
}

/// Warnings emitted at write time by `board move` / `board open` / `board
/// claim`. Returned warnings are printed to stderr as "⚠ ..." lines; the
/// write itself proceeds.
pub fn write_time_warnings(
    existing: Option<&Ticket>,
    new_handoff: &[String],
    new_next: &[String],
) -> Vec<String> {
    let mut out = Vec::new();

    let existing_handoff_total: usize =
        existing.map(|t| t.handoff.iter().map(|h| h.len()).sum()).unwrap_or(0);
    let existing_handoff_count =
        existing.map(|t| t.handoff.len()).unwrap_or(0);
    let new_handoff_total: usize = new_handoff.iter().map(|h| h.len()).sum();

    if existing_handoff_count > 0 && new_handoff_total > 0 {
        let combined = existing_handoff_total + new_handoff_total;
        if combined > HANDOFF_LENGTH_THRESHOLD {
            out.push(format!(
                "Rule03: handoff is becoming a journal — {} existing entries ({}B) + {}B new = {}B total. \
                 The events.jsonl log preserves history; the annotation should be a snapshot. \
                 Consider replacing the current handoff with a one-line state-of-play instead of stacking.",
                existing_handoff_count, existing_handoff_total, new_handoff_total, combined
            ));
        }
    }

    let dated_in_new: usize = new_handoff.iter().map(|h| count_dated_markers(h)).sum();
    if dated_in_new > 0 {
        out.push(format!(
            "Rule03: new handoff text contains {} dated marker(s) (e.g. '2026-04-25 pickup'). \
             The events log already timestamps every modify; date stamps in handoff prose \
             turn the annotation into a changelog. Prefer present-tense current state.",
            dated_in_new
        ));
    }

    for (i, n) in new_next.iter().enumerate() {
        if next_looks_like_concrete_chunk(n) {
            let preview: String = n.chars().take(80).collect();
            out.push(format!(
                "Rule09: --next #{} reads like a concrete work unit, not relay guidance: \
                 \"{}…\". Open it as a sub-ticket now: \
                 `rs-hack board open --kind task --parent <RELAY> --next \"...\"`. \
                 Keep --next on the relay for cross-ticket strategy only.",
                i + 1,
                preview
            ));
        }
    }

    let new_next_total: usize = new_next.iter().map(|n| n.len()).sum();
    if new_next_total > NEXT_LENGTH_THRESHOLD {
        out.push(format!(
            "Rule09: --next text is {}B across {} entries — sub-ticket-shaped chunks tend \
             to bloat the relay's next list. If any of these name a specific file/test/edit, \
             they're sub-tickets.",
            new_next_total,
            new_next.len()
        ));
    }

    out
}

/// Per-ticket smells visible at read time (`board show`, `board status`).
/// Each smell is a short one-liner suitable for a `⚠ ...` bullet.
pub fn ticket_smells(t: &Ticket, board: &TicketBoard) -> Vec<String> {
    let mut out = Vec::new();

    let handoff_total: usize = t.handoff.iter().map(|h| h.len()).sum();
    let handoff_dated: usize = t.handoff.iter().map(|h| count_dated_markers(h)).sum();
    if t.handoff.len() >= 2 && handoff_total > HANDOFF_LENGTH_THRESHOLD {
        out.push(format!(
            "handoff stacked: {} entries totaling {}B (Rule03 — consider condensing to current state)",
            t.handoff.len(),
            handoff_total
        ));
    }
    if handoff_dated >= 2 {
        out.push(format!(
            "{} dated markers in handoff prose (Rule03 — events.jsonl already timestamps; prose should be present-tense)",
            handoff_dated
        ));
    }

    let concrete_next: Vec<usize> = t
        .next_steps
        .iter()
        .enumerate()
        .filter(|(_, n)| next_looks_like_concrete_chunk(n))
        .map(|(i, _)| i + 1)
        .collect();
    if !concrete_next.is_empty() {
        out.push(format!(
            "next #{} reads like a concrete work unit (Rule09 — should be a sub-ticket, not relay guidance)",
            concrete_next.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", #")
        ));
    }

    // Parent vs children state mismatch: relay sits in non-terminal status
    // while every child is review/done. Suggests Col01 isn't being checked
    // when the last child was wrapped up.
    let children: Vec<&Ticket> = board
        .tickets
        .iter()
        .filter(|c| c.parent.as_deref() == Some(t.id.as_str()) && c.id != t.id)
        .collect();
    if !children.is_empty() {
        let all_terminal = children.iter().all(|c| {
            matches!(c.status, TicketStatus::Review | TicketStatus::Done)
        });
        let parent_active = matches!(
            t.status,
            TicketStatus::InProgress | TicketStatus::Claimed | TicketStatus::Open
        );
        if all_terminal && parent_active && !t.is_epic {
            out.push(format!(
                "{}/{} children in review/done while parent is `{}` (Col01 — relay should likely be handoff or review)",
                children.len(),
                children.len(),
                t.status
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticket::ItemType;

    fn mk_ticket(id: &str) -> Ticket {
        Ticket {
            id: id.to_string(),
            title: "t".to_string(),
            item_type: ItemType::Relay,
            kind: None,
            status: TicketStatus::InProgress,
            assignee: None,
            phase: None,
            parent: None,
            severity: None,
            handoff: vec![],
            next_steps: vec![],
            cleanup: vec![],
            verify: vec![],
            gotchas: vec![],
            assumes: vec![],
            depends_on: vec![],
            see_also: vec![],
            file: Default::default(),
            line: 0,
            target: crate::annotation::AnnotationTarget::Module {
                path: String::new(),
            },
            files: vec![],
            conflicts: Default::default(),
            is_epic: false,
            epic_status: None,
        }
    }

    #[test]
    fn flags_stacked_handoff_over_threshold() {
        let mut t = mk_ticket("R001");
        t.handoff = vec!["x".repeat(300), "y".repeat(300)];
        let board = TicketBoard { tickets: vec![t.clone()] };
        let smells = ticket_smells(&t, &board);
        assert!(smells.iter().any(|s| s.contains("handoff stacked")));
    }

    #[test]
    fn flags_dated_markers() {
        let mut t = mk_ticket("R001");
        t.handoff = vec![
            "2026-04-19 pickup: did stuff".to_string(),
            "2026-04-22 pickup: did more stuff".to_string(),
        ];
        let board = TicketBoard { tickets: vec![t.clone()] };
        let smells = ticket_smells(&t, &board);
        assert!(smells.iter().any(|s| s.contains("dated markers")));
    }

    #[test]
    fn flags_concrete_next_with_alloc_verb() {
        let mut t = mk_ticket("R001");
        t.next_steps = vec![
            "Open R012-T2-B as the next free child via rs-hack board claim --kind task --parent R012".to_string(),
        ];
        let board = TicketBoard { tickets: vec![t.clone()] };
        let smells = ticket_smells(&t, &board);
        assert!(smells.iter().any(|s| s.contains("concrete work unit")));
    }

    #[test]
    fn flags_parent_active_with_terminal_children() {
        let mut parent = mk_ticket("R001");
        parent.status = TicketStatus::InProgress;
        let mut child = mk_ticket("R001-T1");
        child.parent = Some("R001".to_string());
        child.status = TicketStatus::Review;
        let board = TicketBoard { tickets: vec![parent.clone(), child] };
        let smells = ticket_smells(&parent, &board);
        assert!(smells.iter().any(|s| s.contains("Col01")));
    }

    #[test]
    fn write_time_warns_on_growing_handoff() {
        let mut existing = mk_ticket("R001");
        existing.handoff = vec!["x".repeat(400)];
        let new_handoff = vec!["y".repeat(200)];
        let warnings = write_time_warnings(Some(&existing), &new_handoff, &[]);
        assert!(warnings.iter().any(|w| w.contains("Rule03")));
    }

    #[test]
    fn write_time_warns_on_concrete_next() {
        let new_next = vec![
            "Open the next free child via rs-hack board claim --kind task --parent R012 then begin: (1) ... (2) ...".to_string(),
        ];
        let warnings = write_time_warnings(None, &[], &new_next);
        assert!(warnings.iter().any(|w| w.contains("Rule09")));
    }

    #[test]
    fn quiet_for_clean_writes() {
        let new_handoff = vec!["short snapshot of state".to_string()];
        let new_next = vec!["high-level guidance for the next picker".to_string()];
        let warnings = write_time_warnings(None, &new_handoff, &new_next);
        assert!(warnings.is_empty(), "got: {:?}", warnings);
    }
}
