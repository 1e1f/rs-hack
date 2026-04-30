//! HAo / HAk wire smoke test against `api.anthropic.com/v1/messages`.
//!
//! Exercises the same wire that `app/tauri/src/agent.rs::run_anthropic_turn`
//! drives, but inline + dependency-free so it can run from the
//! command line without standing up the Tauri host. Confirms that
//! the Bearer + `anthropic-beta: oauth-2025-04-20` header combo is
//! accepted by Anthropic's /v1/messages endpoint with whatever
//! `claude setup-token` produced (HAo) — and as a sanity comparison,
//! that the same body works with `x-api-key` (HAk) auth.
//!
//! Variants:
//!
//! ```text
//! cargo run -p yah-runner --example hao_smoke -- crab   # needs ANTHROPIC_OAUTH_TOKEN
//! cargo run -p yah-runner --example hao_smoke -- hak    # needs ANTHROPIC_API_KEY
//! ```
//!
//! Default model: `claude-haiku-4-5-20251001` (fast + cheap). Override
//! with a second arg.

use futures::StreamExt;
use serde_json::json;
use std::env;

const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
// Two beta values: oauth-2025-04-20 enables OAuth-token semantics,
// claude-code-20250219 declares Claude Code protocol compliance.
// Anthropic enforces both — sending only the first triggers
// shape-mismatch rate-limiting.
const ANTHROPIC_OAUTH_BETA: &str = "oauth-2025-04-20,claude-code-20250219";
const ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let variant = args.get(1).map(|s| s.as_str()).unwrap_or("crab");
    let model = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // OAuth path requires the Claude Code identity prefix as the first
    // system block; API-key path doesn't (and sending it would just be
    // a wasted prompt token). Caller picks via the variant arg.
    let system_blocks = match variant {
        "crab" | "hao" => json!([
            { "type": "text", "text": ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX },
        ]),
        _ => json!([]),
    };

    let body = json!({
        "model": model,
        "max_tokens": 256,
        "stream": true,
        "system": system_blocks,
        "messages": [{
            "role": "user",
            "content": "Reply with exactly the words: HAo wire ok",
        }],
    });

    let client = reqwest::Client::new();
    let req = client
        .post(ANTHROPIC_MESSAGES_URL)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body);

    let req = match variant {
        "crab" | "hao" => {
            let token = env::var("ANTHROPIC_OAUTH_TOKEN").map_err(|_| {
                "ANTHROPIC_OAUTH_TOKEN not set — run `claude setup-token` and export it"
            })?;
            eprintln!("→ crab (HAo): /v1/messages w/ Bearer + anthropic-beta + Claude Code prefix, model {model}");
            req.header("authorization", format!("Bearer {token}"))
                .header("anthropic-beta", ANTHROPIC_OAUTH_BETA)
        }
        "hak" | "anthropic" => {
            let key = env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "ANTHROPIC_API_KEY not set — needed for hak variant")?;
            eprintln!("→ anthropic (HAk): /v1/messages w/ x-api-key, model {model}");
            req.header("x-api-key", key)
        }
        other => return Err(format!("unknown variant '{other}' — expected: crab, hak").into()),
    };

    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        eprintln!(
            "✗ {} returned: {}",
            status,
            body.chars().take(512).collect::<String>()
        );
        std::process::exit(1);
    }

    let mut accumulated = String::new();
    let mut stop_reason: Option<String> = None;
    let mut buffer = String::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        let text = std::str::from_utf8(&bytes)?;
        buffer.push_str(text);
        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer.drain(..idx + 2);
            for line in frame.lines() {
                let Some(payload) = line.strip_prefix("data: ") else {
                    continue;
                };
                if payload == "[DONE]" {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<serde_json::Value>(payload) else {
                    continue;
                };
                match event.get("type").and_then(|v| v.as_str()) {
                    Some("content_block_delta") => {
                        if let Some(t) = event
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            accumulated.push_str(t);
                            print!("{t}");
                        }
                    }
                    Some("message_delta") => {
                        if let Some(reason) = event
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|r| r.as_str())
                        {
                            stop_reason = Some(reason.to_string());
                        }
                    }
                    Some("error") => {
                        let msg = event
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("anthropic error");
                        eprintln!("\n✗ in-stream error: {msg}");
                        std::process::exit(1);
                    }
                    _ => {}
                }
            }
        }
    }
    println!();
    eprintln!(
        "✓ {variant} round-trip OK · stop_reason={:?} · {} chars accumulated",
        stop_reason,
        accumulated.len()
    );
    Ok(())
}
