//! End-to-end integration test for the OpenAI-compatible runner.
//!
//! Stands up an in-process TCP server that replays canned SSE frames,
//! points the runner at it, and asserts the full event stream the
//! consumer sees. Exercises the wire path no unit test can: real
//! HTTP/1.1 framing, real `reqwest::bytes_stream()`, real chunk
//! boundaries chopping deltas mid-frame.

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use kg::agent::AgentEvent;
use kg::anno::EngineRef;
use kg::prelude::{CacheControl, CacheTtl, Prelude};
use runner::{
    OpenAiCompatConfig, OpenAiCompatRunner, Runner, ToolOutcome, ToolRegistry, ToolSchema,
};

const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn a one-shot HTTP/1.1 server that:
///  1. Accepts one connection.
///  2. Reads until the request body is fully consumed.
///  3. Replies with `text/event-stream`, the supplied SSE frames
///     concatenated as the body. No real chunked encoding — we send a
///     single Content-Length response that reqwest still streams via
///     `bytes_stream()`.
///
/// Returns the URL the runner should POST against.
async fn spawn_fake_sse_server(frames: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 8192];
        let mut received = Vec::<u8>::new();
        let mut content_length: Option<usize> = None;
        let mut header_end: Option<usize> = None;
        loop {
            let n = sock.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buf[..n]);
            if header_end.is_none() {
                if let Some(pos) = received
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                {
                    header_end = Some(pos + 4);
                    let header_str = std::str::from_utf8(&received[..pos]).unwrap_or("");
                    for line in header_str.split("\r\n") {
                        if let Some(rest) = line
                            .strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                        {
                            content_length = rest.trim().parse::<usize>().ok();
                        }
                    }
                }
            }
            if let (Some(end), Some(cl)) = (header_end, content_length) {
                if received.len() >= end + cl {
                    break;
                }
            } else if header_end.is_some() && content_length.is_none() {
                // No body declared (GET-style) — stop after headers.
                break;
            }
        }

        let body: String = frames
            .into_iter()
            .map(|f| format!("{}\n\n", f))
            .collect();
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/event-stream\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            body.len(),
            body
        );
        sock.write_all(response.as_bytes()).await.unwrap();
        let _ = sock.flush().await;
        let _ = sock.shutdown().await;
    });
    format!("http://{}/v1/chat/completions", addr)
}

fn fake_prelude(provider: &str) -> Prelude {
    Prelude {
        sections: vec![],
        cache: CacheControl {
            key: "0".into(),
            ttl: CacheTtl::Ephemeral,
        },
        engine: Some(EngineRef {
            provider: provider.into(),
            model: Some("fake-model".into()),
        }),
        think: None,
        estimated_tokens: 0,
        ring_depth: 0.0,
        truncated: false,
    }
}

