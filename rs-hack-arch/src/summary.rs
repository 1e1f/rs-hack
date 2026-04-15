//! @arch:layer(arch)
//! @arch:role(ticket)
//! @hack:ticket(T03, "Remove @anthropic-ai/sdk dep from hack-board package.json if unused")
//! @hack:parent(R001)
//! @hack:status(open)
//!
//! Agent summary capture and storage.
//! Summaries are freeform markdown blobs that agents produce naturally.
//! They live as sidecar files in `.hack/summaries/` and can be promoted
//! to structured relay tickets by the board or another agent.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// A summary written by an agent, stored as a markdown file with frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    /// Unique ID (derived from filename, e.g., "F003-1713100800")
    pub id: String,

    /// Ticket ID this summary is linked to (None = orphan/inbox)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,

    /// Who wrote this summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Unix timestamp when written
    pub timestamp: u64,

    /// The freeform markdown body
    pub text: String,

    /// File path where this summary is stored
    pub file: PathBuf,

    /// Whether this has been promoted to a relay ticket
    #[serde(default)]
    pub promoted: bool,
}

/// Frontmatter for summary files (simple key: value format).
struct SummaryFrontmatter {
    ticket: Option<String>,
    author: Option<String>,
    timestamp: u64,
    promoted: bool,
}

impl SummaryFrontmatter {
    fn parse(yaml: &str) -> Self {
        let mut fm = Self {
            ticket: None,
            author: None,
            timestamp: 0,
            promoted: false,
        };
        for line in yaml.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                match key {
                    "ticket" => fm.ticket = Some(value.to_string()),
                    "author" => fm.author = Some(value.to_string()),
                    "timestamp" => fm.timestamp = value.parse().unwrap_or(0),
                    "promoted" => fm.promoted = value == "true",
                    _ => {}
                }
            }
        }
        fm
    }
}

/// Write a summary to `.hack/summaries/`.
pub fn write_summary(
    workspace: &Path,
    text: &str,
    ticket: Option<&str>,
    author: Option<&str>,
) -> Result<Summary> {
    let summaries_dir = workspace.join(".hack").join("summaries");
    std::fs::create_dir_all(&summaries_dir)
        .context("Failed to create .hack/summaries/")?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let id = if let Some(tid) = ticket {
        format!("{}-{}", tid, timestamp)
    } else {
        format!("summary-{}", timestamp)
    };

    let filename = format!("{}.md", id);
    let file_path = summaries_dir.join(&filename);

    let frontmatter = SummaryFrontmatter {
        ticket: ticket.map(|s| s.to_string()),
        author: author.map(|s| s.to_string()),
        timestamp,
        promoted: false,
    };

    let mut fm_lines = Vec::new();
    if let Some(ref t) = frontmatter.ticket {
        fm_lines.push(format!("ticket: {}", t));
    }
    if let Some(ref a) = frontmatter.author {
        fm_lines.push(format!("author: {}", a));
    }
    fm_lines.push(format!("timestamp: {}", frontmatter.timestamp));
    fm_lines.push(format!("promoted: {}", frontmatter.promoted));

    let content = format!("---\n{}\n---\n\n{}\n", fm_lines.join("\n"), text);
    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write {}", file_path.display()))?;

    Ok(Summary {
        id,
        ticket: ticket.map(|s| s.to_string()),
        author: author.map(|s| s.to_string()),
        timestamp,
        text: text.to_string(),
        file: file_path,
        promoted: false,
    })
}

/// Read all summaries from `.hack/summaries/`.
pub fn read_summaries(workspace: &Path) -> Result<Vec<Summary>> {
    let summaries_dir = workspace.join(".hack").join("summaries");
    if !summaries_dir.exists() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();

    for entry in std::fs::read_dir(&summaries_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            match read_summary_file(&path) {
                Ok(summary) => summaries.push(summary),
                Err(e) => {
                    eprintln!("Warning: Failed to read {}: {}", path.display(), e);
                }
            }
        }
    }

    // Sort newest first
    summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(summaries)
}

/// Read a single summary file.
fn read_summary_file(path: &Path) -> Result<Summary> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Parse frontmatter
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("---\n") {
            let yaml = &rest[..end];
            let body = rest[end + 4..].trim().to_string();

            let fm = SummaryFrontmatter::parse(yaml);
            return Ok(Summary {
                id,
                ticket: fm.ticket,
                author: fm.author,
                timestamp: fm.timestamp,
                text: body,
                file: path.to_path_buf(),
                promoted: fm.promoted,
            });
        }
    }

    // No frontmatter - treat as plain text orphan
    Ok(Summary {
        id,
        ticket: None,
        author: None,
        timestamp: 0,
        text: content.trim().to_string(),
        file: path.to_path_buf(),
        promoted: false,
    })
}

/// Mark a summary as promoted (after converting to relay).
pub fn mark_promoted(path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)?;

    if content.contains("promoted: false") {
        let updated = content.replace("promoted: false", "promoted: true");
        std::fs::write(path, updated)?;
    } else if content.starts_with("---\n") {
        // Add promoted field
        let updated = content.replacen("---\n", "---\npromoted: true\n", 1);
        std::fs::write(path, updated)?;
    }

    Ok(())
}

/// Get summaries linked to a specific ticket.
pub fn summaries_for_ticket<'a>(summaries: &'a [Summary], ticket_id: &str) -> Vec<&'a Summary> {
    summaries
        .iter()
        .filter(|s| s.ticket.as_deref() == Some(ticket_id))
        .collect()
}

/// Get unlinked (orphan) summaries.
pub fn orphan_summaries(summaries: &[Summary]) -> Vec<&Summary> {
    summaries
        .iter()
        .filter(|s| s.ticket.is_none() && !s.promoted)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_and_read_summary() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path();

        let summary = write_summary(
            workspace,
            "Did the thing.\n\n- Changed foo.rs\n- Fixed the bug",
            Some("F003"),
            Some("agent:claude"),
        )
        .unwrap();

        assert!(summary.id.starts_with("F003-"));
        assert_eq!(summary.ticket.as_deref(), Some("F003"));
        assert!(summary.file.exists());

        let summaries = read_summaries(workspace).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].ticket.as_deref(), Some("F003"));
        assert!(summaries[0].text.contains("Changed foo.rs"));
    }

    #[test]
    fn test_orphan_summary() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path();

        write_summary(workspace, "I did some stuff", None, None).unwrap();

        let summaries = read_summaries(workspace).unwrap();
        let orphans = orphan_summaries(&summaries);
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].id.starts_with("summary-"));
    }

    #[test]
    fn test_mark_promoted() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path();

        let summary = write_summary(workspace, "Promote me", Some("F001"), None).unwrap();
        assert!(!summary.promoted);

        mark_promoted(&summary.file).unwrap();

        let summaries = read_summaries(workspace).unwrap();
        assert!(summaries[0].promoted);
    }
}
