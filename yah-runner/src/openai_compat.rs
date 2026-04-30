//! OpenAI-compatible runner.
//!
//! Covers every backend that speaks `POST /v1/chat/completions` with
//! `stream: true` SSE: OpenAI proper, Ollama (local and Pro), Qwen,
//! vLLM, LiteLLM, and any host running an OpenAI-shape gateway. The
//! single implementation pressure-tests the [`Runner`] contract — if
//! the trait holds up here and against the existing Claude path in
//! `app/tauri/src/agent.rs`, it will hold up against the next provider.
//!
//! ## SSE schema we fold
//!
//! Each frame has `data: <json>\n\n` framing. Payloads we recognise:
//!
//! - `{"choices":[{"delta":{"content":"…"},"finish_reason":null}]}` →
//!   yield [`AgentEvent::MessageDelta`].
//! - `{"choices":[{"finish_reason":"stop"}]}` → record the stop reason
//!   for the eventual [`AgentEvent::TurnEnded`].
//! - `{"error":{"message":"…"}}` → yield [`AgentEvent::Error`] and end
//!   the stream.
//! - `data: [DONE]` → end-of-stream sentinel.
//!
//! Any frame that doesn't parse is ignored (matches Anthropic SSE
//! behaviour in the Claude reference impl — never abort a turn over a
//! single weird chunk).
//!
//! ## What's intentionally not here
//!
//! - **Anthropic tool_use** — structurally similar to OpenAI's shape
//!   but uses content-block `tool_use` / `tool_result` blocks instead
//!   of `tools[]` + `tool_calls[]`. Lives in `app/tauri/src/agent.rs`
//!   under the Claude track (R028) and is wired up there once the
//!   native loop ships.
//! - **Parallel sampling** — `n > 1` would require widening the event
//!   shape; we hard-code `n: 1` since that's the only mode the agent
//!   pane drives today.
//! - **Per-session iteration cap** — [`MAX_TOOL_ITERATIONS`] is a const
//!   today. Surfacing it as a per-session knob is a polish follow-up.
//!
//! @yah:ticket(R031-F2, "OpenAI/Ollama function-calling: tools[] in request + tool_calls SSE deltas + runner loop")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R031)
//! @yah:handoff("OpenAI/Ollama tool-calling round-trips end-to-end. Request body grows tools[] from registry schemas; SSE parser extracts choices[].delta.tool_calls[] (id+name+arguments accrete by index across chunks); on finish_reason='tool_calls' the runner appends the assistant tool_calls row to history, dispatches each call through the registry, appends role:tool rows keyed by tool_call_id, and re-issues. Stops on finish_reason='stop' or hits the 8-iteration cap. InnerSession.history switched to Vec<Value> so role:tool rows survive the round-trip.")
//! @yah:verify("cargo test -p yah-runner")
//! @yah:verify("cargo test -p yah-runner --test openai_compat_e2e tool_call_loop_dispatches_then_re_issues_for_final_text")
//! @yah:verify("cargo test -p yah-tauri --lib agent_tools")
//! @yah:assumes("Real Ollama / OpenAI servers stream `delta.tool_calls[].function.arguments` exactly as documented (string fragments that concatenate to valid JSON). The two e2e tests cover the documented shape; the first live round-trip against an actual provider may surface edge cases — empty-args calls, parallel calls with non-monotonic indices, providers that emit `arguments: null` instead of an empty string.")
//! @yah:next("R031-F3 (P3): UI rendering — replace stub tool_call/tool_result switch cases in useChatSession to push assistant tool_use frames + tool result frames keyed by tool_call_id. Existing ToolFrame already renders read/grep/edit; add KG-tool body rendering (arch_node, arch_subgraph) on top.")
//! @arch:see(architecture/agent-tool-calls.md)

use async_stream::stream;
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use yah_kg::agent::{AgentEvent, SessionId};
use yah_kg::prelude::Prelude;

use crate::runner::{EventStream, Runner, RunnerError};
use crate::session_id::mint_session_id;
use crate::tool::ToolRegistry;

/// Default per-turn output cap forwarded as `max_tokens`. Independent
/// of the prelude's ring budget — that bounds *input*; this bounds
/// *output*.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

/// Cap on the cumulative SSE body to keep a misbehaving upstream from
/// filling the host's working set.
const MAX_STREAM_BYTES: usize = 32 * 1024 * 1024; // 32 MB

/// Hard cap on the number of tool-call iterations within a single
/// `send`. Each iteration is one round-trip: provider asks for tools →
/// host dispatches → results sent back → provider replies. A
/// well-behaved agent settles in 1-3 rounds; the cap exists to stop a
/// model that loops on `read_file → grep → read_file …` forever from
/// burning unbounded tokens. Surface as `AgentEvent::Error` when hit.
const MAX_TOOL_ITERATIONS: u32 = 8;

