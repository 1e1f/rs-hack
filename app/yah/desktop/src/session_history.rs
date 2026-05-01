//! @arch:layer(tauri-cmd)
//! @arch:role(bridge)
//!
//! Session history surface — list past chat sessions on disk + (re)index
//! a sidecar of synthesized metadata on user click.
//!
//! Each session lives at `<rig>/.yah/sessions/<id>.jsonl` (written by the
//! daemon's session sink). This module pairs each jsonl with a sidecar
//! `<id>.meta.json` containing a synthesized `{ title, summary, tags,
//! ticketId, engine, turnCount, … }` record. The sidecar is *user-click
//! generated* — never auto. Staleness is `meta.indexedAt < jsonl.mtime`.
//!
//! ## Indexer
//!
//! Today's indexer is a no-LLM stub: title from the first user message,
//! summary from turn / tool-call counts. It exists so the rail UI has
//! something real to show before we wire a cheap LLM call. The seam is
//! `synthesize_meta_stub` — a pure fn over `JsonlSummary`. Layering an
//! LLM call means swapping that one call site, leaving the IO + sidecar
//! plumbing untouched.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::{AppState, RigId};

/// Sidecar shape — `<rig>/.yah/sessions/<id>.meta.json`. camelCase on
/// the wire so the renderer types match without remapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub indexed_at: u64,
    pub turn_count: u32,
    pub tool_call_count: u32,
    pub tool_fail_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_event_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<u64>,
}

/// One row returned by [`agent_session_history_list`]. The renderer
/// rolls these up however it wants (by ticket, by recency, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryRow {
    pub session_id: String,
    pub jsonl_mtime: u64,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<SessionMeta>,
    pub stale: bool,
}

/// Pure summary of a jsonl — derived facts the indexer feeds into a
/// `SessionMeta`. Public so a future LLM-backed indexer can consume it
/// without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct JsonlSummary {
    pub ticket_id: Option<String>,
    pub engine: Option<String>,
    pub first_user_message: Option<String>,
    pub turn_count: u32,
    pub tool_call_count: u32,
    pub tool_fail_count: u32,
    pub first_event_at: Option<u64>,
    pub last_event_at: Option<u64>,
}

#[tauri::command]
pub async fn agent_session_history_list(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<Vec<SessionHistoryRow>, String> {
    let rig_root = rig_root(&state, &rig_id).await?;
    let dir = rig_root.join(".yah").join("sessions");
    let mut rows: Vec<SessionHistoryRow> = Vec::new();
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(rows),
        Err(e) => return Err(format!("read .yah/sessions: {e}")),
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Some(stem) = jsonl_stem(&path) else {
            continue;
        };
        let session_id = stem.to_string();
        let mtime = file_mtime_ms(&path).unwrap_or(0);
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let meta = read_meta(&dir, &session_id);
        let stale = match &meta {
            Some(m) => m.indexed_at < mtime,
            None => true,
        };
        rows.push(SessionHistoryRow {
            session_id,
            jsonl_mtime: mtime,
            bytes,
            meta,
            stale,
        });
    }
    rows.sort_by(|a, b| b.jsonl_mtime.cmp(&a.jsonl_mtime));
    Ok(rows)
}

#[tauri::command]
pub async fn agent_session_history_reindex(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    session_id: String,
) -> Result<SessionMeta, String> {
    let rig_root = rig_root(&state, &rig_id).await?;
    let dir = rig_root.join(".yah").join("sessions");
    let jsonl = dir.join(format!("{session_id}.jsonl"));
    if !jsonl.exists() {
        return Err(format!("no jsonl for session {session_id}"));
    }
    let summary = scan_jsonl(&jsonl).map_err(|e| format!("scan {session_id}: {e}"))?;
    let meta = synthesize_meta_stub(&summary);
    write_meta(&dir, &session_id, &meta)?;
    Ok(meta)
}

