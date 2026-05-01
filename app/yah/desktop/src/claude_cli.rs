//! @arch:layer(host)
//! @arch:role(bridge)
//!
//! Subprocess + service liveness probes for the agent provider panel.
//!
//! Two probes today:
//!
//! * [`claude_cli_probe`] — runs `claude --version` to surface whether
//!   the bundled-or-system `claude` binary is reachable. Backs the
//!   "Claude (PVd)" card in `AgentProvidersPanel`. The PVd preset
//!   (R028-F8) wraps `claude` as a subprocess; this probe is the
//!   bootstrap signal that R028-F8's runtime has something to spawn.
//! * [`ollama_serve_probe`] — pings `localhost:11434/api/tags` so the
//!   Ollama card can flip its "Local mode (free)" pill from a hopeful
//!   default into "Local serve detected" when `ollama serve` is up.
//!   Today's Ollama fallback in agent.rs is silent — without this
//!   probe a user with no local serve gets a connection-refused only
//!   when they try to send their first turn.
//!
//! Both probes are read-only and side-effect-free. Neither touches
//! the keychain.

use serde::Serialize;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCliProbe {
    /// True iff `claude --version` exited successfully within the timeout.
    pub installed: bool,
    /// First line of stdout from `claude --version`. Format is up to the
    /// upstream CLI — we surface it verbatim so a user mismatching their
    /// bundled-claude version can spot the drift.
    pub version: Option<String>,
    /// Resolved path from `which claude`. `None` when not on PATH or
    /// `which` is unavailable. Useful for the bundled-vs-system question.
    pub path: Option<String>,
    /// Human-readable failure reason. Populated for spawn errors,
    /// non-zero exits, and timeouts; left as `None` on the happy path.
    pub error: Option<String>,
}

/// Probe the `claude` CLI by shelling out to `claude --version`. A
/// 3-second timeout protects the panel from hanging on a wedged binary.
#[tauri::command]
pub async fn claude_cli_probe() -> ClaudeCliProbe {
    let path = match Command::new("which").arg("claude").output().await {
        Ok(o) if o.status.success() => {
            let line = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        }
        _ => None,
    };

    let call = tokio::time::timeout(
        Duration::from_secs(3),
        Command::new("claude").arg("--version").output(),
    )
    .await;

    match call {
        Ok(Ok(out)) if out.status.success() => {
            let line = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            ClaudeCliProbe {
                installed: true,
                version: if line.is_empty() { None } else { Some(line) },
                path,
                error: None,
            }
        }
        Ok(Ok(out)) => ClaudeCliProbe {
            installed: false,
            version: None,
            path,
            error: Some(format!(
                "claude --version exited with status {}",
                out.status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".to_string())
            )),
        },
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => ClaudeCliProbe {
            installed: false,
            version: None,
            path,
            error: Some("`claude` not found on PATH".to_string()),
        },
        Ok(Err(e)) => ClaudeCliProbe {
            installed: false,
            version: None,
            path,
            error: Some(format!("could not invoke `claude`: {e}")),
        },
        Err(_) => ClaudeCliProbe {
            installed: false,
            version: None,
            path,
            error: Some("claude --version timed out after 3s".to_string()),
        },
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaServeProbe {
    /// True iff `localhost:11434/api/tags` answered 2xx within the timeout.
    pub running: bool,
    /// `None` for the common "nothing listening on 11434" case so the UI
    /// can render "not running" without a noisy error blob; populated
    /// only when the upstream answered with a non-success status.
    pub error: Option<String>,
}

/// Probe a local Ollama serve. 1.5s timeout matches the panel's
/// snappiness budget — anything slower would feel laggy on every
/// settings open.
#[tauri::command]
pub async fn ollama_serve_probe() -> OllamaServeProbe {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return OllamaServeProbe {
                running: false,
                error: Some(format!("http client init failed: {e}")),
            }
        }
    };
    match client.get("http://localhost:11434/api/tags").send().await {
        Ok(r) if r.status().is_success() => OllamaServeProbe {
            running: true,
            error: None,
        },
        Ok(r) => OllamaServeProbe {
            running: false,
            error: Some(format!("ollama serve returned status {}", r.status())),
        },
        Err(_) => OllamaServeProbe {
            running: false,
            error: None,
        },
    }
}