/// Static configuration for one provider deployment.
///
/// The provider label is the `engine.provider` string the prelude
/// carries (`@yah:engine(openai:gpt-5)` → `provider = "openai"`). A
/// runner refuses sessions whose engine doesn't match its label, so a
/// host running both OpenAI and Ollama can register two runners and
/// dispatch by label without checking model strings.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// Engine-tag this runner serves (`"openai"`, `"ollama"`, `"qwen"`,
    /// …). Compared verbatim against the ticket's `@yah:engine(...)`
    /// provider field.
    pub provider_label: String,
    /// Full URL to `POST` against — typically `…/v1/chat/completions`.
    /// Includes path so a host can point at any OpenAI-compat gateway
    /// (LiteLLM, custom proxies) without the runner inventing routes.
    pub endpoint: String,
    /// Model used when the ticket's `@yah:engine(...)` doesn't carry
    /// an explicit model. Matches the host's workspace-default
    /// affordance.
    pub default_model: String,
    /// `Authorization: Bearer …` token. `None` is fine for
    /// unauthenticated local Ollama; required for hosted services.
    pub api_key: Option<String>,
    /// Per-turn output token cap. Falls back to
    /// [`DEFAULT_MAX_OUTPUT_TOKENS`] if unset.
    pub max_output_tokens: Option<u32>,
}

impl OpenAiCompatConfig {
    /// OpenAI defaults: Chat Completions endpoint + bearer auth.
    /// Pick a model the user is entitled to call — leaving the default
    /// at `gpt-4o` keeps the non-Plus path working until the ticket
    /// pins a specific model.
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider_label: "openai".into(),
            endpoint: "https://api.openai.com/v1/chat/completions".into(),
            default_model: "gpt-4o".into(),
            api_key: Some(api_key.into()),
            max_output_tokens: None,
        }
    }

    /// Local Ollama defaults: OpenAI-compat endpoint on the standard
    /// port, no auth (`ollama serve`'s default deployment is open on
    /// loopback).
    pub fn ollama_local() -> Self {
        Self {
            provider_label: "ollama".into(),
            endpoint: "http://localhost:11434/v1/chat/completions".into(),
            default_model: "llama3.3".into(),
            api_key: None,
            max_output_tokens: None,
        }
    }

    /// Hosted Ollama (Cloud / Pro). Requires an API key. Endpoint
    /// matches the OpenAI-compat path Ollama Cloud documents; override
    /// via `OpenAiCompatConfig` directly if it shifts.
    ///
    /// Default model is `gpt-oss:20b` — a small free-tier cloud model
    /// at the time of writing. The exact catalogue shifts, so the host
    /// (`app/tauri/src/agent.rs`) lets the user override per-session
    /// from the chat picker. See https://docs.ollama.com/cloud for the
    /// current list.
    pub fn ollama_cloud(api_key: impl Into<String>) -> Self {
        Self {
            provider_label: "ollama".into(),
            endpoint: "https://ollama.com/v1/chat/completions".into(),
            default_model: "gpt-oss:20b".into(),
            api_key: Some(api_key.into()),
            max_output_tokens: None,
        }
    }

    fn max_output_tokens(&self) -> u32 {
        self.max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
    }
}

/// In-memory per-session state. The runner keeps history server-side
/// so the host doesn't have to (and so the next turn's body builder
/// has the canonical view).
///
/// `history` carries pre-shaped OpenAI Chat Completions messages
/// (`{role, content}`, plus `tool_calls[]` on assistant turns and
/// `tool_call_id` on `role: "tool"` rows). Storing the protocol shape
/// rather than the cross-runner [`yah_kg::agent::Message`] lets a turn
/// re-issue with tool-result rows interleaved without translating
/// through a lower-fidelity intermediate.
struct InnerSession {
    /// Ticket the session was opened for. Currently unused; kept for
    /// the eventual list_sessions / cost-attribution affordances.
    #[allow(dead_code)]
    ticket_id: String,
    prelude_text: String,
    model: String,
    history: Vec<Value>,
}

/// Concrete [`Runner`] for OpenAI-compatible providers.
///
/// Cheap to clone (an `Arc` internally) so a host can hand the same
/// runner to the Tauri command surface and to a session-listing query
/// at the same time.
#[derive(Clone)]
pub struct OpenAiCompatRunner {
    inner: Arc<Inner>,
}

struct Inner {
    config: OpenAiCompatConfig,
    client: reqwest::Client,
    sessions: RwLock<HashMap<SessionId, Arc<Mutex<InnerSession>>>>,
    /// Optional tool registry. P1 stores it; P2 will use the schemas
    /// to populate the request body's `tools[]` array and dispatch
    /// `tool_calls[]` deltas through `execute`. Until P2 lands, the
    /// runner ignores it — the renderer side and the agent code path
    /// in `app/tauri/src/agent.rs` can construct the registry today
    /// without changing call sites once the loop turns on.
    tools: Option<Arc<dyn ToolRegistry>>,
}

impl OpenAiCompatRunner {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                client: reqwest::Client::new(),
                sessions: RwLock::new(HashMap::new()),
                tools: None,
            }),
        }
    }

    /// Construct a runner with a tool registry attached. Hosts that
    /// already built a registry for the rig hand it in here; sessions
    /// started against this runner will eventually carry the tools[]
    /// array in every provider request (P2). Until then the registry
    /// is held but not exercised — adding it now keeps callers from
    /// having to re-thread the construction site when P2 lands.
    pub fn with_tools(config: OpenAiCompatConfig, tools: Arc<dyn ToolRegistry>) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                client: reqwest::Client::new(),
                sessions: RwLock::new(HashMap::new()),
                tools: Some(tools),
            }),
        }
    }

    /// Provider label this runner answers to. Hosts use this to route
    /// `start` calls; useful when registering more than one runner in
    /// one process.
    pub fn provider_label(&self) -> &str {
        &self.inner.config.provider_label
    }

    /// Borrow the attached tool registry, if any. Hosts that need to
    /// inspect the registry (e.g. listing schemas in a settings UI)
    /// can read through here without round-tripping a turn.
    pub fn tools(&self) -> Option<&Arc<dyn ToolRegistry>> {
        self.inner.tools.as_ref()
    }
}