/// Pure: derive a `SessionMeta` from a `JsonlSummary` without an LLM.
/// First user message becomes the title (truncated); summary is a
/// turns/tools/duration line. The seam an LLM-backed indexer replaces.
pub fn synthesize_meta_stub(s: &JsonlSummary) -> SessionMeta {
    let title = match s.first_user_message.as_deref() {
        Some(text) => truncate_one_line(text, 72),
        None => "(empty session)".to_string(),
    };
    let mut summary = String::new();
    summary.push_str(&format!("{} turns", s.turn_count));
    if s.tool_call_count > 0 {
        summary.push_str(&format!(" · {} tool calls", s.tool_call_count));
        if s.tool_fail_count > 0 {
            summary.push_str(&format!(" ({} failed)", s.tool_fail_count));
        }
    }
    if let (Some(first), Some(last)) = (s.first_event_at, s.last_event_at) {
        if last > first {
            summary.push_str(&format!(" · {}", human_duration_ms(last - first)));
        }
    }
    SessionMeta {
        title,
        summary,
        tags: Vec::new(),
        ticket_id: s.ticket_id.clone().filter(|t| t != "chat"),
        engine: s.engine.clone(),
        indexed_at: now_ms(),
        turn_count: s.turn_count,
        tool_call_count: s.tool_call_count,
        tool_fail_count: s.tool_fail_count,
        first_event_at: s.first_event_at,
        last_event_at: s.last_event_at,
    }
}

fn scan_jsonl(path: &Path) -> std::io::Result<JsonlSummary> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut s = JsonlSummary::default();
    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let t = v.get("t").and_then(|t| t.as_u64());
        if let Some(ts) = t {
            if s.first_event_at.is_none() {
                s.first_event_at = Some(ts);
            }
            s.last_event_at = Some(ts);
        }
        match kind {
            "session_started" => {
                s.ticket_id = v
                    .get("ticketId")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                s.engine = v.get("engine").and_then(|x| x.as_str()).map(str::to_string);
            }
            "user" => {
                s.turn_count += 1;
                if s.first_user_message.is_none() {
                    s.first_user_message = v
                        .get("content")
                        .and_then(|x| x.as_str())
                        .map(str::to_string);
                }
            }
            "tool_call" => s.tool_call_count += 1,
            "tool_result" => {
                if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
                    s.tool_fail_count += 1;
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

async fn rig_root(state: &AppState, rig_id: &RigId) -> Result<PathBuf, String> {
    state
        .path_for(rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))
}

fn jsonl_stem(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    name.strip_suffix(".jsonl")
}

fn meta_path(dir: &Path, session_id: &str) -> PathBuf {
    dir.join(format!("{session_id}.meta.json"))
}

fn read_meta(dir: &Path, session_id: &str) -> Option<SessionMeta> {
    let path = meta_path(dir, session_id);
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_meta(dir: &Path, session_id: &str, meta: &SessionMeta) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("mkdir sessions dir: {e}"))?;
    let path = meta_path(dir, session_id);
    let bytes = serde_json::to_vec_pretty(meta).map_err(|e| format!("encode meta: {e}"))?;
    fs::write(&path, bytes).map_err(|e| format!("write meta: {e}"))
}

fn file_mtime_ms(path: &Path) -> Option<u64> {
    let m = fs::metadata(path).ok()?;
    let t = m.modified().ok()?;
    let d = t.duration_since(UNIX_EPOCH).ok()?;
    Some(d.as_millis() as u64)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate_one_line(s: &str, max_chars: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max_chars - 1).collect();
    format!("{cut}…")
}

