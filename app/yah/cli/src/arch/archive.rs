//! @arch:layer(arch)
//! @arch:role(ticket)
//!
//! Reconstruct archived/disappeared tickets from `.yah/events/<shard>.jsonl`.
//!
//! When a ticket is archived its `@yah:` lines are stripped from source, so
//! `TicketBoard::get` returns None even though the work history is preserved
//! in the per-relay event shard. This module replays a shard for a single id
//! and returns the last-known full snapshot plus a disposition tag.

use crate::arch::ticket::Ticket;
use serde_json::Value;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// `archived` event in the shard — clean terminal state
    Archived,
    /// `disappeared` event — annotation vanished without an archive click
    Disappeared,
    /// Shard exists but has no terminal event for this id (stale snapshot)
    Stale,
}

#[derive(Debug, Clone)]
pub struct ArchivedTicket {
    pub ticket: Ticket,
    pub disposition: Disposition,
    /// Unix seconds of the terminal (or last) event
    pub last_seen: u64,
}

/// Per-relay shard name for any ticket id. Sub-tickets live in their
/// parent's shard (`R007-T1` → `R007.jsonl`); bare ids use their own.
pub fn shard_for(id: &str) -> &str {
    id.split_once('-').map(|(p, _)| p).unwrap_or(id)
}

/// Append a single JSONL event line to the per-relay shard. Creates
/// `.yah/events/` if missing. The caller chooses the shard name —
/// usually `shard_for(ticket_id)`, but a ticket with an explicit
/// `parent` should use the parent's shard.
pub fn append_event(workspace: &Path, shard: &str, event: &Value) -> std::io::Result<()> {
    let dir = workspace.join(".yah").join("events");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{shard}.jsonl"));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut line = serde_json::to_string(event).unwrap();
    line.push('\n');
    f.write_all(line.as_bytes())
}

/// Replay `.yah/events/<shard>.jsonl` to reconstruct the last-known state
/// of `id`. Returns None when the shard is missing or has no events for
/// this id.
pub fn lookup(workspace: &Path, id: &str) -> Option<ArchivedTicket> {
    let path = workspace
        .join(".yah")
        .join("events")
        .join(format!("{}.jsonl", shard_for(id)));
    let raw = std::fs::read_to_string(&path).ok()?;

    let mut state: Option<Value> = None;
    let mut disposition = Disposition::Stale;
    let mut last_seen: u64 = 0;
    let mut saw_event = false;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<Value>(line) else { continue };
        if ev.get("id").and_then(|i| i.as_str()) != Some(id) {
            continue;
        }
        saw_event = true;
        let t = ev.get("t").and_then(|n| n.as_u64()).unwrap_or(0);
        if t > last_seen {
            last_seen = t;
        }
        let kind = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match kind {
            "scan" => {
                if let Some(ticket) = ev.get("ticket") {
                    state = Some(ticket.clone());
                } else if let Some(changes) = ev.get("changes") {
                    if let Some(s) = state.as_mut() {
                        apply_changes(s, changes);
                    }
                }
            }
            "disappeared" | "archived" => {
                if let Some(ticket) = ev.get("lastTicket").or_else(|| ev.get("ticket")) {
                    state = Some(ticket.clone());
                }
                disposition = if kind == "archived" {
                    Disposition::Archived
                } else {
                    Disposition::Disappeared
                };
            }
            _ => {}
        }
    }

    if !saw_event {
        return None;
    }
    let ticket: Ticket = serde_json::from_value(state?).ok()?;
    Some(ArchivedTicket { ticket, disposition, last_seen })
}

/// Apply a `changes` delta object (`{field: {before, after}}`) to a ticket
/// JSON value in place. Unknown fields are inserted; null `after` removes.
fn apply_changes(state: &mut Value, changes: &Value) {
    let Some(obj) = state.as_object_mut() else { return };
    let Some(deltas) = changes.as_object() else { return };
    for (field, delta) in deltas {
        let after = match delta.get("after") {
            Some(v) => v.clone(),
            None => continue,
        };
        if after.is_null() {
            obj.remove(field);
        } else {
            obj.insert(field.clone(), after);
        }
    }
    // Keep `file`/`line` mirrors in sync with `files[0]` if files changed.
    if let Some(files) = obj.get("files").cloned() {
        if let Some(first) = files.as_array().and_then(|a| a.first()) {
            if let Some(p) = first.get("path").cloned() {
                obj.insert("file".into(), p);
            }
            if let Some(l) = first.get("line").cloned() {
                obj.insert("line".into(), l);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_shard(dir: &Path, name: &str, lines: &[&str]) {
        let events = dir.join(".yah").join("events");
        std::fs::create_dir_all(&events).unwrap();
        std::fs::write(events.join(format!("{name}.jsonl")), lines.join("\n")).unwrap();
    }

    #[test]
    fn shard_routing() {
        assert_eq!(shard_for("R018"), "R018");
        assert_eq!(shard_for("R018-T1"), "R018");
        assert_eq!(shard_for("T01"), "T01");
    }

    #[test]
    fn missing_shard_returns_none() {
        let tmp = std::env::temp_dir().join("yah-archive-test-missing");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(lookup(&tmp, "R999").is_none());
    }

    #[test]
    fn replays_genesis_then_changes_then_disappeared() {
        let tmp = std::env::temp_dir().join("yah-archive-test-replay");
        let _ = std::fs::remove_dir_all(&tmp);
        let genesis = r#"{"t":1,"type":"scan","id":"R001","ticket":{"id":"R001","title":"hello","item_type":"relay","status":"open","depends_on":[],"see_also":[],"file":"a.rs","line":1,"target":{"Module":{"path":"a"}},"files":[{"path":"a.rs","line":1}]}}"#;
        let delta = r#"{"t":2,"type":"scan","id":"R001","changes":{"status":{"before":"open","after":"in-progress"}}}"#;
        let bye = r#"{"t":3,"type":"disappeared","id":"R001","lastTicket":{"id":"R001","title":"hello","item_type":"relay","status":"in-progress","depends_on":[],"see_also":[],"file":"a.rs","line":1,"target":{"Module":{"path":"a"}},"files":[{"path":"a.rs","line":1}]}}"#;
        write_shard(&tmp, "R001", &[genesis, delta, bye]);
        let got = lookup(&tmp, "R001").expect("should reconstruct");
        assert_eq!(got.disposition, Disposition::Disappeared);
        assert_eq!(got.last_seen, 3);
        assert_eq!(got.ticket.id, "R001");
        assert_eq!(got.ticket.status.to_string(), "in-progress");
    }

    #[test]
    fn sub_ticket_uses_parent_shard() {
        let tmp = std::env::temp_dir().join("yah-archive-test-subtick");
        let _ = std::fs::remove_dir_all(&tmp);
        let ev = r#"{"t":1,"type":"scan","id":"R001-T1","ticket":{"id":"R001-T1","title":"sub","item_type":"ticket","status":"open","parent":"R001","depends_on":[],"see_also":[],"file":"a.rs","line":2,"target":{"Module":{"path":"a"}},"files":[{"path":"a.rs","line":2}]}}"#;
        write_shard(&tmp, "R001", &[ev]);
        let got = lookup(&tmp, "R001-T1").expect("should reconstruct");
        assert_eq!(got.ticket.id, "R001-T1");
        assert_eq!(got.disposition, Disposition::Stale);
    }
}
