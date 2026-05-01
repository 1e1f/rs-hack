//! @arch:layer(cli)
//! @arch:role(bridge)
//!
//! `yah agent run` — headless CLI driving the `runner` crate.
//!
//! Phase 1 of R032-T3 (yah-agentd). Goal: prove the agent loop runs
//! end-to-end without Tauri, using T1's [`crate::keys::KeysStore`] for
//! credentials and `runner`'s [`OpenAiCompatRunner`] for the HTTP wire.
//! When this works, yah-agentd is "host the same code in a Unix-socket
//! JSON-RPC server" — same `OpenAiCompatRunner`, same key lookup,
//! `SessionEventSink` swapped for a notification pusher.
//!
//! Wire shape:
//! - SessionStarted → stderr (one informational line)
//! - MessageDelta   → stdout (raw text, no newline)
//! - TurnEnded      → stdout single trailing newline + stderr summary
//! - TurnFailed     → stderr error line + non-zero exit
//! - Error          → stderr + non-zero exit
//!
//! Hello-world (no token, needs `ollama serve` on the box):
//!
//! ```text
//! yah agent run --provider ollama --model qwen2.5-coder:1.5b 'say hi'
//! ```

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use futures_util::StreamExt;
use kg::agent::AgentEvent;
use kg::prelude::{CacheControl, CacheTtl, Prelude};
use runner::{OpenAiCompatConfig, OpenAiCompatRunner, Runner};
use std::io::{IsTerminal, Read, Write};

use crate::keys::KeysStore;

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Run a one-shot agent turn and stream the reply to stdout.
    ///
    /// Default provider is `ollama` (loopback `ollama serve`, no token).
    /// `openai` reads its token from the `yah keys` vault under the
    /// `openai` slot. The Anthropic-native runner lives in the desktop
    /// host (`app/yah/desktop/src/agent.rs`) and is not yet exposed as
    /// a CLI subcommand — until that path lifts into `runner`, this
    /// subcommand intentionally errors on `--provider anthropic`.
    Run {
        /// Provider preset: `ollama` (default), `openai`.
        #[arg(long, default_value = "ollama")]
        provider: String,

        /// Override the model id. Falls back to the provider's default
        /// (`llama3.3` for ollama, `gpt-4o` for openai).
        #[arg(long)]
        model: Option<String>,

        /// Override the chat-completions endpoint URL. Useful for
        /// pointing at a remote ollama or any OpenAI-compat gateway.
        #[arg(long)]
        endpoint: Option<String>,

        /// User message. Omit to read from stdin (recommended for
        /// multi-line prompts and shell-history hygiene).
        prompt: Option<String>,
    },
}

pub fn handle_agent_command(cmd: AgentCommands) -> Result<()> {
    match cmd {
        AgentCommands::Run {
            provider,
            model,
            endpoint,
            prompt,
        } => {
            let prompt = resolve_prompt(prompt)?;
            let config = build_config(&provider, model, endpoint)?;
            let rt = tokio::runtime::Runtime::new()
                .context("failed to start tokio runtime for `yah agent run`")?;
            rt.block_on(run_one_shot(config, prompt))
        }
    }
}

fn resolve_prompt(arg: Option<String>) -> Result<String> {
    if let Some(p) = arg {
        if p.is_empty() {
            bail!("prompt is empty");
        }
        return Ok(p);
    }
    if std::io::stdin().is_terminal() {
        bail!("no prompt provided — pass it as a positional arg or pipe via stdin");
    }
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    let trimmed = s.trim_end_matches(['\n', '\r']).to_string();
    if trimmed.is_empty() {
        bail!("stdin prompt is empty");
    }
    Ok(trimmed)
}

fn build_config(
    provider: &str,
    model: Option<String>,
    endpoint: Option<String>,
) -> Result<OpenAiCompatConfig> {
    let mut cfg = match provider {
        "ollama" => OpenAiCompatConfig::ollama_local(),
        "openai" => {
            let store = KeysStore::open()?;
            let token = store
                .get("openai")?
                .ok_or_else(|| anyhow::anyhow!(
                    "no openai token in keychain — set one with: yah keys set openai"
                ))?;
            OpenAiCompatConfig::openai(token)
        }
        "anthropic" | "claude" | "crab" => {
            bail!(
                "anthropic/claude is not yet wired into the headless CLI — \
                 the Anthropic-native runner lives in the desktop host \
                 (app/yah/desktop/src/agent.rs). Use --provider ollama or openai \
                 for now; lifting the Anthropic path into `runner` is part of \
                 R032's session-state lift follow-up."
            );
        }
        other => bail!(
            "unknown provider: {other:?} — known: ollama, openai (anthropic via desktop)"
        ),
    };
    if let Some(m) = model {
        cfg.default_model = m;
    }
    if let Some(e) = endpoint {
        cfg.endpoint = e;
    }
    Ok(cfg)
}

fn empty_prelude() -> Prelude {
    Prelude {
        sections: Vec::new(),
        cache: CacheControl {
            key: "yah-agent-run-cli".into(),
            ttl: CacheTtl::Ephemeral,
        },
        engine: None,
        think: None,
        estimated_tokens: 0,
        ring_depth: 0.0,
        truncated: false,
    }
}

async fn run_one_shot(config: OpenAiCompatConfig, prompt: String) -> Result<()> {
    let runner = OpenAiCompatRunner::new(config);
    let prelude = empty_prelude();
    let session_id = runner
        .start(prelude, "cli")
        .await
        .map_err(|e| anyhow::anyhow!("runner start failed: {e}"))?;
    eprintln!("==> session {} started", session_id.0);

    let mut stream = runner
        .send(&session_id, prompt)
        .await
        .map_err(|e| anyhow::anyhow!("runner send failed: {e}"))?;

    let mut stdout = std::io::stdout().lock();
    let mut exit_code = 0;
    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::SessionStarted { .. } | AgentEvent::TurnStarted { .. } => {}
            AgentEvent::MessageDelta { text, .. } => {
                stdout.write_all(text.as_bytes())?;
                stdout.flush()?;
            }
            AgentEvent::TurnEnded {
                stop_reason, usage, ..
            } => {
                stdout.write_all(b"\n")?;
                stdout.flush()?;
                eprintln!(
                    "==> turn ended (stop_reason={}, in/out tokens={}/{})",
                    stop_reason.as_deref().unwrap_or("none"),
                    usage
                        .as_ref()
                        .and_then(|u| u.input_tokens)
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into()),
                    usage
                        .as_ref()
                        .and_then(|u| u.output_tokens)
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "?".into()),
                );
            }
            AgentEvent::TurnFailed { message, text, .. } => {
                if !text.is_empty() {
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
                eprintln!("==> turn failed: {message}");
                exit_code = 1;
            }
            AgentEvent::Error { message, .. } => {
                eprintln!("==> error: {message}");
                exit_code = 1;
            }
            _ => {}
        }
    }

    let _ = runner.stop(&session_id).await;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
