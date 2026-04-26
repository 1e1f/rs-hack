//! Sibling-workspace registry for `board claim` anti-collision (R002 P1).
//!
//! See architecture/multi-worktree-sync.md. The registry lets a workspace
//! enumerate other clones / worktrees / SSH peers that may also be allocating
//! `@yah:` IDs, so `board claim` can union their existing IDs into its
//! `max(id) + 1` scan and avoid same-machine and cross-machine collisions.
//!
//! Three sibling kinds:
//!
//! - **`git`** — a `git worktree add` subtree sharing `.git/common-dir`.
//!   Auto-discovered from `git worktree list --porcelain`; never written to
//!   the registry file.
//! - **`local`** — a separate clone of the same repo at a different path on
//!   this machine. Listed in `.yah/worktrees.json`.
//! - **`remote`** — another machine reachable via SSH. Listed in
//!   `.yah/worktrees.json` with `{host, path}`. Queried via
//!   `ssh <host> yah board tickets -f json -p <path>`.
//!
//! The registry file is gitignored — every clone owns its own view.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One declared sibling workspace. Auto-discovered git worktrees are merged
/// in at enumeration time and never written to the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Sibling {
    /// Separate clone of the same repo at another local path.
    Local { path: PathBuf },
    /// `git worktree add` subtree sharing `.git/common-dir`. Usually
    /// auto-discovered; explicit entries are tolerated for symmetry.
    Git { path: PathBuf },
    /// SSH peer. `path` is the remote workspace path (may be `~`-relative).
    Remote { host: String, path: String },
}

impl Sibling {
    pub fn label(&self) -> String {
        match self {
            Sibling::Local { path } => format!("local:{}", path.display()),
            Sibling::Git { path } => format!("git:{}", path.display()),
            Sibling::Remote { host, path } => format!("remote:{}:{}", host, path),
        }
    }
}

/// Contents of `.yah/worktrees.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub siblings: Vec<Sibling>,
}

impl Registry {
    pub fn path(workspace: &Path) -> PathBuf {
        workspace.join(".yah").join("worktrees.json")
    }

    /// Load the registry. Returns an empty registry if the file doesn't exist.
    pub fn load(workspace: &Path) -> Result<Self> {
        let p = Self::path(workspace);
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&p)
            .with_context(|| format!("read {}", p.display()))?;
        let reg: Self = serde_json::from_str(&raw)
            .with_context(|| format!("parse {}", p.display()))?;
        Ok(reg)
    }

    /// Save (pretty-printed). Creates `.yah/` if needed.
    pub fn save(&self, workspace: &Path) -> Result<()> {
        let p = Self::path(workspace);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let body = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(&p, body).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }

    /// Add a sibling unless an equivalent entry already exists.
    /// Returns true if added, false if it was a duplicate.
    pub fn add(&mut self, sib: Sibling) -> bool {
        if self.siblings.iter().any(|s| s == &sib) {
            return false;
        }
        self.siblings.push(sib);
        true
    }

    /// Remove the first sibling matching `key` (path or `host:path`).
    /// Returns true if something was removed.
    pub fn remove(&mut self, key: &str) -> bool {
        let before = self.siblings.len();
        self.siblings.retain(|s| match s {
            Sibling::Local { path } | Sibling::Git { path } => {
                path.to_string_lossy() != key
            }
            Sibling::Remote { host, path } => {
                let combined = format!("{}:{}", host, path);
                combined != key && host != key && path != key
            }
        });
        before != self.siblings.len()
    }
}

/// Auto-discover sibling git worktrees by running `git worktree list --porcelain`.
/// Filters out the workspace itself. Returns empty on any git error (bare
/// clones, no git installed, etc).
pub fn discover_git_worktrees(workspace: &Path) -> Vec<Sibling> {
    let canon_self = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let out = match Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(workspace)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut results = Vec::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            let p = PathBuf::from(rest.trim());
            let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
            if canon == canon_self {
                continue;
            }
            results.push(Sibling::Git { path: canon });
        }
    }
    results
}

/// Union of registry entries + auto-discovered git worktrees, deduped by
/// canonical path (for local/git) or `host:path` (for remote).
pub fn enumerate(workspace: &Path) -> Result<Vec<Sibling>> {
    let reg = Registry::load(workspace)?;
    let auto = discover_git_worktrees(workspace);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<Sibling> = Vec::new();
    for sib in reg.siblings.into_iter().chain(auto.into_iter()) {
        let key = match &sib {
            Sibling::Local { path } | Sibling::Git { path } => {
                let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
                format!("path:{}", canon.display())
            }
            Sibling::Remote { host, path } => format!("ssh:{}:{}", host, path),
        };
        if seen.insert(key) {
            out.push(sib);
        }
    }
    Ok(out)
}