/// Fetch the OpenAI-compat `/v1/models` catalogue for a given chat
/// endpoint. Lifts `…/chat/completions` to `…/models` and parses the
/// `data[].id` array — the same shape OpenAI, Ollama (local + Cloud),
/// vLLM, and most gateways serve.
///
/// Returns model ids sorted alphabetically. Empty `Ok(vec![])` when
/// the endpoint replies but the catalogue is empty (legitimate state
/// for a fresh local Ollama with no models pulled). Errors propagate
/// as `RunnerError::Transport` so the host's renderer can surface a
/// clear "couldn't list models" message.
pub async fn list_openai_compat_models(
    chat_endpoint: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, RunnerError> {
    let models_url = derive_models_url(chat_endpoint);
    let client = reqwest::Client::new();
    let mut req = client.get(&models_url);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| RunnerError::Transport(format!("models request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(RunnerError::Transport(format!(
            "models endpoint returned {}: {}",
            status,
            body.chars().take(256).collect::<String>(),
        )));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| RunnerError::Transport(format!("models response parse failed: {e}")))?;
    let mut out: Vec<String> = json
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    Ok(out)
}

/// Replace the trailing `/chat/completions` path with `/models`. Falls
/// back to appending `/models` if the path doesn't match — keeps any
/// gateway that serves both at the same base happy.
fn derive_models_url(chat_endpoint: &str) -> String {
    if let Some(stripped) = chat_endpoint.strip_suffix("/chat/completions") {
        format!("{stripped}/models")
    } else if let Some(stripped) = chat_endpoint.strip_suffix('/') {
        format!("{stripped}/models")
    } else {
        format!("{chat_endpoint}/models")
    }
}

#[async_trait]
impl Runner for OpenAiCompatRunner {
    async fn start(
        &self,
        prelude: Prelude,
        ticket_id: &str,
    ) -> Result<SessionId, RunnerError> {
        if let Some(engine) = prelude.engine.as_ref() {
            if engine.provider != self.inner.config.provider_label {
                return Err(RunnerError::UnsupportedEngine {
                    engine: engine.as_payload(),
                });
            }
        }
        let model = prelude
            .engine
            .as_ref()
            .and_then(|e| e.model.clone())
            .unwrap_or_else(|| self.inner.config.default_model.clone());

        let id = mint_session_id();
        let session = InnerSession {
            ticket_id: ticket_id.to_string(),
            prelude_text: prelude.render(),
            model,
            history: Vec::new(),
        };
        self.inner
            .sessions
            .write()
            .await
            .insert(id.clone(), Arc::new(Mutex::new(session)));
        Ok(id)
    }

    async fn send(
        &self,
        session_id: &SessionId,
        message: String,
    ) -> Result<EventStream, RunnerError> {
        let session = self
            .inner
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| RunnerError::SessionNotFound(session_id.as_str().to_string()))?;

        // Append the user turn so the upcoming request body picks it up.
        // History is now provider-shaped JSON; we push raw `{role, content}`
        // rows here and (further below, inside the stream) push the
        // assistant rows + tool-result rows as they are produced.
        {
            let mut s = session.lock().await;
            s.history.push(json!({
                "role": "user",
                "content": message,
            }));
        }

        // Snapshot what the stream needs so the borrow on `&self` ends
        // before the stream lives across iteration boundaries.
        let client = self.inner.client.clone();
        let endpoint = self.inner.config.endpoint.clone();
        let api_key = self.inner.config.api_key.clone();
        let provider_label = self.inner.config.provider_label.clone();
        let max_output_tokens = self.inner.config.max_output_tokens();
        let tools = self.inner.tools.clone();
        let tools_array: Option<Vec<Value>> = tools_array_payload(tools.as_ref());
        let session_id_owned = session_id.clone();
        let session_arc = Arc::clone(&session);

        // Issue the first request synchronously so transport / 4xx
        // failures surface as `RunnerError::Transport` from `send`
        // rather than as a mid-stream `AgentEvent::Error`. Subsequent
        // re-issues for tool-call iterations happen inside the stream
        // (where the only way to signal failure is in-stream), since by
        // then the host has already committed to a streaming pane.
        let initial_resp = issue_chat_completions(
            &client,
            &endpoint,
            api_key.as_deref(),
            &provider_label,
            {
                let s = session_arc.lock().await;
                build_request_body(&s, max_output_tokens, tools_array.as_deref())
            },
        )
        .await?;
        let initial_bytes_stream = initial_resp.bytes_stream();

