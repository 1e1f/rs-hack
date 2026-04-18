//! @arch:layer(arch)
//! @arch:role(status)
//!
//! Board status summary — the "what's in flight" view for planning agents.
//!
//! Aggregates [`TicketBoard`] plus light reads of `.hack/todo.md` and
//! `.hack/events.jsonl` into a single snapshot an agent can consume in one
//! command. Deliberately different shape from `board tickets` (per-ticket
//! dump): counts, ownership, epic progress, smell signals.

use crate::ticket::{ItemType, Ticket, TicketBoard, TicketStatus};
use serde::Serialize;
use std::path::Path;

/// One-shot snapshot of the board.
#[derive(Debug, Serialize)]
pub struct BoardStatus {
    /// Count of tickets per column. Keys: "open", "active", "handoff", "review", "epic".
    pub counts: ColumnCounts,

    /// Who is holding what — tickets in `claimed` or `in-progress`. These are
    /// off-limits for refactor work per R5.
    pub active: Vec<ActiveTicket>,

    /// Handoff column — claimable batons, with the first next-step for orientation.
    pub handoffs: Vec<HandoffTicket>,

    /// Epics with live child progress.
    pub epics: Vec<EpicSummary>,

    /// Pre-ticket inbox from `.hack/todo.md`.
    pub todos: Vec<TodoSummary>,

    /// Smell: tickets that disappeared from source without an archive event.
    /// Up to 5 most-recent IDs; full log in `.hack/events.jsonl`.
    pub disappeared: Vec<String>,