fn human_duration_ms(ms: u64) -> String {
    let s = ms / 1000;
    if s < 60 {
        return format!("{s}s");
    }
    let m = s / 60;
    if m < 60 {
        return format!("{m}m");
    }
    let h = m / 60;
    let rem_m = m % 60;
    if rem_m == 0 {
        format!("{h}h")
    } else {
        format!("{h}h{rem_m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        path
    }

    #[test]
    fn scan_extracts_first_user_msg_and_counts() {
        let tmp = TempDir::new().unwrap();
        let path = write_jsonl(
            tmp.path(),
            "session:abcd1234.jsonl",
            &[
                r#"{"kind":"session_started","sessionId":"session:abcd1234","ticketId":"R031","engine":"claude:opus-4-7","t":1700000000000}"#,
                r#"{"kind":"user","content":"Find the architecture markdown.","t":1700000010000}"#,
                r#"{"kind":"tool_call","toolName":"list_dir","toolCallId":"c1","t":1700000011000}"#,
                r#"{"kind":"tool_result","toolCallId":"c1","ok":true,"t":1700000011500}"#,
                r#"{"kind":"tool_call","toolName":"read_arch_doc","toolCallId":"c2","t":1700000012000}"#,
                r#"{"kind":"tool_result","toolCallId":"c2","ok":false,"t":1700000012500}"#,
                r#"{"kind":"user","content":"Try again.","t":1700000020000}"#,
            ],
        );
        let s = scan_jsonl(&path).unwrap();
        assert_eq!(s.ticket_id.as_deref(), Some("R031"));
        assert_eq!(s.engine.as_deref(), Some("claude:opus-4-7"));
        assert_eq!(s.turn_count, 2);
        assert_eq!(s.tool_call_count, 2);
        assert_eq!(s.tool_fail_count, 1);
        assert_eq!(
            s.first_user_message.as_deref(),
            Some("Find the architecture markdown.")
        );
        assert_eq!(s.first_event_at, Some(1700000000000));
        assert_eq!(s.last_event_at, Some(1700000020000));
    }

    #[test]
    fn synthesize_meta_stub_renders_title_and_summary() {
        let s = JsonlSummary {
            ticket_id: Some("R031".into()),
            engine: Some("openai:qwen2.5".into()),
            first_user_message: Some("Find the architecture markdown.".into()),
            turn_count: 2,
            tool_call_count: 6,
            tool_fail_count: 2,
            first_event_at: Some(1_700_000_000_000),
            last_event_at: Some(1_700_000_180_000),
        };
        let meta = synthesize_meta_stub(&s);
        assert_eq!(meta.title, "Find the architecture markdown.");
        assert_eq!(meta.summary, "2 turns · 6 tool calls (2 failed) · 3m");
        assert_eq!(meta.ticket_id.as_deref(), Some("R031"));
        assert_eq!(meta.engine.as_deref(), Some("openai:qwen2.5"));
        assert!(meta.tags.is_empty());
    }

    #[test]
    fn synthesize_meta_stub_drops_chat_pseudo_ticket() {
        let s = JsonlSummary {
            ticket_id: Some("chat".into()),
            first_user_message: Some("hi".into()),
            turn_count: 1,
            ..Default::default()
        };
        let meta = synthesize_meta_stub(&s);
        assert!(meta.ticket_id.is_none());
    }

    #[test]
    fn truncate_one_line_collapses_whitespace_and_ellipsizes() {
        assert_eq!(truncate_one_line("hello   world", 72), "hello world");
        let long = "x".repeat(100);
        let out = truncate_one_line(&long, 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn meta_round_trips_through_disk() {
        let tmp = TempDir::new().unwrap();
        let meta = SessionMeta {
            title: "t".into(),
            summary: "s".into(),
            tags: vec!["auth".into()],
            ticket_id: Some("R031".into()),
            engine: Some("claude".into()),
            indexed_at: 42,
            turn_count: 1,
            tool_call_count: 0,
            tool_fail_count: 0,
            first_event_at: None,
            last_event_at: None,
        };
        write_meta(tmp.path(), "session:zzz", &meta).unwrap();
        let read = read_meta(tmp.path(), "session:zzz").unwrap();
        assert_eq!(read, meta);
    }
}