        let stream = stream! {
            yield AgentEvent::TurnStarted { session_id: session_id_owned.clone() };

            let session_id = session_id_owned;
            let mut iterations: u32 = 0;
            // `final_text` is the assistant text from the *last* iteration —
            // the one that ended with finish_reason: "stop". Intermediate
            // iterations (finish_reason: "tool_calls") contribute their text
            // to the live MessageDelta stream and to the assistant message
            // row in history, but TurnEnded.text mirrors the Claude path
            // and carries only the final reply. The initial values get
            // overwritten on the break path; they exist so the values are
            // typed in scope without `Option<…>` ceremony.
            #[allow(unused_assignments)]
            let mut final_text = String::new();
            #[allow(unused_assignments)]
            let mut last_stop_reason: Option<String> = None;

            // First iteration uses the pre-fetched bytes_stream; later
            // iterations issue fresh requests inside the loop and surface
            // any failure as `AgentEvent::Error` (the stream is already
            // live so a synchronous error can't reach the caller).
            let mut pending_bytes_stream = Some(initial_bytes_stream);

            'turn: loop {
                let mut bytes_stream = match pending_bytes_stream.take() {
                    Some(b) => b,
                    None => {
                        let body = {
                            let s = session_arc.lock().await;
                            build_request_body(&s, max_output_tokens, tools_array.as_deref())
                        };
                        match issue_chat_completions(
                            &client,
                            &endpoint,
                            api_key.as_deref(),
                            &provider_label,
                            body,
                        )
                        .await
                        {
                            Ok(resp) => resp.bytes_stream(),
                            Err(e) => {
                                // Pre-stream failure on a tool-loop
                                // re-issue. No text accumulated yet for
                                // this iteration; earlier iterations'
                                // text was already streamed to the
                                // renderer.
                                yield AgentEvent::TurnFailed {
                                    session_id: session_id.clone(),
                                    text: String::new(),
                                    message: format!("{e}"),
                                };
                                return;
                            }
                        }
                    }
                };
                let mut iter_text = String::new();
                let mut iter_finish: Option<String> = None;
                let mut tool_call_accs: BTreeMap<u32, ToolCallAcc> = BTreeMap::new();
                let mut total_bytes: usize = 0;
                let mut buffer = String::new();

                'sse: while let Some(chunk) = bytes_stream.next().await {
                    let bytes = match chunk {
                        Ok(b) => b,
                        Err(e) => {
                            yield AgentEvent::TurnFailed {
                                session_id: session_id.clone(),
                                text: iter_text.clone(),
                                message: format!("stream read failed: {e}"),
                            };
                            return;
                        }
                    };
                    total_bytes = total_bytes.saturating_add(bytes.len());
                    if total_bytes > MAX_STREAM_BYTES {
                        yield AgentEvent::TurnFailed {
                            session_id: session_id.clone(),
                            text: iter_text.clone(),
                            message: format!(
                                "stream exceeded soft cap of {} bytes — aborting",
                                MAX_STREAM_BYTES
                            ),
                        };
                        return;
                    }
                    let text = match std::str::from_utf8(&bytes) {
                        Ok(s) => s,
                        Err(e) => {
                            yield AgentEvent::TurnFailed {
                                session_id: session_id.clone(),
                                text: iter_text.clone(),
                                message: format!("non-utf8 chunk: {e}"),
                            };
                            return;
                        }
                    };
                    buffer.push_str(text);

                    while let Some(idx) = buffer.find("\n\n") {
                        let frame = buffer[..idx].to_string();
                        buffer.drain(..idx + 2);
                        for parsed in parse_sse_frame(&frame) {
                            match parsed {
                                FrameEvent::Delta { text, tool_calls, finish } => {
                                    if !text.is_empty() {
                                        iter_text.push_str(&text);
                                        yield AgentEvent::MessageDelta {
                                            session_id: session_id.clone(),
                                            text,
                                        };
                                    }
                                    for tcd in tool_calls {
                                        let acc = tool_call_accs
                                            .entry(tcd.index)
                                            .or_default();
                                        if let Some(id) = tcd.id {
                                            if !id.is_empty() {
                                                acc.id = id;
                                            }
                                        }
                                        if let Some(name) = tcd.name {
                                            if !name.is_empty() {
                                                acc.name = name;
                                            }
                                        }
                                        if let Some(args_chunk) = tcd.arguments {
                                            acc.arguments_buf.push_str(&args_chunk);
                                        }
                                    }
                                    if finish.is_some() {
                                        iter_finish = finish;
                                    }
                                }
                                FrameEvent::Error(msg) => {
                                    yield AgentEvent::TurnFailed {
                                        session_id: session_id.clone(),
                                        text: iter_text.clone(),
                                        message: msg,
                                    };
                                    return;
                                }
                                FrameEvent::Done => break 'sse,
                            }
                        }
                    }
                }

                last_stop_reason = iter_finish.clone();
                let calls: Vec<ToolCallFinalized> = tool_call_accs
                    .into_iter()
                    .map(|(_, acc)| acc.finalize())
                    .filter(|c| !c.id.is_empty() || !c.name.is_empty())
                    .collect();

                let go_again = matches!(iter_finish.as_deref(), Some("tool_calls"))
                    || (!calls.is_empty() && iter_finish.is_none());