#[tokio::test]
async fn streams_deltas_then_turn_ended_with_stop_reason() {
    let url = spawn_fake_sse_server(vec![
        r#"data: {"choices":[{"delta":{"content":"hello "},"finish_reason":null}]}"#.into(),
        r#"data: {"choices":[{"delta":{"content":"world"},"finish_reason":null}]}"#.into(),
        r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.into(),
        "data: [DONE]".into(),
    ])
    .await;

    let runner = OpenAiCompatRunner::new(OpenAiCompatConfig {
        provider_label: "openai".into(),
        endpoint: url,
        default_model: "fake-model".into(),
        api_key: Some("dummy".into()),
        max_output_tokens: None,
    });

    let id = runner
        .start(fake_prelude("openai"), "T01")
        .await
        .expect("start");
    let stream = runner
        .send(&id, "hi".into())
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("send");

    let events = tokio::time::timeout(CLIENT_TIMEOUT, stream.collect::<Vec<_>>())
        .await
        .expect("stream completes within timeout");

    assert_eq!(
        events.len(),
        4,
        "expected TurnStarted + 2 deltas + TurnEnded, got {events:?}"
    );
    assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
    let AgentEvent::MessageDelta { ref text, .. } = events[1] else {
        panic!("expected MessageDelta, got {:?}", events[1]);
    };
    assert_eq!(text, "hello ");
    let AgentEvent::MessageDelta { ref text, .. } = events[2] else {
        panic!("expected MessageDelta, got {:?}", events[2]);
    };
    assert_eq!(text, "world");
    let AgentEvent::TurnEnded {
        ref text,
        ref stop_reason,
        ..
    } = events[3]
    else {
        panic!("expected TurnEnded, got {:?}", events[3]);
    };
    assert_eq!(text, "hello world");
    assert_eq!(stop_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn error_payload_in_stream_yields_error_event_and_terminates() {
    let url = spawn_fake_sse_server(vec![
        r#"data: {"choices":[{"delta":{"content":"partial"},"finish_reason":null}]}"#.into(),
        r#"data: {"error":{"message":"context length exceeded"}}"#.into(),
        // Server keeps emitting after the error, but the parser must
        // stop forwarding anything past it (the runner returns from the
        // inner stream as soon as it yields Error).
        r#"data: {"choices":[{"delta":{"content":"unreachable"}}]}"#.into(),
    ])
    .await;

    let runner = OpenAiCompatRunner::new(OpenAiCompatConfig {
        provider_label: "openai".into(),
        endpoint: url,
        default_model: "fake-model".into(),
        api_key: Some("dummy".into()),
        max_output_tokens: None,
    });

    let id = runner
        .start(fake_prelude("openai"), "T01")
        .await
        .expect("start");
    let stream = runner
        .send(&id, "hi".into())
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("send");

    let events = tokio::time::timeout(CLIENT_TIMEOUT, stream.collect::<Vec<_>>())
        .await
        .expect("stream completes within timeout");

    // Expect: TurnStarted, MessageDelta(partial), TurnFailed (carrying
    // the partial accumulator). No TurnEnded.
    assert_eq!(events.len(), 3, "got {events:?}");
    assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
    let AgentEvent::MessageDelta { text: ref delta_text, .. } = events[1] else {
        panic!("expected MessageDelta, got {:?}", events[1]);
    };
    let AgentEvent::TurnFailed { ref message, ref text, .. } = events[2] else {
        panic!("expected TurnFailed, got {:?}", events[2]);
    };
    assert!(message.contains("context length"), "{message}");
    // TurnFailed carries the same accumulated text the renderer streamed.
    assert_eq!(text, delta_text, "TurnFailed.text should mirror accumulated deltas");
}

#[tokio::test]
async fn upstream_4xx_surfaces_as_runner_error_before_stream_starts() {
    // 401 from the upstream means we never get to stream — `send`
    // should return `RunnerError::Transport` synchronously rather than
    // yielding it on the stream.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 8192];
        // Drain whatever the client sent.
        let _ = sock.read(&mut buf).await;
        let body = r#"{"error":{"message":"invalid_api_key"}}"#;
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {}",
            body.len(),
            body
        );
        let _ = sock.write_all(response.as_bytes()).await;
        let _ = sock.shutdown().await;
    });

    let runner = OpenAiCompatRunner::new(OpenAiCompatConfig {
        provider_label: "openai".into(),
        endpoint: format!("http://{}/v1/chat/completions", addr),
        default_model: "fake-model".into(),
        api_key: Some("bad-key".into()),
        max_output_tokens: None,
    });

    let id = runner
        .start(fake_prelude("openai"), "T01")
        .await
        .expect("start");
    match runner.send(&id, "hi".into()).await {
        Err(runner::RunnerError::Transport(msg)) => {
            assert!(msg.contains("401"), "{msg}");
            assert!(msg.contains("invalid_api_key"), "{msg}");
        }
        Err(other) => panic!("expected Transport error, got {other:?}"),
        Ok(_) => panic!("expected error for 401 response"),
    }
}

/// Spawn a server that answers `responses.len()` sequential requests,
/// each with a different SSE payload. The runner re-issues a request
/// per tool-call iteration; this lets the test simulate the
/// "tool_calls → role:tool → stop" round-trip without a real provider.
async fn spawn_multi_request_sse_server(responses: Vec<Vec<String>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for frames in responses {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 8192];
            let mut received = Vec::<u8>::new();
            let mut content_length: Option<usize> = None;
            let mut header_end: Option<usize> = None;
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                received.extend_from_slice(&buf[..n]);
                if header_end.is_none() {
                    if let Some(pos) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let header_str = std::str::from_utf8(&received[..pos]).unwrap_or("");
                        for line in header_str.split("\r\n") {
                            if let Some(rest) = line
                                .strip_prefix("Content-Length:")
                                .or_else(|| line.strip_prefix("content-length:"))
                            {
                                content_length = rest.trim().parse::<usize>().ok();
                            }
                        }
                    }
                }
                if let (Some(end), Some(cl)) = (header_end, content_length) {
                    if received.len() >= end + cl {
                        break;
                    }
                } else if header_end.is_some() && content_length.is_none() {
                    break;
                }
            }

            let body: String = frames
                .into_iter()
                .map(|f| format!("{}\n\n", f))
                .collect();
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: text/event-stream\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {}",
                body.len(),
                body
            );
            sock.write_all(response.as_bytes()).await.unwrap();
            let _ = sock.flush().await;
            let _ = sock.shutdown().await;
        }
    });
    format!("http://{}/v1/chat/completions", addr)
}

