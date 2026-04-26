//! @arch:layer(arch)
//! @arch:role(ticket)
//! @arch:see(architecture/hack-board.md)
//!
//! Promote a freeform summary into a structured `@hack:relay(...)` annotation
//! in source. This is the shared backend for both:
//!
//! - The CLI: `rs-hack board promote --summary <ID> --file <PATH>`
//! - The MCP tool: `hack_promote`
//! - The HTTP endpoint: `POST /api/promote/<id>` (shells out to the CLI)
//!
//! Allocation is serialized through `.hack/id.lock`, so concurrent promoters
//! and `board claim` invocations cannot collide on the next R-number.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::extract::extract_from_workspace;
use crate::summary::read_summaries;
use crate::ticket::TicketBoard;

/// Result of a successful promotion.
#[derive(Debug, Clone)]
pub struct PromoteResult {
    /// Allocated relay ID, e.g. "R012".
    pub relay_id: String,
    /// Title written into the annotation (truncated first line of the summary).
    pub relay_title: String,
    /// Source file the annotation was written to (workspace-relative when possible).
    pub file: PathBuf,
    /// 1-indexed line of the first annotation line in the new file.
    pub line: usize,
    /// Path to the summary file whose frontmatter was updated.
    pub summary_file: PathBuf,
}

/// Promote a summary to an in-source relay.
///
/// Steps:
/// 1. Acquire the workspace ID lock.
/// 2. Read the summary by id.
/// 3. Allocate the next bare R-number across the workspace.
/// 4. Write a `@hack:relay(...)` annotation block to `target_file`.
/// 5. Update the summary frontmatter: `promoted: true`, `relay_id`, `relay_title`.
///
/// If `title_override` is `None`, the relay title is taken from the first
/// non-blank line of the summary body, truncated to 80 chars.
///
/// `target_file` must be a `.rs` file (the only kind the extractor scans
/// today; see R001-T2 for the non-Rust expansion).
pub fn promote_summary(
    workspace: &Path,
    summary_id: &str,
    target_file: &Path,
    title_override: Option<&str>,
    assignee: Option<&str>,
) -> Result<PromoteResult> {
    let workspace = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());

    // Resolve target relative to workspace and validate.
    let target = if target_file.is_absolute() {
        target_file.to_path_buf()
    } else {
        workspace.join(target_file)
    };
    if !target.exists() {
        bail!("Target file does not exist: {}", target.display());
    }
    match target.extension().and_then(|e| e.to_str()) {
        Some("rs") => {}
        Some(ext) => bail!(
            "promote only supports .rs files; .{} is not scanned by the extractor yet \
             (see R001-T2). Anchor the relay on a .rs file.",
            ext
        ),
        None => bail!(
            "promote requires a file with a .rs extension; got {}",
            target.display()
        ),
    }
    let target = std::fs::canonicalize(&target).unwrap_or(target);

    // Find the summary.
    let summaries = read_summaries(&workspace).context("Failed to read summaries")?;
    let summary = summaries
        .iter()
        .find(|s| s.id == summary_id)
        .ok_or_else(|| anyhow::anyhow!("Summary '{}' not found in .hack/summaries/", summary_id))?;
    if summary.promoted {
        bail!("Summary '{}' is already promoted", summary_id);
    }

    // Pick the relay title.
    let title = match title_override {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => first_line_title(&summary.text),
    };

    // Lock + allocate + write inside the same critical section.
    let _lock = IdLock::acquire(&workspace)?;

    let annotations = extract_from_workspace(&workspace)
        .context("Failed to scan workspace for existing IDs")?;
    let board = TicketBoard::from_annotations(&annotations);
    let relay_id = next_relay_id(&board);

    // Build a relay annotation block. Status defaults to `handoff` so the
    // promoted relay lands in the Handoff column with a working baton.
    let handoff_msg = first_paragraph(&summary.text);
    let block = build_relay_annotation_block(
        &relay_id,
        &title,
        Some("handoff"),
        assignee,
        if handoff_msg.is_empty() {
            &[]
        } else {
            std::slice::from_ref(&handoff_msg)
        },
    );

    let original = std::fs::read_to_string(&target)
        .with_context(|| format!("Failed to read {}", target.display()))?;
    let (new_content, line) = insert_module_doc_block(&original, &block);
    std::fs::write(&target, new_content)
        .with_context(|| format!("Failed to write {}", target.display()))?;

    // Update summary frontmatter (best-effort; the annotation is the source of truth).
    if let Err(e) = update_summary_frontmatter(&summary.file, &relay_id, &title) {
        eprintln!(
            "warning: wrote relay {} but failed to update summary frontmatter: {}",
            relay_id, e
        );
    }

    let rel_file = target
        .strip_prefix(&workspace)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| target.clone());

    Ok(PromoteResult {
        relay_id,
        relay_title: title,
        file: rel_file,
        line,
        summary_file: summary.file.clone(),
    })
}