                if go_again {
                    // Append the assistant turn carrying the tool_calls
                    // exactly as the provider produced them — the next
                    // request body must echo `tool_calls[]` so role:tool
                    // rows can be paired by `tool_call_id`.
                    let assistant_tool_calls: Vec<Value> = calls
                        .iter()
                        .map(|c| json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": c.arguments_str,
                            }
                        }))
                        .collect();
                    {
                        let mut s = session_arc.lock().await;
                        s.history.push(json!({
                            "role": "assistant",
                            "content": iter_text,
                            "tool_calls": assistant_tool_calls,
                        }));
                    }

                    let registry = match tools.as_ref() {
                        Some(r) => r,
                        None => {
                            // Defensive: we never advertised tools[] in the
                            // request body, so the model shouldn't have
                            // produced tool_calls. Treat as a protocol
                            // violation.
                            yield AgentEvent::TurnFailed {
                                session_id: session_id.clone(),
                                text: iter_text.clone(),
                                message: "provider produced tool_calls but no registry is attached"
                                    .into(),
                            };
                            return;
                        }
                    };

                    for call in calls {
                        let args_value = parse_tool_arguments(&call.arguments_str);
                        yield AgentEvent::ToolCall {
                            session_id: session_id.clone(),
                            tool_call_id: call.id.clone(),
                            tool_name: call.name.clone(),
                            args: args_value.clone(),
                        };
                        let outcome = match args_value {
                            Value::Null => crate::tool::ToolOutcome {
                                ok: false,
                                result: json!({
                                    "error": format!(
                                        "tool arguments are not valid JSON: {}",
                                        truncate_for_error(&call.arguments_str, 200)
                                    ),
                                    "kind": "invalid_arguments",
                                }),
                            },
                            v => registry.execute(&call.name, v).await,
                        };
                        yield AgentEvent::ToolResult {
                            session_id: session_id.clone(),
                            tool_call_id: call.id.clone(),
                            ok: outcome.ok,
                            result: outcome.result.clone(),
                        };
                        let tool_content = serde_json::to_string(&outcome.result)
                            .unwrap_or_else(|_| "null".into());
                        {
                            let mut s = session_arc.lock().await;
                            s.history.push(json!({
                                "role": "tool",
                                "tool_call_id": call.id,
                                "content": tool_content,
                            }));
                        }
                    }

                    iterations += 1;
                    if iterations >= MAX_TOOL_ITERATIONS {
                        yield AgentEvent::TurnFailed {
                            session_id: session_id.clone(),
                            text: iter_text.clone(),
                            message: format!(
                                "tool-call iteration cap of {MAX_TOOL_ITERATIONS} reached — agent may be in a loop",
                            ),
                        };
                        return;
                    }
                    // Loop: re-issue request with the new role:tool rows.
                    continue 'turn;
                }

                // Plain text reply (or `stop` with no tool calls). Persist
                // the assistant text and break out of the loop.
                {
                    let mut s = session_arc.lock().await;
                    s.history.push(json!({
                        "role": "assistant",
                        "content": iter_text.clone(),
                    }));
                }
                final_text = iter_text;
                break 'turn;
            }

            yield AgentEvent::TurnEnded {
                session_id,
                text: final_text,
                stop_reason: last_stop_reason,
                // HO-family usage capture is the next R028-F11 follow-up:
                // set stream_options.include_usage on the request body
                // and parse the final chunk's `usage` object.
                usage: None,
            };
        };

        Ok(Box::pin(stream) as Pin<Box<dyn futures::stream::Stream<Item = AgentEvent> + Send>>)
    }

    async fn stop(&self, session_id: &SessionId) -> Result<bool, RunnerError> {
        Ok(self
            .inner
            .sessions
            .write()
            .await
            .remove(session_id)
            .is_some())
    }
}

/// Parsed events from one `\n\n`-delimited SSE frame. A single frame
/// can carry multiple `data: …` lines; OpenAI typically emits one
/// chunk per frame, but the parser handles multiple defensively.
#[derive(Debug, PartialEq, Eq)]
enum FrameEvent {
    Delta {
        text: String,
        tool_calls: Vec<ToolCallDelta>,
        finish: Option<String>,
    },
    Error(String),
    Done,
}

/// One increment of a `delta.tool_calls[]` entry. OpenAI streams these
/// in pieces: the first chunk carries `index + id + function.name +
/// (often) the opening `{` of `function.arguments`; later chunks just
/// have `index` and a slice of `function.arguments`. The accumulator
/// (see [`ToolCallAcc`]) joins them by `index`.
#[derive(Debug, PartialEq, Eq, Default)]
struct ToolCallDelta {
    index: u32,
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

/// Live merge buffer for one tool call across SSE chunks. Keyed by the
/// provider's `index`. The `arguments_buf` is appended to in-place; we
/// only parse it into JSON once the SSE stream closes (or the
/// `finish_reason: "tool_calls"` chunk lands).
#[derive(Debug, Default)]
struct ToolCallAcc {
    id: String,
    name: String,
    arguments_buf: String,
}

#[derive(Debug)]
struct ToolCallFinalized {
    id: String,
    name: String,
    arguments_str: String,
}

impl ToolCallAcc {
    fn finalize(self) -> ToolCallFinalized {
        ToolCallFinalized {
            id: self.id,
            name: self.name,
            arguments_str: self.arguments_buf,
        }
    }
}

/// Issue one streaming chat-completions request. Used both for the
/// first request of a `send` (synchronously, so transport failures
/// surface as `RunnerError`) and for subsequent tool-call iterations
/// inside the stream (where any error has to ride the event stream).
///
/// On a non-success status code the response body is read up-front and
/// folded into the `RunnerError::Transport` message so callers see the
/// provider's error description rather than a bare HTTP code.
async fn issue_chat_completions(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: Option<&str>,
    provider_label: &str,
    body: Value,
) -> Result<reqwest::Response, RunnerError> {
    let mut req = client.post(endpoint).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| {
        RunnerError::Transport(format!("{} request failed: {}", provider_label, e))
    })?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(RunnerError::Transport(format!(
            "{} returned {}: {}",
            provider_label,
            status,
            body_text.chars().take(512).collect::<String>(),
        )));
    }
    Ok(resp)
}