/// Stub registry exposing one tool that returns whatever the test
/// hands it. Tracks the call count so assertions can verify the
/// runner dispatched.
struct StubRegistry {
    schemas: Vec<ToolSchema>,
    response: Value,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ToolRegistry for StubRegistry {
    fn schemas(&self) -> Vec<ToolSchema> {
        self.schemas.clone()
    }
    async fn execute(&self, _name: &str, _args: Value) -> ToolOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ToolOutcome::ok(self.response.clone())
    }
}

#[tokio::test]
async fn tool_call_loop_dispatches_then_re_issues_for_final_text() {
    // Round-trip 1: provider asks for `read_file({ path: "Cargo.toml" })`.
    // Round-trip 2: provider returns plain text with finish_reason "stop".
    // Expected event order:
    //   TurnStarted → ToolCall → ToolResult → MessageDelta → TurnEnded
    let url = spawn_multi_request_sse_server(vec![
        // First response: streamed tool_call across two chunks + a
        // terminal finish_reason="tool_calls" frame.
        vec![
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":"}}]},"finish_reason":null}]}"#.into(),
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Cargo.toml\"}"}}]},"finish_reason":null}]}"#.into(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#.into(),
            "data: [DONE]".into(),
        ],
        // Second response: model reasons over the tool result and ends.
        vec![
            r#"data: {"choices":[{"delta":{"content":"It's the workspace Cargo.toml."},"finish_reason":null}]}"#.into(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.into(),
            "data: [DONE]".into(),
        ],
    ])
    .await;

    let calls = Arc::new(AtomicUsize::new(0));
    let registry: Arc<dyn ToolRegistry> = Arc::new(StubRegistry {
        schemas: vec![ToolSchema {
            name: "read_file".into(),
            description: "Read a file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        }],
        response: json!({ "content": "[package]\nname = \"x\"\n", "bytes": 24 }),
        calls: Arc::clone(&calls),
    });

    let runner = OpenAiCompatRunner::with_tools(
        OpenAiCompatConfig {
            provider_label: "openai".into(),
            endpoint: url,
            default_model: "fake-model".into(),
            api_key: Some("dummy".into()),
            max_output_tokens: None,
        },
        registry,
    );

    let id = runner
        .start(fake_prelude("openai"), "T01")
        .await
        .expect("start");
    let stream = runner
        .send(&id, "what's in Cargo.toml?".into())
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("send");

    let events = tokio::time::timeout(CLIENT_TIMEOUT, stream.collect::<Vec<_>>())
        .await
        .expect("stream completes within timeout");

    assert_eq!(calls.load(Ordering::SeqCst), 1, "registry must dispatch once");

    // Order: TurnStarted, ToolCall, ToolResult, MessageDelta, TurnEnded.
    assert_eq!(events.len(), 5, "got {events:?}");
    assert!(matches!(events[0], AgentEvent::TurnStarted { .. }));
    let AgentEvent::ToolCall {
        ref tool_name,
        ref args,
        ref tool_call_id,
        ..
    } = events[1]
    else {
        panic!("expected ToolCall, got {:?}", events[1]);
    };
    assert_eq!(tool_name, "read_file");
    assert_eq!(args["path"], "Cargo.toml");
    assert_eq!(tool_call_id, "call_1");
    let AgentEvent::ToolResult {
        ok,
        ref result,
        ref tool_call_id,
        ..
    } = events[2]
    else {
        panic!("expected ToolResult, got {:?}", events[2]);
    };
    assert!(ok);
    assert_eq!(result["bytes"], 24);
    assert_eq!(tool_call_id, "call_1");
    let AgentEvent::MessageDelta { ref text, .. } = events[3] else {
        panic!("expected MessageDelta, got {:?}", events[3]);
    };
    assert_eq!(text, "It's the workspace Cargo.toml.");
    let AgentEvent::TurnEnded {
        ref text,
        ref stop_reason,
        ..
    } = events[4]
    else {
        panic!("expected TurnEnded, got {:?}", events[4]);
    };
    assert_eq!(text, "It's the workspace Cargo.toml.");
    assert_eq!(stop_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn tool_call_with_invalid_arguments_surfaces_invalid_args_outcome() {
    // Model produces malformed JSON for `arguments`. The runner must
    // emit ToolCall + a synthetic ToolResult with kind: invalid_arguments
    // (without dispatching to the registry) so the model can correct on
    // the next turn.
    let url = spawn_multi_request_sse_server(vec![
        vec![
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_bad","type":"function","function":{"name":"read_file","arguments":"{ unclosed"}}]}}]}"#.into(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#.into(),
            "data: [DONE]".into(),
        ],
        vec![
            r#"data: {"choices":[{"delta":{"content":"sorry, retrying"},"finish_reason":null}]}"#.into(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.into(),
            "data: [DONE]".into(),
        ],
    ])
    .await;

    let calls = Arc::new(AtomicUsize::new(0));
    let registry: Arc<dyn ToolRegistry> = Arc::new(StubRegistry {
        schemas: vec![ToolSchema {
            name: "read_file".into(),
            description: "Read.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        }],
        response: json!({}),
        calls: Arc::clone(&calls),
    });

    let runner = OpenAiCompatRunner::with_tools(
        OpenAiCompatConfig {
            provider_label: "openai".into(),
            endpoint: url,
            default_model: "fake-model".into(),
            api_key: Some("dummy".into()),
            max_output_tokens: None,
        },
        registry,
    );

    let id = runner
        .start(fake_prelude("openai"), "T01")
        .await
        .expect("start");
    let stream = runner
        .send(&id, "go".into())
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("send");
    let events = tokio::time::timeout(CLIENT_TIMEOUT, stream.collect::<Vec<_>>())
        .await
        .expect("stream completes within timeout");

    // Registry must NOT have been called — args were malformed, so the
    // runner short-circuits with an invalid_arguments result.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let result_event = events
        .iter()
        .find(|e| matches!(e, AgentEvent::ToolResult { .. }))
        .expect("ToolResult expected");
    let AgentEvent::ToolResult { ok, ref result, .. } = result_event else {
        unreachable!();
    };
    assert!(!ok);
    assert_eq!(result["kind"], "invalid_arguments");
}

/// Live HOk validation: drive a real `ollama serve` round-trip end-to-end.
///
/// `#[ignore]` because it depends on a process the dev has to start
/// themselves; CI doesn't bring one up. Run as:
///
/// ```bash
/// ollama serve &  # if not already running
/// cargo test -p yah-runner --test openai_compat_e2e -- --ignored ollama_local_round_trip
/// ```
///
/// Asserts the contract from R028-F3's @yah:next: a Prelude → start →
/// send cycle yields TurnStarted, at least one MessageDelta, and
/// TurnEnded with a stop reason.
#[tokio::test]
#[ignore = "requires `ollama serve` on localhost:11434 with qwen2.5-coder:1.5b pulled"]
async fn ollama_local_round_trip() {
    // Quick liveness probe — if ollama isn't up, skip with a friendly
    // hint rather than a confusing connection-refused panic.
    let probe = reqwest::Client::new()
        .get("http://localhost:11434/api/tags")
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    if probe.is_err() {
        eprintln!("ollama not reachable on :11434 — start `ollama serve` first");
        return;
    }

    let prelude = Prelude {
        sections: vec![],
        cache: CacheControl {
            key: "live-ollama-smoke".into(),
            ttl: CacheTtl::Ephemeral,
        },
        engine: Some(EngineRef {
            provider: "ollama".into(),
            model: Some("qwen2.5-coder:1.5b".into()),
        }),
        think: None,
        estimated_tokens: 0,
        ring_depth: 0.0,
        truncated: false,
    };

    let runner = OpenAiCompatRunner::new(OpenAiCompatConfig::ollama_local());

    let id = runner.start(prelude, "T01").await.expect("start");
    let stream = runner
        .send(&id, "Reply with the single word: pong".into())
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("send");

    // Generous timeout — first request after model warmup can be slow.
    let events = tokio::time::timeout(Duration::from_secs(60), stream.collect::<Vec<_>>())
        .await
        .expect("stream completes within 60s");

    assert!(
        matches!(events.first(), Some(AgentEvent::TurnStarted { .. })),
        "expected TurnStarted first, got {events:?}"
    );

    let delta_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::MessageDelta { .. }))
        .count();
    assert!(delta_count >= 1, "expected ≥1 MessageDelta, got {events:?}");

    let last = events.last().expect("non-empty event stream");
    let AgentEvent::TurnEnded {
        ref text,
        ref stop_reason,
        ..
    } = last
    else {
        panic!("expected TurnEnded last, got {last:?} — full stream {events:?}");
    };
    assert!(!text.is_empty(), "TurnEnded should carry accumulated text");
    assert!(
        stop_reason.is_some(),
        "TurnEnded should carry a stop reason (got None) — full stream {events:?}"
    );
}
