//! @arch:layer(kg_store)
//! @arch:role(bridge)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! Per-rig agent runtime settings (R031-F5 production flip).
//!
//! Today this carries one flag — `agent_writers_enabled` — that gates
//! whether [`crate::agent::start_runner_session`] hands the runner the
//! [`crate::agent_tools::KgToolRegistry::with_experimental_writers`]
//! surface or the read-only one. The flag defaults to `false`: writers
//! exist behind the approval gate, but a fresh rig stays read-only
//! until the operator opts in via the Settings UI.
//!
//! Persisted under `<rig_root>/.yah/agent-settings.json` next to
//! `agent-approval-rules.json`. Same shape rules apply: missing or
//! malformed file falls back to defaults (writers disabled), since the
//! safe fallback is always the smaller surface.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Filename for the per-rig agent settings blob. Lives under `.yah/`
/// so it's checked in alongside the rest of yah's tool-namespace state.
pub const AGENT_SETTINGS_FILENAME: &str = "agent-settings.json";

/// Versioned envelope. Mirrors the shape of
/// [`crate::agent_approval::ApprovalRuleset`] so a future field
/// addition can land as `V2` without silently dropping older rigs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum AgentSettings {
    #[serde(rename = "1")]
    V1(AgentSettingsV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsV1 {
    /// When `true`, [`crate::agent::start_runner_session`] uses
    /// [`crate::agent_tools::KgToolRegistry::with_experimental_writers`]
    /// and write tools become reachable behind the approval gate. When
    /// `false` (default), the runner only sees the read-only surface
    /// and every write attempt 404s at the registry — the gate never
    /// runs.
    #[serde(default)]
    pub agent_writers_enabled: bool,
}

impl Default for AgentSettingsV1 {
    fn default() -> Self {
        Self {
            agent_writers_enabled: false,
        }
    }
}

impl AgentSettings {
    pub fn defaults() -> Self {
        Self::V1(AgentSettingsV1::default())
    }

    pub fn agent_writers_enabled(&self) -> bool {
        match self {
            Self::V1(v) => v.agent_writers_enabled,
        }
    }

    pub fn set_agent_writers_enabled(&mut self, enabled: bool) {
        match self {
            Self::V1(v) => v.agent_writers_enabled = enabled,
        }
    }
}

fn settings_path(rig_root: &Path) -> PathBuf {
    rig_root.join(".yah").join(AGENT_SETTINGS_FILENAME)
}

/// Read the per-rig settings file. Missing / malformed → defaults
/// (writers disabled). Errors are logged but never propagated — a
/// corrupt file shouldn't keep the agent from booting in read-only
/// mode.
pub fn load_or_default(rig_root: &Path) -> AgentSettings {
    let path = settings_path(rig_root);
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<AgentSettings>(&bytes) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "agent settings file malformed; using defaults (writers disabled)",
                );
                AgentSettings::defaults()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AgentSettings::defaults(),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "agent settings file read failed; using defaults (writers disabled)",
            );
            AgentSettings::defaults()
        }
    }
}

/// Persist `settings` to `<rig_root>/.yah/agent-settings.json` via
/// write-then-rename. Best-effort — IO errors are logged and swallowed
/// so a transient FS failure doesn't bubble up to the renderer's
/// settings toggle.
pub fn save(rig_root: &Path, settings: &AgentSettings) -> std::io::Result<()> {
    let path = settings_path(rig_root);
    let bytes = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_have_writers_disabled() {
        let s = AgentSettings::defaults();
        assert!(!s.agent_writers_enabled());
    }

    #[test]
    fn missing_file_loads_defaults() {
        let tmp = tempdir().unwrap();
        let s = load_or_default(tmp.path());
        assert!(!s.agent_writers_enabled());
    }

    #[test]
    fn round_trips_through_disk() {
        let tmp = tempdir().unwrap();
        let mut s = AgentSettings::defaults();
        s.set_agent_writers_enabled(true);
        save(tmp.path(), &s).unwrap();
        let loaded = load_or_default(tmp.path());
        assert!(loaded.agent_writers_enabled());
    }

    #[test]
    fn malformed_file_falls_back_to_defaults() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join(".yah");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(AGENT_SETTINGS_FILENAME), b"{not json").unwrap();
        let loaded = load_or_default(tmp.path());
        assert!(!loaded.agent_writers_enabled());
    }

    #[test]
    fn unknown_version_rejected_into_defaults() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join(".yah");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(AGENT_SETTINGS_FILENAME),
            br#"{"version":"99","agentWritersEnabled":true}"#,
        )
        .unwrap();
        let loaded = load_or_default(tmp.path());
        assert!(!loaded.agent_writers_enabled());
    }
}