/// Build the `tools[]` request payload from a registry, if any. Returns
/// `None` when the runner was constructed without a registry — in which
/// case the request body omits the field entirely (the model is told
/// nothing about tools and should never emit `tool_calls`).
fn tools_array_payload(tools: Option<&Arc<dyn ToolRegistry>>) -> Option<Vec<Value>> {
    let registry = tools?;
    let schemas = registry.schemas();
    if schemas.is_empty() {
        return None;
    }
    Some(
        schemas
            .into_iter()
            .map(|s| {
                json!({
                    "type": "function",
                    "function": {
                        "name": s.name,
                        "description": s.description,
                        "parameters": s.input_schema,
                    }
                })
            })
            .collect(),
    )
}

/// Parse the `arguments` JSON the model produced for a tool call.
/// Empty / whitespace-only → `{}` (the model declared no args). Any
/// other parse failure → `Value::Null`, which the caller surfaces as a
/// structured invalid-arguments outcome so the model can fix its next
/// attempt.
fn parse_tool_arguments(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str::<Value>(trimmed).unwrap_or(Value::Null)
}

/// Truncate a string for inclusion in an error message. Keeps tool
/// failure outputs scannable when the model produces a multi-KB blob
/// of malformed JSON.
fn truncate_for_error(raw: &str, cap: usize) -> String {
    if raw.chars().count() <= cap {
        return raw.to_string();
    }
    let cut = raw.char_indices().nth(cap).map(|(i, _)| i).unwrap_or(raw.len());
    format!("{}…", &raw[..cut])
}