    /// Total count of `disappeared` events in the log, even if later restored.
    pub disappeared_total: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct ColumnCounts {
    pub open: usize,
    pub active: usize,
    pub handoff: usize,
    pub review: usize,
    pub epic: usize,
}

#[derive(Debug, Serialize)]
pub struct ActiveTicket {
    pub id: String,
    pub title: String,
    pub assignee: Option<String>,
    pub status: String,
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct HandoffTicket {
    pub id: String,
    pub title: String,
    pub assignee: Option<String>,
    pub next: Option<String>,
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct EpicSummary {
    pub id: String,
    pub title: String,
    pub derived_status: String,
    pub children_total: usize,
    pub children_done: usize,
    pub children_active: usize,
    pub children_handoff: usize,
    pub children_open: usize,
}

#[derive(Debug, Serialize)]
pub struct TodoSummary {
    pub id: String,
    pub text_preview: String,
    pub kind: Option<String>,
    pub stage: Option<String>,
}

impl BoardStatus {
    /// Compute a status snapshot from the board and a workspace root. Reads
    /// `.hack/todo.md` and `.hack/events.jsonl` best-effort — missing files
    /// yield empty sections, not errors.
    pub fn compute(board: &TicketBoard, workspace: &Path) -> Self {
        let mut counts = ColumnCounts::default();
        let mut active = Vec::new();
        let mut handoffs = Vec::new();

        for t in &board.tickets {
            if t.is_epic {
                counts.epic += 1;
                continue;
            }
            match t.status {
                TicketStatus::Open => counts.open += 1,
                TicketStatus::Claimed | TicketStatus::InProgress => {
                    counts.active += 1;
                    active.push(ActiveTicket {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        assignee: t.assignee.clone(),
                        status: match t.status {
                            TicketStatus::Claimed => "claimed".into(),
                            TicketStatus::InProgress => "in-progress".into(),
                            _ => unreachable!(),
                        },
                        file: t.file.display().to_string(),
                        line: t.line,
                    });
                }
                TicketStatus::Handoff => {
                    counts.handoff += 1;
                    handoffs.push(HandoffTicket {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        assignee: t.assignee.clone(),
                        next: t.next_steps.first().cloned(),
                        file: t.file.display().to_string(),
                        line: t.line,
                    });
                }
                TicketStatus::Review | TicketStatus::Done => counts.review += 1,
            }
        }

        let epics = board
            .tickets
            .iter()
            .filter(|t| t.is_epic)
            .map(|epic| summarize_epic(board, epic))
            .collect();

        let todos = read_todos(workspace);
        let (disappeared, disappeared_total) = scan_disappeared(workspace);

        Self {
            counts,
            active,
            handoffs,
            epics,
            todos,
            disappeared,
            disappeared_total,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Board status\n\n");

        out.push_str(&format!(
            "**Counts** · Open {} · Active {} · Handoff {} · Review {} · Epics {}\n\n",
            self.counts.open,
            self.counts.active,
            self.counts.handoff,
            self.counts.review,
            self.counts.epic
        ));

        if !self.active.is_empty() {
            out.push_str("## Active (off-limits for refactor — R5)\n\n");
            for t in &self.active {
                let who = t.assignee.as_deref().unwrap_or("unassigned");
                out.push_str(&format!(
                    "- **{}** `{}` · {} · {} ({}:{})\n",
                    t.id, t.status, who, t.title, t.file, t.line
                ));
            }
            out.push('\n');
        }

        if !self.handoffs.is_empty() {
            out.push_str("## Handoff — claimable batons\n\n");
            for t in &self.handoffs {
                out.push_str(&format!("- **{}**: {}\n", t.id, t.title));
                if let Some(ref n) = t.next {
                    out.push_str(&format!("  next: {}\n", n));
                }
                out.push_str(&format!("  pickup: `rs-hack board tickets --prompt {}`\n", t.id));
            }
            out.push('\n');
        }

        if !self.epics.is_empty() {
            out.push_str("## Epics\n\n");
            for e in &self.epics {
                out.push_str(&format!(
                    "- **{}** ({}): {} — {}/{} done · {} active · {} handoff · {} open\n",
                    e.id,
                    e.derived_status,
                    e.title,
                    e.children_done,
                    e.children_total,
                    e.children_active,
                    e.children_handoff,
                    e.children_open
                ));
            }
            out.push('\n');
        }

        if !self.todos.is_empty() {
            out.push_str("## Todos (pre-ticket inbox)\n\n");
            for t in &self.todos {
                let kind = t.kind.as_deref().unwrap_or("—");
                let stage = t.stage.as_deref().unwrap_or("—");
                out.push_str(&format!(
                    "- **{}** [{}/{}]: {}\n",
                    t.id, kind, stage, t.text_preview
                ));
            }
            out.push('\n');
        }

        if self.disappeared_total > 0 {
            out.push_str("## Smell — disappeared tickets\n\n");
            out.push_str(&format!(
                "{} total `disappeared` events in `.hack/events.jsonl`. Most recent IDs: {}\n\n",
                self.disappeared_total,
                if self.disappeared.is_empty() {
                    "(none)".to_string()
                } else {
                    self.disappeared.join(", ")
                }
            ));
            out.push_str(
                "Investigate: a ticket removed from source without an archive event is a clobber. \
                 Restore from the event log if it was accidental.\n\n",
            );
        }

        if self.counts.open == 0
            && self.counts.active == 0
            && self.counts.handoff == 0
            && self.counts.review == 0
            && self.todos.is_empty()
        {
            out.push_str("_Board is empty._\n");
        }

        out
    }
}

fn summarize_epic(board: &TicketBoard, epic: &Ticket) -> EpicSummary {
    let children: Vec<&Ticket> = board.children_of(&epic.id);
    let mut done = 0;
    let mut active = 0;
    let mut handoff = 0;
    let mut open = 0;
    for c in &children {
        match c.status {
            TicketStatus::Review | TicketStatus::Done => done += 1,
            TicketStatus::Claimed | TicketStatus::InProgress => active += 1,
            TicketStatus::Handoff => handoff += 1,
            TicketStatus::Open => open += 1,
        }
    }
    EpicSummary {
        id: epic.id.clone(),
        title: epic.title.clone(),
        derived_status: epic.epic_status.clone().unwrap_or_else(|| "active".into()),
        children_total: children.len(),
        children_done: done,
        children_active: active,
        children_handoff: handoff,
        children_open: open,
    }
}

fn read_todos(workspace: &Path) -> Vec<TodoSummary> {
    let path = workspace.join(".hack").join("todo.md");
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    parse_todos(&content)
}

/// Minimal parser mirroring hack-board/src/server.ts::parseTodos.
/// Blocks start with `## <id>`; each following line is either `key: value`
/// (kind / stage / see) or body text.
fn parse_todos(content: &str) -> Vec<TodoSummary> {
    let mut todos = Vec::new();
    let mut current: Option<(String, Vec<String>, Option<String>, Option<String>)> = None;

    let flush = |cur: Option<(String, Vec<String>, Option<String>, Option<String>)>,
                 out: &mut Vec<TodoSummary>| {
        if let Some((id, body, kind, stage)) = cur {
            let text = body.join(" ").trim().to_string();
            let preview = preview_text(&text, 80);
            out.push(TodoSummary {
                id,
                text_preview: preview,
                kind,
                stage,
            });
        }
    };

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            flush(current.take(), &mut todos);
            current = Some((rest.trim().to_string(), Vec::new(), None, None));
            continue;
        }
        let Some(ref mut cur) = current else { continue };
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("kind:") {
            cur.2 = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("stage:") {
            cur.3 = Some(rest.trim().to_string());
        } else if trimmed.starts_with("see:") {
            // ignore for summary — counts aren't needed here
        } else if !trimmed.is_empty() {
            cur.1.push(trimmed.to_string());
        }
    }
    flush(current, &mut todos);
    todos
}

fn preview_text(s: &str, max: usize) -> String {
    let trimmed: String = s.lines().next().unwrap_or("").chars().take(max).collect();
    if s.len() > trimmed.len() || s.contains('\n') {
        format!("{}…", trimmed)
    } else {
        trimmed
    }
}

/// Returns (up-to-5 most recent disappeared IDs, total count).
///
/// Reads the per-relay event shards at `.hack/events/*.jsonl` when they
/// exist. Falls back to the legacy single-file `.hack/events.jsonl` for
/// workspaces that haven't been migrated yet — the board server's first
/// run rewrites the legacy file into shards, but this function should
/// work either way so `rs-hack board status` doesn't break during
/// migration.
fn scan_disappeared(workspace: &Path) -> (Vec<String>, usize) {
    let mut lines: Vec<String> = Vec::new();

    let shard_dir = workspace.join(".hack").join("events");
    if shard_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&shard_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    lines.extend(content.lines().map(|l| l.to_string()));
                }
            }
        }
    } else {
        let legacy = workspace.join(".hack").join("events.jsonl");
        if let Ok(content) = std::fs::read_to_string(&legacy) {
            lines.extend(content.lines().map(|l| l.to_string()));
        }
    }

    // Collect `disappeared` events with their timestamps so we can return
    // the most recent regardless of which shard they came from.
    let mut all: Vec<(u64, String)> = Vec::new();
    for line in &lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) == Some("disappeared") {
            if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                let t = v.get("t").and_then(|n| n.as_u64()).unwrap_or(0);
                all.push((t, id.to_string()));
            }
        }
    }
    let total = all.len();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    let tail: Vec<String> = all.iter().rev().take(5).map(|(_, id)| id.clone()).collect();
    (tail, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_todos_extracts_id_kind_stage_and_preview() {
        let content = "\
# Todos

## abc-1
kind: feature
stage: refine
Build the thing properly

## abc-2
kind: bug
Fix the other thing
see: reference architecture/x.md
";
        let todos = parse_todos(content);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].id, "abc-1");
        assert_eq!(todos[0].kind.as_deref(), Some("feature"));
        assert_eq!(todos[0].stage.as_deref(), Some("refine"));
        assert!(todos[0].text_preview.contains("Build the thing"));
        assert_eq!(todos[1].kind.as_deref(), Some("bug"));
        assert_eq!(todos[1].stage, None);
    }

    #[test]
    fn preview_truncates_and_adds_ellipsis() {
        let long = "a".repeat(200);
        let p = preview_text(&long, 80);
        assert_eq!(p.chars().count(), 81); // 80 + ellipsis
        assert!(p.ends_with('…'));
    }

    #[test]
    fn scan_disappeared_handles_missing_file() {
        let tmp = std::env::temp_dir().join("rs-hack-status-test-empty");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let (ids, n) = scan_disappeared(&tmp);
        assert!(ids.is_empty());
        assert_eq!(n, 0);
    }

    #[test]
    fn scan_disappeared_legacy_single_file() {
        let tmp = std::env::temp_dir().join("rs-hack-status-test-disappeared-legacy");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".hack")).unwrap();
        let log = "\
{\"t\":1,\"type\":\"created\",\"id\":\"X1\"}
{\"t\":2,\"type\":\"disappeared\",\"id\":\"A\"}
{\"t\":3,\"type\":\"archived\",\"id\":\"X1\"}
{\"t\":4,\"type\":\"disappeared\",\"id\":\"B\"}
{\"t\":5,\"type\":\"disappeared\",\"id\":\"C\"}
";
        std::fs::write(tmp.join(".hack").join("events.jsonl"), log).unwrap();
        let (ids, n) = scan_disappeared(&tmp);
        assert_eq!(n, 3);
        assert_eq!(ids, vec!["C", "B", "A"]);
    }

    #[test]
    fn scan_disappeared_sharded_layout() {
        // Events split across per-relay shards. scan_disappeared should
        // union them, sort by timestamp, and return the most recent.
        let tmp = std::env::temp_dir().join("rs-hack-status-test-disappeared-shards");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".hack").join("events")).unwrap();
        std::fs::write(
            tmp.join(".hack").join("events").join("R001.jsonl"),
            "{\"t\":2,\"type\":\"disappeared\",\"id\":\"A\"}\n\
             {\"t\":4,\"type\":\"disappeared\",\"id\":\"B\"}\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join(".hack").join("events").join("R002.jsonl"),
            "{\"t\":5,\"type\":\"disappeared\",\"id\":\"C\"}\n\
             {\"t\":1,\"type\":\"scan\",\"id\":\"R002\",\"hash\":\"abc\"}\n",
        )
        .unwrap();
        let (ids, n) = scan_disappeared(&tmp);
        assert_eq!(n, 3);
        assert_eq!(ids, vec!["C", "B", "A"]);
    }

    #[test]
    fn scan_disappeared_prefers_shards_over_legacy() {
        // If both layouts are present (unlikely in practice — migration
        // should rename the legacy file), we prefer the sharded view so
        // migrated state wins.
        let tmp = std::env::temp_dir().join("rs-hack-status-test-disappeared-both");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".hack").join("events")).unwrap();
        std::fs::write(
            tmp.join(".hack").join("events.jsonl"),
            "{\"t\":1,\"type\":\"disappeared\",\"id\":\"LEGACY\"}\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join(".hack").join("events").join("R001.jsonl"),
            "{\"t\":2,\"type\":\"disappeared\",\"id\":\"SHARD\"}\n",
        )
        .unwrap();
        let (ids, _) = scan_disappeared(&tmp);
        assert_eq!(ids, vec!["SHARD"]);
    }
}
