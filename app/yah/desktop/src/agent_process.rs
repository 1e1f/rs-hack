//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Subprocess-driven agent runner for the `claude` (PVd) preset.
//!
//! Sister to [`crate::agent`] (the HA-family runner): same Tauri-side
//! surface (`start` returns a [`StartSessionResult`], `send` streams
//! [`AgentEvent`]s through the `agent:event` channel, `stop` kills the
//! session) but the wire shape is a subprocess speaking
//! `--input-format stream-json --output-format stream-json` over its
//! stdio bus instead of `/v1/messages`.
//!
//! ## Lifecycle
//!
//! 1. Renderer calls [`crate::agent::agent_start_session`] with a
//!    `claude-cli` engine; the dispatcher in `agent.rs` lands here.
//! 2. We render the [`Prelude`] to `<rig>/.yah/CLAUDE.md` (yah-owned,
//!    regenerated per session) and idempotently inject one
//!    `@.yah/CLAUDE.md` import line at the top of root
//!    `<rig>/CLAUDE.md` (creates the root file if absent, otherwise
//!    preserves user content). Claude Code reads root `CLAUDE.md` as
//!    project memory on spawn and follows the import.
//! 3. We spawn `claude --print --input-format stream-json
//!    --output-format stream-json --include-partial-messages` with
//!    `cwd = rig_root`. Auth is delegated entirely — Claude Code
//!    handles its own OAuth / login.
//! 4. A background task reads stdout line-by-line, parses each line
//!    as `stream-json`, and maps to [`AgentEvent`]s on the
//!    `agent:event` channel.
//! 5. [`crate::agent::agent_send`] writes a `{"type":"user",...}`
//!    line to the subprocess's stdin.
//! 6. [`crate::agent::agent_stop`] closes stdin (cooperative shutdown),
//!    then escalates to `kill()` if the child hasn't exited within a
//!    grace window. `.yah/CLAUDE.md` is left in place as a session
//!    record (the next start overwrites); the root `CLAUDE.md`
//!    import line is stable across sessions and never removed.
//!
//! ## Why not `<rig>/.claude/CLAUDE.md`?
//!
//! Claude Code's documented memory locations are `<cwd>/CLAUDE.md`
//! (project) and `~/.claude/CLAUDE.md` (user) — `<cwd>/.claude/CLAUDE.md`
//! is **not** read. Generating there would have been a silent no-op.
//! The split-with-import shape (yah-owned `.yah/CLAUDE.md` + a tiny
//! root import line) lets users keep their own checked-in `CLAUDE.md`
//! content alongside ours.
//!
//! ## What's not here
//!
//! - **Rules-driven generator** — the prelude is rendered verbatim
//!   today; the eventual amalgamation of yah-defined policies (agent
//!   roles, dos/don'ts, column-driven skill blocks) lives behind the
//!   `R028-F6` / `R028-F5` work. This module is just the sink.
//! - **Tool dispatch** — depends on R028-F9 (yah-mcp). Until that
//!   lands, the subprocess uses Claude Code's own builtin tools (and
//!   any MCP servers the user has registered globally in their
//!   `~/.claude/settings.json`). When yah-mcp ships, we'll write
//!   `<rig>/.claude/settings.json` here registering it as a stdio
//!   MCP server.
//! - **Cost analytics** — Claude Code's stream-json output doesn't
//!   surface `cache_read_input_tokens` per turn, so the ring-cost
//!   gauge is approximated for PVd sessions. Tracked as a metrics gap
//!   in `.yah/arch/authored/yah-agent-runtime.md`.
//!
//! @yah:ticket(R028-F10, "Rules-driven CLAUDE.md generator: agent roles, dos/don'ts, column-driven skill blocks")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R028)
//! @yah:handoff("R028-F8 lands the sink (.yah/CLAUDE.md + idempotent root-import injection). Today the file is just prelude.render() verbatim. This ticket builds the actual amalgamation: yah board rules grow new vocabulary for agent-facing policy ('agent role', 'do', 'dont'), and the prelude assembler folds them into a structured CLAUDE.md template. Docs page describing the rule schema goes alongside (.yah/arch/authored/yah-claude-md-generator.md or similar).")
//! @yah:next("Define new @yah:rule kinds: agent-role(name, body), agent-do(text), agent-dont(text). Versioned schema so future shape changes don't silently ignore old rules.")
//! @yah:next("Extend prelude assembler (yah-kg/src/prelude.rs) with PreludeSectionKind::AgentPolicy that renders the rule set as a 'Roles' / 'Do' / 'Don't' block. Section ordering: ticket → policy → parent chain → KG slice.")
//! @yah:next("Resolution: rules attached at workspace level apply everywhere; column-tagged rules (@yah:rule(...) on a relay) scope to that relay's children. R028-F5 (skills by column) is the sister mechanism — share resolution code.")
//! @yah:next("Authoring page .yah/arch/authored/yah-claude-md-generator.md: rule schema, examples, scoping rules, how to test (yah board agent-context --ticket <ID> --format claude prints the rendered CLAUDE.md).")
//! @yah:next("Test fixture: a rig with a small policy ruleset + one ticket; snapshot-test that .yah/CLAUDE.md rendered via write_prelude_md contains the expected role/do/don't blocks in order.")
//! @arch:see(.yah/arch/authored/yah-claude-md-generator.md)
//!
//! @yah:ticket(R028-F12, "Slash command UX for wrapped claude: autocomplete, recognized-command tag, mid-turn input lock")
//! @yah:status(open)
//! @yah:assignee(agent:claude)
//! @yah:parent(R028)
//! @yah:handoff("Wrapping the claude CLI inherits its custom slash-command behaviour — typing /foo expands .claude/commands/foo.md verbatim into the prompt. Currently invisible to the user: no autocomplete, no acknowledgement that /foo resolves to a real command, and (parity bug with Claude VS Code) sending /command while a turn is in flight gets it interpreted as text instead of expanded. Scoped to PVd preset for now; the HA-family runner doesn't pass through slash commands at all.")
//! @yah:next("Tauri command list_slash_commands(rig_id) -> Vec<SlashCommand{name, source, description}> reading rig-local <rig>/.claude/commands/*.md and ~/.claude/commands/*.md. Cache per-rig; invalidate on FS watch event. Built-ins enumerated separately (only the ones that work in --print mode).")
//! @yah:next("Chat input rewrite (yah-ui/src/components/agent/ChatInput.tsx): autocomplete dropdown when input starts with '/', recognized-command badge before send, debounced fetch of available commands on session start.")
//! @yah:next("Mid-turn lock: while turn_open=true (no terminal AgentEvent yet), disable the send button + queue typed text. On TurnEnded the queued text flushes. Closes the VS Code parity bug where /command sent during a streaming turn gets treated as plain text.")
//! @yah:next("Test fixture: rig with .claude/commands/explain.md + .claude/commands/test.md; assert list_slash_commands returns both with source='rig'.")