/// File lock held during ID allocation. Created at `.hack/id.lock`; released
/// on drop. Mirrors the lock used by `rs-hack board claim` so the two
/// allocators serialize against each other.
pub struct IdLock {
    path: PathBuf,
}

impl IdLock {
    pub fn acquire(workspace: &Path) -> Result<Self> {
        let hack_dir = workspace.join(".hack");
        std::fs::create_dir_all(&hack_dir)?;
        let path = hack_dir.join("id.lock");
        let start = std::time::Instant::now();
        let mut delay_ms = 10u64;
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = writeln!(
                        f,
                        "pid={} claimed_at={}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                    );
                    return Ok(IdLock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if start.elapsed() > std::time::Duration::from_secs(5) {
                        bail!(
                            "Another rs-hack process is holding {}; waited 5s. \
                             Delete the lock file if stale.",
                            path.display()
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    delay_ms = (delay_ms * 2).min(200);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Drop for IdLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Pick the next bare relay ID (`R001`, `R002`, …). Skips compound IDs.
pub fn next_relay_id(board: &TicketBoard) -> String {
    let max: u32 = board
        .tickets
        .iter()
        .filter_map(|t| {
            if t.id.contains('-') {
                return None;
            }
            t.id.strip_prefix('R').and_then(|n| n.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0);
    format!("R{:03}", max + 1)
}

fn first_line_title(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim_start_matches(['#', ' ', '\t']).trim())
        .find(|l| !l.is_empty())
        .unwrap_or("Untitled relay");
    line.chars().take(80).collect()
}

fn first_paragraph(text: &str) -> String {
    let mut out = String::new();
    let mut started = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        if started {
            out.push(' ');
        }
        out.push_str(trimmed);
        started = true;
    }
    out
}

/// Build a `//! @hack:relay(...)` annotation block.
fn build_relay_annotation_block(
    id: &str,
    title: &str,
    status: Option<&str>,
    assignee: Option<&str>,
    handoff: &[String],
) -> String {
    let mut lines = Vec::<String>::new();
    lines.push(format!("//! @hack:relay({}, {:?})", id, title));
    if let Some(s) = status {
        lines.push(format!("//! @hack:status({})", s));
    }
    if let Some(a) = assignee {
        lines.push(format!("//! @hack:assignee({})", a));
    }
    for h in handoff {
        lines.push(format!("//! @hack:handoff({:?})", h));
    }
    lines.join("\n")
}

/// Insert `block` into `content` as a module-level doc comment. If the file
/// already starts with `//!` lines, append the block to that run (separated
/// by a blank `//!` line). Otherwise prepend at the top.
/// Returns the new content and the 1-indexed line of the block's first line.
pub fn insert_module_doc_block(content: &str, block: &str) -> (String, usize) {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut head_end = 0usize;
    while head_end < lines.len() {
        if lines[head_end].trim_start().starts_with("//!") {
            head_end += 1;
        } else {
            break;
        }
    }

    if head_end > 0 {
        let mut new_lines: Vec<String> = lines[..head_end].iter().map(|s| s.to_string()).collect();
        new_lines.push("//!".to_string());
        let insert_start_line = new_lines.len() + 1;
        for l in block.split('\n') {
            new_lines.push(l.to_string());
        }
        for l in &lines[head_end..] {
            new_lines.push((*l).to_string());
        }
        (new_lines.join("\n"), insert_start_line)
    } else {
        let mut out = String::new();
        out.push_str(block);
        out.push_str("\n\n");
        out.push_str(content);
        (out, 1)
    }
}

/// Write `relay_id` and `relay_title` into the summary frontmatter.
fn update_summary_frontmatter(path: &Path, relay_id: &str, relay_title: &str) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Locate frontmatter block.
    let Some(rest) = content.strip_prefix("---\n") else {
        bail!("Summary {} has no frontmatter", path.display());
    };
    let Some(end) = rest.find("\n---\n") else {
        bail!("Summary {} frontmatter is unterminated", path.display());
    };
    let yaml = &rest[..end];
    let body = &rest[end + 5..];

    let mut promoted_seen = false;
    let mut relay_id_seen = false;
    let mut relay_title_seen = false;
    let mut new_yaml = String::new();
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("promoted:") {
            let _ = rest;
            new_yaml.push_str("promoted: true\n");
            promoted_seen = true;
        } else if trimmed.starts_with("relay_id:") {
            new_yaml.push_str(&format!("relay_id: {}\n", relay_id));
            relay_id_seen = true;
        } else if trimmed.starts_with("relay_title:") {
            new_yaml.push_str(&format!("relay_title: {}\n", relay_title));
            relay_title_seen = true;
        } else {
            new_yaml.push_str(line);
            new_yaml.push('\n');
        }
    }
    if !promoted_seen {
        new_yaml.push_str("promoted: true\n");
    }
    if !relay_id_seen {
        new_yaml.push_str(&format!("relay_id: {}\n", relay_id));
    }
    if !relay_title_seen {
        new_yaml.push_str(&format!("relay_title: {}\n", relay_title));
    }

    let new_content = format!("---\n{}---\n{}", new_yaml, body);
    std::fs::write(path, new_content)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::write_summary;
    use tempfile::TempDir;

    fn write_target(workspace: &Path, content: &str) -> PathBuf {
        let path = workspace.join("src");
        std::fs::create_dir_all(&path).unwrap();
        let f = path.join("lib.rs");
        std::fs::write(&f, content).unwrap();
        f
    }

    #[test]
    fn test_promote_writes_relay_annotation_and_marks_summary() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();

        write_target(
            ws,
            "//! existing module doc\n//! more docs\n\npub fn hello() {}\n",
        );
        let s = write_summary(ws, "Refactor extract pipeline\n\nMore details", None, Some("agent:claude"))
            .unwrap();

        let res = promote_summary(ws, &s.id, Path::new("src/lib.rs"), None, Some("agent:claude")).unwrap();
        assert!(res.relay_id.starts_with('R'));
        assert_eq!(res.relay_title, "Refactor extract pipeline");

        let content = std::fs::read_to_string(ws.join("src/lib.rs")).unwrap();
        assert!(content.contains(&format!(
            "@hack:relay({}, \"Refactor extract pipeline\")",
            res.relay_id
        )));
        assert!(content.contains("@hack:status(handoff)"));
        assert!(content.contains("existing module doc"));

        // Summary frontmatter updated.
        let updated = std::fs::read_to_string(&res.summary_file).unwrap();
        assert!(updated.contains("promoted: true"));
        assert!(updated.contains(&format!("relay_id: {}", res.relay_id)));
    }

    #[test]
    fn test_promote_allocates_next_id() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        write_target(
            ws,
            "//! @hack:relay(R005, \"existing\")\n//! @hack:status(handoff)\n\npub fn x() {}\n",
        );
        let s = write_summary(ws, "Brand new work", None, None).unwrap();
        let res = promote_summary(ws, &s.id, Path::new("src/lib.rs"), None, None).unwrap();
        assert_eq!(res.relay_id, "R006");
    }

    #[test]
    fn test_promote_rejects_non_rust_file() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let f = ws.join("notes.md");
        std::fs::write(&f, "# notes\n").unwrap();
        let s = write_summary(ws, "x", None, None).unwrap();
        let err = promote_summary(ws, &s.id, Path::new("notes.md"), None, None).unwrap_err();
        assert!(err.to_string().contains(".md"));
    }

    #[test]
    fn test_promote_double_fails() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        write_target(ws, "pub fn x() {}\n");
        let s = write_summary(ws, "Once", None, None).unwrap();
        promote_summary(ws, &s.id, Path::new("src/lib.rs"), None, None).unwrap();
        let err = promote_summary(ws, &s.id, Path::new("src/lib.rs"), None, None).unwrap_err();
        assert!(err.to_string().contains("already promoted"));
    }
}
