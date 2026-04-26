//! @arch:layer(arch)
//! @arch:role(ticket)
//!
//! Design-comment capture, parallel to `summary.rs`.
//!
//! Comments are durable design notes attached to a relay (or to the inbox).
//! Distinction from summaries: a summary describes *progress* and is
//! promotable to a relay; a comment describes *design intent / rationale*
//! and is not part of the inbox flow. Use this from /refine and /design
//! when you want to record "why we're doing it this way" without conflating
//! it with "here's what I just did".
//!
//! Storage: `.hack/comments/{relay}/{timestamp}-{author}.md` (relay-scoped)
//! or `.hack/comments/_inbox/{timestamp}-{author}.md` (no relay).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,

    /// Relay (or compound ticket) the comment is attached to. None = inbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    pub timestamp: u64,

    pub text: String,

    pub file: PathBuf,
}

pub fn write_comment(
    workspace: &Path,
    text: &str,
    relay: Option<&str>,
    author: Option<&str>,
) -> Result<Comment> {
    let comments_dir = workspace.join(".hack").join("comments");
    let bucket = relay.unwrap_or("_inbox");
    let dir = comments_dir.join(bucket);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create {}", dir.display()))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let author_slug = author.map(slugify).unwrap_or_else(|| "anon".into());
    let filename = format!("{}-{}.md", timestamp, author_slug);
    let file_path = dir.join(&filename);
    let id = format!("{}/{}", bucket, file_path.file_stem().unwrap().to_string_lossy());

    let mut fm_lines = Vec::new();
    if let Some(r) = relay {
        fm_lines.push(format!("relay: {}", r));
    }
    if let Some(a) = author {
        fm_lines.push(format!("author: {}", a));
    }
    fm_lines.push(format!("timestamp: {}", timestamp));

    let content = format!("---\n{}\n---\n\n{}\n", fm_lines.join("\n"), text);
    std::fs::write(&file_path, &content)
        .with_context(|| format!("Failed to write {}", file_path.display()))?;

    Ok(Comment {
        id,
        relay: relay.map(|s| s.to_string()),
        author: author.map(|s| s.to_string()),
        timestamp,
        text: text.to_string(),
        file: file_path,
    })
}

pub fn read_comments(workspace: &Path) -> Result<Vec<Comment>> {
    let comments_dir = workspace.join(".hack").join("comments");
    if !comments_dir.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for bucket_entry in std::fs::read_dir(&comments_dir)? {
        let bucket_entry = bucket_entry?;
        if !bucket_entry.file_type()?.is_dir() {
            continue;
        }
        let bucket = bucket_entry.file_name().to_string_lossy().to_string();
        for file_entry in std::fs::read_dir(bucket_entry.path())? {
            let file_entry = file_entry?;
            let path = file_entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                match read_comment_file(&path, &bucket) {
                    Ok(c) => out.push(c),
                    Err(e) => eprintln!("Warning: Failed to read {}: {}", path.display(), e),
                }
            }
        }
    }

    out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(out)
}

pub fn comments_for_relay<'a>(comments: &'a [Comment], relay_id: &str) -> Vec<&'a Comment> {
    comments
        .iter()
        .filter(|c| c.relay.as_deref() == Some(relay_id))
        .collect()
}

fn read_comment_file(path: &Path, bucket: &str) -> Result<Comment> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let id = format!("{}/{}", bucket, stem);

    let (relay, author, timestamp, body) = if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("---\n") {
            let yaml = &rest[..end];
            let body = rest[end + 4..].trim().to_string();
            let mut relay = None;
            let mut author = None;
            let mut timestamp: u64 = 0;
            for line in yaml.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    match k.trim() {
                        "relay" => relay = Some(v.trim().to_string()),
                        "author" => author = Some(v.trim().to_string()),
                        "timestamp" => timestamp = v.trim().parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            (relay, author, timestamp, body)
        } else {
            (None, None, 0, content.trim().to_string())
        }
    } else {
        (None, None, 0, content.trim().to_string())
    };

    let relay = relay.or_else(|| {
        if bucket == "_inbox" {
            None
        } else {
            Some(bucket.to_string())
        }
    });

    Ok(Comment {
        id,
        relay,
        author,
        timestamp,
        text: body,
        file: path.to_path_buf(),
    })
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn relay_scoped_comment_round_trips() {
        let tmp = TempDir::new().unwrap();
        let c = write_comment(
            tmp.path(),
            "Why we picked the shard layout: avoids merge conflicts on `.hack/events.jsonl`.",
            Some("R002"),
            Some("agent:claude"),
        )
        .unwrap();
        assert_eq!(c.relay.as_deref(), Some("R002"));
        assert!(c.file.starts_with(tmp.path().join(".hack/comments/R002")));

        let all = read_comments(tmp.path()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].relay.as_deref(), Some("R002"));
        assert!(all[0].text.contains("shard layout"));

        let r002 = comments_for_relay(&all, "R002");
        assert_eq!(r002.len(), 1);
    }

    #[test]
    fn inbox_comment_when_no_relay() {
        let tmp = TempDir::new().unwrap();
        let c = write_comment(tmp.path(), "Pre-relay design thought.", None, None).unwrap();
        assert!(c.relay.is_none());
        assert!(c.file.starts_with(tmp.path().join(".hack/comments/_inbox")));

        let all = read_comments(tmp.path()).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].relay.is_none());
    }
}