use crate::agent::{emit_event_pub, AgentSessions, StartSessionResult, EVENT_NAME};
use crate::state::RigId;
use kg::agent::{AgentEvent, SessionId, TurnUsage};
use kg::anno::EngineRef;
use kg::prelude::Prelude;
use runner::mint_session_id;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

/// Default model when the engine doesn't pin one. Mirrors
/// [`crate::agent::DEFAULT_CLAUDE_MODEL`] — once R027 grows a
/// workspace-default-model setting, both call sites read it.
const DEFAULT_PROCESS_MODEL: &str = "claude-opus-4-7";

/// Grace window between stdin close and SIGKILL on `stop`. Claude Code
/// flushes its result on EOF; if it's still alive after this it's
/// almost certainly stuck and a kill is warranted.
const STOP_GRACE: Duration = Duration::from_millis(1500);

/// Per-session state for the subprocess runner.
///
/// `child` is held under a `Mutex` so the streaming task and the
/// stop / send commands can take turns without racing on its
/// stdin/stdout pipes. The reader task owns stdout outright (it never
/// goes back into the mutex once the spawn task pulled it out).
pub struct ProcessSession {
    pub id: SessionId,
    pub rig_id: RigId,
    pub ticket_id: String,
    pub engine: EngineRef,
    pub model: String,
    pub rig_root: PathBuf,
    /// Path to the yah-owned generated prelude file
    /// (`<rig>/.yah/CLAUDE.md`). Held for inspection / debugging; we
    /// don't remove it on stop (next session overwrites).
    pub prelude_md_path: PathBuf,
    /// Live subprocess. `None` once `stop` has been called.
    pub child: Option<Child>,
    /// Stdin handle pulled out of the child so `send` can write to
    /// the subprocess without holding the child mutex across awaits.
    pub stdin: Option<ChildStdin>,
    /// Reader task that consumes stdout and emits AgentEvents. Aborted
    /// on stop.
    pub reader: Option<JoinHandle<()>>,
}

