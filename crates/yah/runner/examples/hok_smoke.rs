//! HOk preset smoke test — drives the OpenAI-compat runner end-to-end
//! against a real provider.
//!
//! Picks a provider variant from CLI args and runs a single turn:
//! build a tiny Prelude → start session → send "say hi" → print every
//! AgentEvent the runner emits → stop session. Exits non-zero if the
//! turn errors before reaching `TurnEnded`.
//!
//! Variants:
//!
//! ```text
//! cargo run -p yah-runner --example hok_smoke -- ollama-local [model]
//! cargo run -p yah-runner --example hok_smoke -- openai [model]       # needs OPENAI_API_KEY
//! cargo run -p yah-runner --example hok_smoke -- ollama-cloud [model] # needs OLLAMA_API_KEY
//! ```
//!
//! Default model per variant matches `OpenAiCompatConfig::*` shipping
//! defaults. Override by passing a second arg.

use futures::StreamExt;
use std::env;
use kg::prelude::{CacheControl, CacheTtl, Prelude};
use runner::{OpenAiCompatConfig, OpenAiCompatRunner, Runner};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let variant = args.get(1).map(|s| s.as_str()).unwrap_or("ollama-local");
    let model_override = args.get(2).cloned();

    let mut config = match variant {
        "ollama-local" => OpenAiCompatConfig::ollama_local(),
        "openai" => OpenAiCompatConfig::openai(
            env::var("OPENAI_API_KEY")
                .map_err(|_| "OPENAI_API_KEY not set — needed for openai variant")?,
        ),
        "ollama-cloud" => OpenAiCompatConfig::ollama_cloud(
            env::var("OLLAMA_API_KEY")
                .map_err(|_| "OLLAMA_API_KEY not set — needed for ollama-cloud variant")?,
        ),
        other => {
            return Err(format!(
                "unknown variant '{other}' — expected: ollama-local, openai, ollama-cloud"
            )
            .into())
        }
    };
    if let Some(m) = model_override {
        config.default_model = m;
    }

    eprintln!(
        "→ {} @ {} (model: {})",
        config.provider_label, config.endpoint, config.default_model
    );

    let runner = OpenAiCompatRunner::new(config);

    let prelude = Prelude {
        sections: vec![],
        cache: CacheControl {
            key: "hok-smoke".into(),
            ttl: CacheTtl::Ephemeral,
        },
        engine: None,
        think: None,
        estimated_tokens: 0,
        ring_depth: 0.0,
        truncated: false,
    };

    let session_id = runner.start(prelude, "hok-smoke").await?;
    eprintln!("→ session {} started", session_id.as_str());

    let mut stream = runner
        .send(
            &session_id,
            "Reply with exactly the words: HOk smoke OK".into(),
        )
        .await?;

    let mut got_turn_ended = false;
    let mut turn_failed_message: Option<String> = None;
    while let Some(event) = stream.next().await {
        println!("{}", serde_json::to_string(&event)?);
        match event {
            runner::AgentEvent::TurnEnded { .. } => got_turn_ended = true,
            runner::AgentEvent::TurnFailed { message, .. } => {
                turn_failed_message = Some(message);
            }
            _ => {}
        }
    }

    runner.stop(&session_id).await?;

    if let Some(msg) = turn_failed_message {
        eprintln!("✗ turn failed: {msg}");
        std::process::exit(1);
    }
    if !got_turn_ended {
        eprintln!("✗ stream ended without TurnEnded");
        std::process::exit(1);
    }
    eprintln!("✓ HOk round-trip OK");
    Ok(())
}