fn parse_sse_frame(frame: &str) -> Vec<FrameEvent> {
    let mut out = Vec::new();
    for line in frame.lines() {
        let Some(payload) = line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:"))
        else {
            continue;
        };
        let payload = payload.trim();
        if payload == "[DONE]" {
            out.push(FrameEvent::Done);
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        if let Some(error) = json.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("openai-compatible error")
                .to_string();
            out.push(FrameEvent::Error(msg));
            continue;
        }
        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
            for choice in choices {
                let delta = choice.get("delta");
                let text = delta
                    .and_then(|d| d.get("content"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_calls = delta
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(parse_tool_call_delta)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let finish = choice
                    .get("finish_reason")
                    .and_then(|f| f.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                out.push(FrameEvent::Delta {
                    text,
                    tool_calls,
                    finish,
                });
            }
        }
    }
    out
}

fn parse_tool_call_delta(raw: &Value) -> Option<ToolCallDelta> {
    let index = raw.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
    let id = raw
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let func = raw.get("function");
    let name = func
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let arguments = func
        .and_then(|f| f.get("arguments"))
        .and_then(|a| a.as_str())
        .map(|s| s.to_string());
    if id.is_none() && name.is_none() && arguments.is_none() {
        return None;
    }
    Some(ToolCallDelta {
        index,
        id,
        name,
        arguments,
    })
}

fn build_request_body(
    session: &InnerSession,
    max_tokens: u32,
    tools: Option<&[Value]>,
) -> serde_json::Value {
    let mut messages: Vec<Value> = Vec::with_capacity(session.history.len() + 1);
    messages.push(json!({
        "role": "system",
        "content": session.prelude_text,
    }));
    messages.extend(session.history.iter().cloned());
    let mut body = json!({
        "model": session.model,
        "stream": true,
        "max_tokens": max_tokens,
        "messages": messages,
    });
    if let Some(arr) = tools {
        if !arr.is_empty() {
            body["tools"] = Value::Array(arr.to_vec());
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(prelude: &str, model: &str) -> InnerSession {
        InnerSession {
            ticket_id: "T01".into(),
            prelude_text: prelude.into(),
            model: model.into(),
            history: Vec::new(),
        }
    }

    #[test]
    fn body_includes_system_prelude_then_history_in_order() {
        let mut s = session("PRELUDE", "gpt-4o");
        s.history.push(json!({ "role": "user", "content": "hi" }));
        s.history.push(json!({ "role": "assistant", "content": "hey" }));
        s.history.push(json!({ "role": "user", "content": "what's up" }));
        let body = build_request_body(&s, 1024, None);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 1024);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "PRELUDE");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "hi");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[2]["content"], "hey");
        assert_eq!(msgs[3]["role"], "user");
        assert_eq!(msgs[3]["content"], "what's up");
        // No registry attached → `tools` field absent (not just empty).
        assert!(body.get("tools").is_none(), "{body}");
    }

    #[test]
    fn body_includes_tools_array_when_schemas_provided() {
        let s = session("PRELUDE", "gpt-4o");
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a rig file.",
                "parameters": { "type": "object", "properties": { "path": { "type": "string" } } }
            }
        })];
        let body = build_request_body(&s, 1024, Some(&tools));
        let arr = body["tools"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["function"]["name"], "read_file");
        assert_eq!(arr[0]["type"], "function");
    }

    #[test]
    fn parse_frame_extracts_content_delta_without_finish() {
        let frame = r#"data: {"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![FrameEvent::Delta {
                text: "hello".into(),
                tool_calls: vec![],
                finish: None
            }]
        );
    }

    #[test]
    fn parse_frame_extracts_finish_reason_on_terminal_chunk() {
        let frame = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![FrameEvent::Delta {
                text: "".into(),
                tool_calls: vec![],
                finish: Some("stop".into())
            }]
        );
    }

    #[test]
    fn parse_frame_done_sentinel_emits_done_event() {
        assert_eq!(parse_sse_frame("data: [DONE]"), vec![FrameEvent::Done]);
        assert_eq!(parse_sse_frame("data:[DONE]"), vec![FrameEvent::Done]);
    }

    #[test]
    fn parse_frame_error_payload_emits_error_event() {
        let frame = r#"data: {"error":{"message":"rate limited"}}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(evs, vec![FrameEvent::Error("rate limited".into())]);
    }

    #[test]
    fn parse_frame_skips_unparseable_lines_silently() {
        // Stray heartbeat / event-stream comments shouldn't break the
        // parser. A real OpenAI deployment occasionally interleaves
        // `: keepalive\n\n` ping frames; we just want them ignored.
        assert!(parse_sse_frame(": keepalive").is_empty());
        assert!(parse_sse_frame("data: not json").is_empty());
    }

    #[test]
    fn parse_frame_handles_multiple_choices_in_one_payload() {
        let frame = r#"data: {"choices":[{"delta":{"content":"a"}},{"delta":{"content":"b"},"finish_reason":"stop"}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![
                FrameEvent::Delta {
                    text: "a".into(),
                    tool_calls: vec![],
                    finish: None
                },
                FrameEvent::Delta {
                    text: "b".into(),
                    tool_calls: vec![],
                    finish: Some("stop".into())
                },
            ]
        );
    }

    #[test]
    fn parse_frame_extracts_tool_call_first_chunk_with_id_and_name() {
        // First streaming chunk for a tool call: id + name set, args
        // typically begins with `{` then accretes across chunks. The
        // accumulator (tested separately) joins them by index.
        let frame = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"read_file","arguments":"{\""}}]}}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![FrameEvent::Delta {
                text: "".into(),
                tool_calls: vec![ToolCallDelta {
                    index: 0,
                    id: Some("call_abc".into()),
                    name: Some("read_file".into()),
                    arguments: Some("{\"".into()),
                }],
                finish: None,
            }]
        );
    }

    #[test]
    fn parse_frame_extracts_tool_call_continuation_chunk() {
        // Mid-stream chunk: only the args slice is set; id and name come
        // through from the first chunk.
        let frame = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"path\":\"Cargo.toml\"}"}}]}}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![FrameEvent::Delta {
                text: "".into(),
                tool_calls: vec![ToolCallDelta {
                    index: 0,
                    id: None,
                    name: None,
                    arguments: Some("path\":\"Cargo.toml\"}".into()),
                }],
                finish: None,
            }]
        );
    }

    #[test]
    fn parse_frame_carries_finish_reason_tool_calls_with_empty_delta() {
        // Final chunk of a tool-call response: empty delta + finish
        // reason. The runner uses this to know it should dispatch the
        // accumulated calls and re-issue.
        let frame = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let evs = parse_sse_frame(frame);
        assert_eq!(
            evs,
            vec![FrameEvent::Delta {
                text: "".into(),
                tool_calls: vec![],
                finish: Some("tool_calls".into()),
            }]
        );
    }

    #[test]
    fn tool_call_acc_joins_args_across_chunks_in_index_order() {
        // Simulate the accumulator the runner uses: index → ToolCallAcc.
        // First chunk seeds id+name; later chunks just append to args.
        let mut accs: BTreeMap<u32, ToolCallAcc> = BTreeMap::new();
        let chunks = vec![
            ToolCallDelta {
                index: 0,
                id: Some("call_1".into()),
                name: Some("grep".into()),
                arguments: Some("{\"pat".into()),
            },
            ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some("tern\":\"foo\"}".into()),
            },
            ToolCallDelta {
                index: 1,
                id: Some("call_2".into()),
                name: Some("read_file".into()),
                arguments: Some("{\"path\":\"x\"}".into()),
            },
        ];
        for tcd in chunks {
            let acc = accs.entry(tcd.index).or_default();
            if let Some(id) = tcd.id {
                acc.id = id;
            }
            if let Some(name) = tcd.name {
                acc.name = name;
            }
            if let Some(args) = tcd.arguments {
                acc.arguments_buf.push_str(&args);
            }
        }
        let finalized: Vec<_> = accs.into_iter().map(|(_, a)| a.finalize()).collect();
        assert_eq!(finalized.len(), 2);
        assert_eq!(finalized[0].id, "call_1");
        assert_eq!(finalized[0].name, "grep");
        assert_eq!(finalized[0].arguments_str, "{\"pattern\":\"foo\"}");
        assert_eq!(finalized[1].id, "call_2");
        assert_eq!(finalized[1].name, "read_file");
        assert_eq!(finalized[1].arguments_str, "{\"path\":\"x\"}");
    }

    #[test]
    fn parse_tool_arguments_treats_empty_as_object() {
        // Models sometimes emit `arguments: ""` for parameter-less calls.
        // The downstream registry expects `type: object` args, so we
        // synthesise `{}` rather than failing the call.
        assert_eq!(parse_tool_arguments(""), json!({}));
        assert_eq!(parse_tool_arguments("   "), json!({}));
    }

    #[test]
    fn parse_tool_arguments_returns_null_on_invalid_json() {
        // Malformed args produce Value::Null; the runner converts that
        // into an `invalid_arguments` ToolOutcome the model can correct.
        assert_eq!(parse_tool_arguments("not json"), Value::Null);
        assert_eq!(parse_tool_arguments("{ unclosed"), Value::Null);
    }

    #[test]
    fn truncate_for_error_keeps_short_strings_intact() {
        assert_eq!(truncate_for_error("hello", 200), "hello");
    }

    #[test]
    fn truncate_for_error_appends_ellipsis_when_long() {
        let long = "x".repeat(500);
        let out = truncate_for_error(&long, 100);
        assert_eq!(out.chars().count(), 101);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn tools_array_payload_returns_none_when_registry_absent() {
        assert!(tools_array_payload(None).is_none());
    }

    #[test]
    fn tools_array_payload_wraps_each_schema_as_function_object() {
        use crate::tool::{ToolOutcome, ToolRegistry, ToolSchema};
        use async_trait::async_trait;

        struct StubRegistry(Vec<ToolSchema>);
        #[async_trait]
        impl ToolRegistry for StubRegistry {
            fn schemas(&self) -> Vec<ToolSchema> {
                self.0.clone()
            }
            async fn execute(&self, _name: &str, _args: Value) -> ToolOutcome {
                ToolOutcome::ok(json!(null))
            }
        }

        let registry: Arc<dyn ToolRegistry> = Arc::new(StubRegistry(vec![ToolSchema {
            name: "read_file".into(),
            description: "Read a file.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }]));
        let arr = tools_array_payload(Some(&registry)).expect("schemas present");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "function");
        assert_eq!(arr[0]["function"]["name"], "read_file");
        assert_eq!(arr[0]["function"]["description"], "Read a file.");
        assert_eq!(arr[0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tools_array_payload_returns_none_for_empty_schemas() {
        // An attached-but-empty registry shouldn't pollute the request
        // body with `tools: []`. Most providers treat the empty array as
        // "tools allowed but none offered" which is just confusing.
        use crate::tool::{ToolOutcome, ToolRegistry, ToolSchema};
        use async_trait::async_trait;

        struct EmptyRegistry;
        #[async_trait]
        impl ToolRegistry for EmptyRegistry {
            fn schemas(&self) -> Vec<ToolSchema> {
                vec![]
            }
            async fn execute(&self, _name: &str, _args: Value) -> ToolOutcome {
                ToolOutcome::ok(json!(null))
            }
        }
        let registry: Arc<dyn ToolRegistry> = Arc::new(EmptyRegistry);
        assert!(tools_array_payload(Some(&registry)).is_none());
    }

    #[tokio::test]
    async fn start_refuses_engine_with_a_different_provider_label() {
        use yah_kg::anno::EngineRef;
        use yah_kg::prelude::{CacheControl, CacheTtl, Prelude};

        let runner = OpenAiCompatRunner::new(OpenAiCompatConfig::openai("dummy"));
        let prelude = Prelude {
            sections: vec![],
            cache: CacheControl {
                key: "0".into(),
                ttl: CacheTtl::Ephemeral,
            },
            engine: Some(EngineRef {
                provider: "claude".into(),
                model: None,
            }),
            think: None,
            estimated_tokens: 0,
            ring_depth: 0.0,
            truncated: false,
        };
        let err = runner.start(prelude, "T01").await.unwrap_err();
        assert!(matches!(err, RunnerError::UnsupportedEngine { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn start_then_stop_round_trips_session_registry() {
        let runner = OpenAiCompatRunner::new(OpenAiCompatConfig::openai("dummy"));
        let prelude = yah_kg::prelude::Prelude {
            sections: vec![],
            cache: yah_kg::prelude::CacheControl {
                key: "0".into(),
                ttl: yah_kg::prelude::CacheTtl::Ephemeral,
            },
            engine: None, // no engine = accept any provider, fall through to default model
            think: None,
            estimated_tokens: 0,
            ring_depth: 0.0,
            truncated: false,
        };
        let id = runner.start(prelude, "T01").await.unwrap();
        assert!(runner.stop(&id).await.unwrap());
        // Idempotent: second stop returns false.
        assert!(!runner.stop(&id).await.unwrap());
    }

    #[test]
    fn derive_models_url_swaps_chat_completions_suffix() {
        assert_eq!(
            derive_models_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/models",
        );
        assert_eq!(
            derive_models_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/models",
        );
    }

    #[test]
    fn derive_models_url_appends_when_path_doesnt_match() {
        assert_eq!(
            derive_models_url("https://gateway.example/v1"),
            "https://gateway.example/v1/models",
        );
        assert_eq!(
            derive_models_url("https://gateway.example/v1/"),
            "https://gateway.example/v1/models",
        );
    }

    #[tokio::test]
    async fn send_to_unknown_session_returns_session_not_found() {
        let runner = OpenAiCompatRunner::new(OpenAiCompatConfig::openai("dummy"));
        // EventStream doesn't impl Debug, so we can't `.unwrap_err()` on
        // `Result<EventStream, _>` — match the `Err` arm explicitly.
        match runner
            .send(&SessionId::new("session:00000000"), "hi".into())
            .await
        {
            Err(RunnerError::SessionNotFound(_)) => {}
            Err(other) => panic!("expected SessionNotFound, got {other:?}"),
            Ok(_) => panic!("expected error for missing session"),
        }
    }
}