/// Open a `claude` subprocess session for `ticket_id` on `rig_id`.
///
/// Side effects (in order):
/// 1. Writes `<rig_root>/.yah/CLAUDE.md` with the prelude and
///    idempotently injects an `@.yah/CLAUDE.md` import line into
///    root `<rig_root>/CLAUDE.md` (creates the root file if absent).
/// 2. Spawns `claude` with stream-json I/O.
/// 3. Registers the session in `AgentSessions::process_map`.
/// 4. Spawns the stdout reader task.
/// 5. Emits `AgentEvent::SessionStarted` on `agent:event`.
///
/// Failures before step 4 surface as `Err(...)`; failures after the
/// reader is live ride `AgentEvent::Error` on the stream so the
/// renderer's session pane can show them inline.
pub async fn start_process_session(
    sessions: &AgentSessions,
    app: &AppHandle,
    rig_id: RigId,
    ticket_id: String,
    engine: EngineRef,
    prelude: Prelude,
    rig_root: &Path,
) -> Result<StartSessionResult, String> {
    let session_id = mint_session_id();
    let prelude_text = prelude.render();
    let cache_key = prelude.cache.key.clone();
    let estimated_tokens = prelude.estimated_tokens;
    let ring_depth = prelude.ring_depth;
    let truncated = prelude.truncated;
    let model = engine
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_PROCESS_MODEL.to_string());

    let prelude_md_path = write_prelude_md(rig_root, &prelude_text)
        .map_err(|e| format!("CLAUDE.md write failed: {e}"))?;

    let mut child =
        spawn_claude(rig_root, &model).map_err(|e| format!("claude spawn failed: {e}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "claude subprocess produced no stdin handle".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "claude subprocess produced no stdout handle".to_string())?;

    let app_clone = app.clone();
    let rig_id_clone = rig_id.clone();
    let session_id_clone = session_id.clone();
    let reader = tauri::async_runtime::spawn(async move {
        run_reader(app_clone, rig_id_clone, session_id_clone, stdout).await;
    });

    let session = ProcessSession {
        id: session_id.clone(),
        rig_id: rig_id.clone(),
        ticket_id: ticket_id.clone(),
        engine: engine.clone(),
        model: model.clone(),
        rig_root: rig_root.to_path_buf(),
        prelude_md_path,
        child: Some(child),
        stdin: Some(stdin),
        reader: Some(reader),
    };
    sessions.insert_process(session).await;

    emit_event_pub(
        app,
        &rig_id,
        &AgentEvent::SessionStarted {
            session_id: session_id.clone(),
            ticket_id: ticket_id.clone(),
            engine: engine.as_payload(),
            cache_key: cache_key.clone(),
            estimated_tokens,
            ring_depth,
        },
    );

    Ok(StartSessionResult {
        session_id,
        ticket_id,
        engine: engine.as_payload(),
        model,
        cache_key,
        estimated_tokens,
        ring_depth,
        truncated,
    })
}

/// Send a user turn to the subprocess. Returns immediately; the
/// reader task will fan deltas out via `agent:event`.
pub async fn send_process(
    app: &AppHandle,
    session: Arc<Mutex<ProcessSession>>,
    session_id: SessionId,
    text: String,
) -> Result<(), String> {
    let line = encode_user_line(&text);
    let mut s = session.lock().await;
    let stdin = s
        .stdin
        .as_mut()
        .ok_or_else(|| "session subprocess has no live stdin (already stopped?)".to_string())?;
    if let Err(e) = stdin.write_all(line.as_bytes()).await {
        // Surface inline so the renderer can flag the failed turn
        // without the host bouncing a synchronous error back through
        // the IPC.
        let rig_id = s.rig_id.clone();
        emit_event_pub(
            app,
            &rig_id,
            &AgentEvent::Error {
                session_id,
                message: format!("write to claude stdin failed: {e}"),
            },
        );
        return Err(format!("write to claude stdin failed: {e}"));
    }
    if let Err(e) = stdin.flush().await {
        let rig_id = s.rig_id.clone();
        emit_event_pub(
            app,
            &rig_id,
            &AgentEvent::Error {
                session_id,
                message: format!("flush claude stdin failed: {e}"),
            },
        );
        return Err(format!("flush claude stdin failed: {e}"));
    }
    Ok(())
}

/// Tear down the subprocess session. Idempotent.
///
/// Sequence: drop stdin (cooperative EOF), wait `STOP_GRACE`, kill if
/// still alive. The reader task ends naturally when stdout closes.
///
/// `.yah/CLAUDE.md` is intentionally left in place — the next
/// session overwrites it, and leaving it lets the user `cat`-inspect
/// what their last agent saw. Root `CLAUDE.md`'s import line is
/// stable across sessions and never touched on stop.
pub async fn stop_process(session: Arc<Mutex<ProcessSession>>) -> Result<(), String> {
    let (mut child, reader) = {
        let mut s = session.lock().await;
        // Dropping stdin sends EOF to the subprocess; claude exits
        // cleanly after flushing the in-flight result.
        s.stdin = None;
        (s.child.take(), s.reader.take())
    };

    if let Some(child) = child.as_mut() {
        // Race the wait against the grace window — if claude exits
        // on its own we skip the kill entirely.
        let waited = tokio::time::timeout(STOP_GRACE, child.wait()).await;
        if waited.is_err() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    if let Some(reader) = reader {
        reader.abort();
    }

    Ok(())
}

/// Spawn the `claude` CLI for one session. Caller owns the returned
/// handles; we never hold them in a global.
fn spawn_claude(rig_root: &Path, model: &str) -> std::io::Result<Child> {
    let mut cmd = Command::new("claude");
    cmd.current_dir(rig_root)
        .arg("--print")
        .arg("--input-format")
        .arg("stream-json")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--include-partial-messages")
        .arg("--verbose")
        .arg("--model")
        .arg(model)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Import line we inject into root `<rig>/CLAUDE.md` so Claude Code
/// pulls in the yah-generated prelude. The path is relative to the
/// importing file (root CLAUDE.md), which Claude Code resolves to
/// `<rig_root>/.yah/CLAUDE.md`.
const PRELUDE_IMPORT_LINE: &str = "@.yah/CLAUDE.md";

/// Render `prelude_text` to `<rig_root>/.yah/CLAUDE.md` (yah-owned)
/// and ensure root `<rig_root>/CLAUDE.md` carries one
/// `@.yah/CLAUDE.md` import line at the top. Returns the path to the
/// generated yah file (the root CLAUDE.md is the user's; we don't
/// claim ownership of it beyond the one import line).
///
/// Why this shape: Claude Code reads `<cwd>/CLAUDE.md` (project
/// memory) but not `<cwd>/.claude/CLAUDE.md`. The split lets users
/// keep their own checked-in CLAUDE.md content alongside the
/// per-session generated block — yah owns `.yah/CLAUDE.md`
/// (overwritten each session, safe to .gitignore or check in), and
/// root CLAUDE.md carries one stable import line.
fn write_prelude_md(rig_root: &Path, prelude_text: &str) -> std::io::Result<PathBuf> {
    let yah_dir = rig_root.join(".yah");
    std::fs::create_dir_all(&yah_dir)?;
    let gen_path = yah_dir.join("CLAUDE.md");
    std::fs::write(&gen_path, prelude_text)?;

    let root_path = rig_root.join("CLAUDE.md");
    ensure_import_line(&root_path, PRELUDE_IMPORT_LINE)?;
    Ok(gen_path)
}

/// Idempotently ensure `path` contains a line equal to `import`.
///
/// - If the file is missing: create it with `<import>\n`.
/// - If the file already contains the exact line (whitespace-trimmed
///   match anywhere in the file): no-op.
/// - Otherwise: prepend `<import>\n\n` to the existing content,
///   preserving every byte the user authored.
///
/// Prepend (not append) because the import is load-bearing — Claude
/// Code reads top-down and the prelude should establish ground rules
/// before any project-specific instructions in the user's CLAUDE.md.
fn ensure_import_line(path: &Path, import: &str) -> std::io::Result<()> {
    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };

    if let Some(s) = existing.as_deref() {
        if s.lines().any(|l| l.trim() == import) {
            return Ok(());
        }
    }

    let new_content = match existing {
        Some(s) if !s.is_empty() => {
            let mut out = String::with_capacity(s.len() + import.len() + 2);
            out.push_str(import);
            out.push_str("\n\n");
            out.push_str(&s);
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out
        }
        _ => format!("{import}\n"),
    };
    std::fs::write(path, new_content)
}

/// Encode one user turn as a stream-json line. Trailing newline is
/// the frame terminator claude expects on `--input-format stream-json`.
fn encode_user_line(text: &str) -> String {
    let payload = serde_json::json!({
        "type": "user",
        "message": {
            "role": "user",
            "content": [{ "type": "text", "text": text }]
        },
        "parent_tool_use_id": null,
    });
    let mut line = payload.to_string();
    line.push('\n');
    line
}

/// Read stdout line-by-line and translate each frame into AgentEvents.
async fn run_reader(
    app: AppHandle,
    rig_id: RigId,
    session_id: SessionId,
    stdout: tokio::process::ChildStdout,
) {
    let mut lines = BufReader::new(stdout).lines();
    let mut turn_open = false;
    let mut accumulated = String::new();
    loop {
        let next = lines.next_line().await;
        match next {
            Ok(Some(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                    tracing::warn!(line = %line, "claude stream-json: non-JSON line; skipping");
                    continue;
                };
                let translated =
                    translate_frame(&value, &session_id, &mut turn_open, &mut accumulated);
                for ev in translated {
                    emit_event_pub(&app, &rig_id, &ev);
                }
            }
            Ok(None) => break, // stdout closed → subprocess exited.
            Err(e) => {
                tracing::warn!(error = %e, "claude stdout read failed");
                emit_event_pub(
                    &app,
                    &rig_id,
                    &AgentEvent::Error {
                        session_id: session_id.clone(),
                        message: format!("claude stdout read failed: {e}"),
                    },
                );
                break;
            }
        }
    }
    // If the subprocess vanished mid-turn (e.g. crash), surface a
    // TurnFailed so the renderer's session pane doesn't hang on
    // "thinking…".
    if turn_open {
        emit_event_pub(
            &app,
            &rig_id,
            &AgentEvent::TurnFailed {
                session_id,
                text: std::mem::take(&mut accumulated),
                message: "claude subprocess closed mid-turn".into(),
            },
        );
    }
}

/// Pure mapper from one stream-json frame to zero-or-more AgentEvents.
/// Pulled out so the wire-format translation can be unit-tested
/// without spawning a real subprocess.
///
/// `turn_open` tracks whether we've emitted `TurnStarted` for the
/// current turn; `accumulated` is the per-turn text buffer flushed
/// into `TurnEnded`. Both are caller-owned so a reader can carry
/// them across the line loop.
pub(crate) fn translate_frame(
    frame: &serde_json::Value,
    session_id: &SessionId,
    turn_open: &mut bool,
    accumulated: &mut String,
) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    let kind = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        // Subprocess init — claude announces session_id, model,
        // cwd, registered tools, etc. The renderer doesn't care
        // about most of this; we already emitted SessionStarted at
        // spawn time so we can drop the frame.
        "system" => {}
        // Streaming partial messages from `--include-partial-messages`.
        // Claude wraps Anthropic's SSE events in a `stream_event`
        // frame; the inner `event.type == "content_block_delta"` is
        // what our HA path also folds into MessageDelta.
        "stream_event" => {
            let event = frame.get("event");
            let inner_type = event
                .and_then(|e| e.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match inner_type {
                "message_start" => {
                    if !*turn_open {
                        *turn_open = true;
                        accumulated.clear();
                        out.push(AgentEvent::TurnStarted {
                            session_id: session_id.clone(),
                        });
                    }
                }
                "content_block_delta" => {
                    if let Some(text) = event
                        .and_then(|e| e.get("delta"))
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        accumulated.push_str(text);
                        out.push(AgentEvent::MessageDelta {
                            session_id: session_id.clone(),
                            text: text.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
        // Whole-message frame (emitted regardless of
        // --include-partial-messages). When partial deltas already
        // accumulated text we skip; otherwise this is our only
        // signal and we emit a single MessageDelta carrying the full
        // assistant text so the renderer renders something.
        "assistant" => {
            if accumulated.is_empty() {
                if let Some(text) = extract_assistant_text(frame) {
                    if !*turn_open {
                        *turn_open = true;
                        out.push(AgentEvent::TurnStarted {
                            session_id: session_id.clone(),
                        });
                    }
                    accumulated.push_str(&text);
                    out.push(AgentEvent::MessageDelta {
                        session_id: session_id.clone(),
                        text,
                    });
                }
            }
        }
        // Echo of the user turn we wrote to stdin, plus tool results
        // claude is feeding back to itself. We don't surface those
        // — the renderer already drew the user message when the user
        // sent it, and tool dispatch is internal to claude until
        // R028-F9 wires yah-mcp.
        "user" => {}
        // End-of-turn. `subtype` distinguishes success from various
        // failure modes; `is_error` flips us between TurnEnded and
        // TurnFailed.
        "result" => {
            let is_error = frame
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let subtype = frame.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
            let text = std::mem::take(accumulated);
            *turn_open = false;
            if is_error {
                let message = frame
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or(subtype)
                    .to_string();
                out.push(AgentEvent::TurnFailed {
                    session_id: session_id.clone(),
                    text,
                    message,
                });
            } else {
                out.push(AgentEvent::TurnEnded {
                    session_id: session_id.clone(),
                    text,
                    stop_reason: Some(subtype.to_string()),
                    usage: extract_result_usage(frame),
                });
            }
        }
        _ => {}
    }
    out
}

/// Extract token usage from a `result` frame's `usage` object.
/// `result.usage` is the same shape as Anthropic's `/v1/messages`
/// `usage`, modulo extra fields claude adds (server_tool_use,
/// service_tier) that we don't surface.
///
/// Returns `None` only when the frame has no `usage` object at all
/// — partial-usage frames (e.g. only `input_tokens` populated) yield
/// a `TurnUsage` with the missing fields as `None`.
fn extract_result_usage(frame: &serde_json::Value) -> Option<TurnUsage> {
    let usage = frame.get("usage")?;
    let read_u32 = |k: &str| usage.get(k).and_then(|v| v.as_u64()).map(|n| n as u32);
    Some(TurnUsage {
        input_tokens: read_u32("input_tokens"),
        output_tokens: read_u32("output_tokens"),
        cache_read_input_tokens: read_u32("cache_read_input_tokens"),
        cache_creation_input_tokens: read_u32("cache_creation_input_tokens"),
        // claude doesn't separately expose thinking-token totals on
        // result frames; the budget consumed gauge is approximated
        // from output_tokens + the configured think budget the host
        // already knows about.
        thinking_tokens: None,
    })
}

/// Best-effort text extraction from a whole-message `assistant` frame.
/// Matches the Anthropic content-block shape claude wraps:
/// `{"message":{"content":[{"type":"text","text":"…"}, …]}}`.
fn extract_assistant_text(frame: &serde_json::Value) -> Option<String> {
    let blocks = frame
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())?;
    let mut out = String::new();
    for block in blocks {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "text" {
            if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Wire shape for serializing the EVENT_NAME constant from this
/// module's tests if needed. Reuses the const from `agent.rs` so the
/// channel name stays single-sourced.
#[allow(dead_code)]
pub const PROCESS_EVENT_NAME: &str = EVENT_NAME;

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct DebugFrame<'a> {
    line: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sid() -> SessionId {
        SessionId::new("session:deadbeef")
    }

    #[test]
    fn translate_frame_emits_turn_started_on_message_start_stream_event() {
        let frame = json!({
            "type": "stream_event",
            "event": { "type": "message_start" },
        });
        let mut turn_open = false;
        let mut acc = String::new();
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
        assert!(turn_open);
    }

    #[test]
    fn translate_frame_folds_content_block_delta_into_message_delta() {
        let frame = json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_delta",
                "delta": { "type": "text_delta", "text": "hello" },
            },
        });
        let mut turn_open = true;
        let mut acc = String::new();
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::MessageDelta { text, .. } => assert_eq!(text, "hello"),
            other => panic!("unexpected event {other:?}"),
        }
        assert_eq!(acc, "hello");
    }

    #[test]
    fn translate_frame_result_success_emits_turn_ended_with_subtype() {
        let frame = json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
        });
        let mut turn_open = true;
        let mut acc = String::from("partial answer");
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::TurnEnded {
                text,
                stop_reason,
                usage,
                ..
            } => {
                assert_eq!(text, "partial answer");
                assert_eq!(stop_reason.as_deref(), Some("success"));
                assert!(usage.is_none(), "result frame without usage → None");
            }
            other => panic!("unexpected event {other:?}"),
        }
        assert!(!turn_open);
        assert!(acc.is_empty(), "accumulator drained on TurnEnded");
    }

    #[test]
    fn translate_frame_result_carries_usage_from_result_frame() {
        // claude's stream-json result frame carries the full usage
        // object — same shape as Anthropic's /v1/messages, with extra
        // fields we don't surface (server_tool_use, service_tier).
        // We pull the four wire-shaped numbers and ignore the rest.
        let frame = json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "usage": {
                "input_tokens": 1234,
                "output_tokens": 567,
                "cache_read_input_tokens": 1000,
                "cache_creation_input_tokens": 0,
                "server_tool_use": { "web_search_requests": 0 },
            },
        });
        let mut turn_open = true;
        let mut acc = String::from("answer");
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::TurnEnded { usage: Some(u), .. } => {
                assert_eq!(u.input_tokens, Some(1234));
                assert_eq!(u.output_tokens, Some(567));
                assert_eq!(u.cache_read_input_tokens, Some(1000));
                assert_eq!(u.cache_creation_input_tokens, Some(0));
                assert!(u.thinking_tokens.is_none());
            }
            other => panic!("expected TurnEnded with Some(usage), got {other:?}"),
        }
    }

    #[test]
    fn translate_frame_result_with_is_error_emits_turn_failed() {
        // Claude flips `is_error` for `error_max_turns`,
        // `error_during_execution`, etc. We map all of those to
        // TurnFailed so the renderer's UI doesn't have to switch
        // on subtype — the carrier event already says "this turn
        // didn't complete cleanly".
        let frame = json!({
            "type": "result",
            "subtype": "error_max_turns",
            "is_error": true,
        });
        let mut turn_open = true;
        let mut acc = String::from("got this far");
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::TurnFailed { text, message, .. } => {
                assert_eq!(text, "got this far");
                assert_eq!(message, "error_max_turns");
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn translate_frame_assistant_full_message_when_no_partial_text() {
        // Without --include-partial-messages claude only emits one
        // assistant frame per reply. We synthesize a single
        // MessageDelta so the renderer still renders something —
        // streaming users will see an instant blob, but the wire
        // contract holds.
        let frame = json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [
                    { "type": "text", "text": "non-streamed reply" },
                ],
            },
        });
        let mut turn_open = false;
        let mut acc = String::new();
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        // TurnStarted + MessageDelta.
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
        match &events[1] {
            AgentEvent::MessageDelta { text, .. } => {
                assert_eq!(text, "non-streamed reply")
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn translate_frame_skips_assistant_when_partial_already_accumulated() {
        // With --include-partial-messages the whole-message frame is
        // a duplicate of the deltas we already streamed. Skipping
        // it avoids emitting the same text twice.
        let frame = json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": "duplicate" }],
            },
        });
        let mut turn_open = true;
        let mut acc = String::from("duplicate");
        let events = translate_frame(&frame, &sid(), &mut turn_open, &mut acc);
        assert!(events.is_empty(), "no duplicate emit when acc has text");
    }

    #[test]
    fn translate_frame_drops_system_init_and_user_echo() {
        // The system frame and user echo both predate any meaningful
        // renderer-visible state; dropping them keeps the event log
        // clean and avoids confusing the UI's session list.
        let init = json!({
            "type": "system",
            "subtype": "init",
            "tools": [],
        });
        let user_echo = json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": "echo" }] },
        });
        let mut turn_open = false;
        let mut acc = String::new();
        assert!(translate_frame(&init, &sid(), &mut turn_open, &mut acc).is_empty());
        assert!(translate_frame(&user_echo, &sid(), &mut turn_open, &mut acc).is_empty());
    }

    #[test]
    fn encode_user_line_emits_stream_json_user_envelope() {
        let line = encode_user_line("hello world");
        assert!(line.ends_with('\n'), "frame must terminate with newline");
        let parsed: serde_json::Value =
            serde_json::from_str(line.trim_end()).expect("user line is valid JSON");
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"][0]["type"], "text");
        assert_eq!(parsed["message"]["content"][0]["text"], "hello world");
    }

    #[test]
    fn write_prelude_md_writes_to_yah_dir_and_creates_root_with_import() {
        // Fresh rig: no root CLAUDE.md. We should end up with
        // .yah/CLAUDE.md containing the prelude and a root CLAUDE.md
        // containing exactly the import line.
        let dir = tempfile::tempdir().unwrap();
        let gen = write_prelude_md(dir.path(), "PRELUDE\n").unwrap();
        assert!(gen.ends_with(".yah/CLAUDE.md"), "{gen:?}");
        assert_eq!(std::fs::read_to_string(&gen).unwrap(), "PRELUDE\n");

        let root = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(root, "@.yah/CLAUDE.md\n");
    }

    #[test]
    fn write_prelude_md_preserves_user_content_and_prepends_import() {
        // User has a checked-in CLAUDE.md. We must not lose any byte
        // of it; the import line goes at the top with a blank-line
        // separator so claude reads our prelude before any project
        // instructions the user wrote.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "# My Project\n\nUse 4-space indents.\n",
        )
        .unwrap();
        write_prelude_md(dir.path(), "PRELUDE\n").unwrap();

        let root = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(
            root, "@.yah/CLAUDE.md\n\n# My Project\n\nUse 4-space indents.\n",
            "import prepends with blank-line separator; user content untouched",
        );
    }

    #[test]
    fn ensure_import_line_is_idempotent() {
        // Calling write_prelude_md twice (e.g. user re-opens the
        // session) must not stack import lines. Idempotency is a
        // hard requirement — the on-disk shape is supposed to be
        // stable across sessions, only `.yah/CLAUDE.md` rotates.
        let dir = tempfile::tempdir().unwrap();
        write_prelude_md(dir.path(), "PRELUDE A\n").unwrap();
        write_prelude_md(dir.path(), "PRELUDE B\n").unwrap();

        let root = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        let import_count = root
            .lines()
            .filter(|l| l.trim() == "@.yah/CLAUDE.md")
            .count();
        assert_eq!(import_count, 1);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".yah/CLAUDE.md")).unwrap(),
            "PRELUDE B\n",
            "second call rotates the .yah file but root stays put",
        );
    }

    #[test]
    fn ensure_import_line_skips_when_already_present_anywhere() {
        // User might place the import line themselves further down
        // (e.g. inside an "imports" section). We should still
        // recognise it and skip the prepend — no double-import, no
        // shuffling user content around.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "# Header\n\n## Imports\n@.yah/CLAUDE.md\n",
        )
        .unwrap();
        write_prelude_md(dir.path(), "PRELUDE\n").unwrap();

        let root = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(root, "# Header\n\n## Imports\n@.yah/CLAUDE.md\n");
    }
}