/// Query a sibling for the IDs of every `@yah:ticket` / `@yah:relay`
/// declaration in its source. Local/git siblings are scanned via subprocess
/// (`yah board tickets -f json -p <path>`); remote siblings via
/// `ssh <host> yah board tickets -f json -p <path>`.
///
/// Errors are non-fatal: returns `Ok(Err(message))` so the caller can warn
/// and proceed (per design doc — race resolution is P5's job).
pub fn scan_sibling_ids(sib: &Sibling) -> std::result::Result<Vec<String>, String> {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "yah".to_string());

    let output = match sib {
        Sibling::Local { path } | Sibling::Git { path } => Command::new(&exe)
            .args(["board", "tickets", "-f", "json", "-p"])
            .arg(path)
            .output()
            .map_err(|e| format!("spawn {}: {}", exe, e))?,
        Sibling::Remote { host, path } => Command::new("ssh")
            .arg(host)
            .arg(format!("yah board tickets -f json -p {}", shell_quote(path)))
            .output()
            .map_err(|e| format!("spawn ssh {}: {}", host, e))?,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "exit {}: {}",
            output.status,
            stderr.lines().next().unwrap_or("").trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ticket_ids(&stdout).map_err(|e| format!("parse: {}", e))
}

/// Pull `id` strings out of the `board tickets -f json` payload.
/// Tolerates both array-of-tickets shape and `"No tickets found"` non-JSON.
fn parse_ticket_ids(json: &str) -> Result<Vec<String>> {
    let trimmed = json.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(Vec::new());
    }
    let v: serde_json::Value = serde_json::from_str(trimmed)
        .with_context(|| "non-JSON output from `board tickets`")?;
    let arr = v.as_array().ok_or_else(|| {
        anyhow::anyhow!("expected JSON array, got {}", v)
    })?;
    let mut ids = Vec::with_capacity(arr.len());
    for item in arr {
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// Minimal POSIX-ish shell quoting for the remote `path` argument.
fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/_.-~".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Union the IDs of every reachable sibling. Warnings are written to stderr
/// per failed sibling; reachable siblings still contribute their IDs.
pub fn union_sibling_ids(workspace: &Path) -> Result<BTreeSet<String>> {
    let mut all: BTreeSet<String> = BTreeSet::new();
    let siblings = enumerate(workspace)?;
    for sib in &siblings {
        match scan_sibling_ids(sib) {
            Ok(ids) => {
                for id in ids {
                    all.insert(id);
                }
            }
            Err(msg) => {
                eprintln!(
                    "warning: sibling {} unreachable — allocated ID may collide if \
                     that workspace has claimed recently ({})",
                    sib.label(),
                    msg
                );
            }
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn registry_round_trip() {
        let dir = tempdir().unwrap();
        let ws = dir.path();
        let mut reg = Registry::default();
        assert!(reg.add(Sibling::Local { path: PathBuf::from("/tmp/foo") }));
        assert!(reg.add(Sibling::Remote {
            host: "leif@workmac".into(),
            path: "~/ss/nt".into(),
        }));
        // Duplicate is a no-op.
        assert!(!reg.add(Sibling::Local { path: PathBuf::from("/tmp/foo") }));
        reg.save(ws).unwrap();

        let loaded = Registry::load(ws).unwrap();
        assert_eq!(loaded.siblings.len(), 2);
    }

    #[test]
    fn registry_remove_by_path() {
        let mut reg = Registry::default();
        reg.add(Sibling::Local { path: PathBuf::from("/tmp/foo") });
        reg.add(Sibling::Local { path: PathBuf::from("/tmp/bar") });
        assert!(reg.remove("/tmp/foo"));
        assert_eq!(reg.siblings.len(), 1);
        assert!(!reg.remove("/tmp/foo"));
    }

    #[test]
    fn registry_remove_by_remote_combo() {
        let mut reg = Registry::default();
        reg.add(Sibling::Remote {
            host: "leif@workmac".into(),
            path: "~/ss/nt".into(),
        });
        assert!(reg.remove("leif@workmac:~/ss/nt"));
        assert_eq!(reg.siblings.len(), 0);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let reg = Registry::load(dir.path()).unwrap();
        assert!(reg.siblings.is_empty());
    }

    #[test]
    fn parses_ticket_json() {
        let json = r#"[{"id":"R001","title":"x"},{"id":"T03"}]"#;
        let ids = parse_ticket_ids(json).unwrap();
        assert_eq!(ids, vec!["R001", "T03"]);
    }

    #[test]
    fn parses_empty_array() {
        assert!(parse_ticket_ids("[]").unwrap().is_empty());
        assert!(parse_ticket_ids("").unwrap().is_empty());
    }

    #[test]
    fn shell_quote_passthrough() {
        assert_eq!(shell_quote("/tmp/foo"), "/tmp/foo");
        assert_eq!(shell_quote("~/ss/nt"), "~/ss/nt");
    }

    #[test]
    fn shell_quote_escapes_spaces() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("don't"), "'don'\\''t'");
    }
}
