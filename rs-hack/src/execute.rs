//! Silent execution of an `Operation` across a set of files.
//!
//! Returns a structured `ExecuteResult` with everything a caller needs to render output, make
//! decisions, or surface errors. No `println!`/`eprintln!` — embedders (MCP server, yah, tests)
//! decide what to display; the CLI in `main.rs` wraps these calls with its own renderer.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::editor::RustEditor;
use crate::operations::{BackupNode, Operation};
use crate::state::{
    generate_run_id, get_state_dir, hash_file, save_backup_nodes, save_run_metadata,
    FileModification, RunMetadata, RunStatus,
};

#[derive(Debug, Clone, Default)]
pub struct ExecuteOpts {
    /// When true, write modified files. When false, perform a dry run.
    pub apply: bool,
    /// Optional override of the destination path. Only meaningful with a single input file.
    pub output: Option<PathBuf>,
    /// Stop after this many modifications across all files.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub old_content: String,
    pub new_content: String,
    pub modified_nodes: Vec<BackupNode>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub changes: Vec<FileChange>,
    pub total_modifications: usize,
    pub unmatched_qualified_paths: HashMap<String, usize>,
    pub parse_errors: Vec<(PathBuf, String)>,
    /// Last per-file apply error from a multi-file run (single-file errors bubble up).
    pub last_error: Option<String>,
    pub limit_hit: bool,
    /// Set when `execute_with_state` applied changes successfully.
    pub run_id: Option<String>,
    /// Per-file metadata captured for state tracking. Empty for `execute()`.
    pub files_modified: Vec<FileModification>,
}

/// Apply `op` across `files` without printing anything. When `opts.apply` is
/// true, writes modified files in place (or to `opts.output` if set);
/// otherwise performs a dry run and only fills the result.
pub fn execute(files: &[PathBuf], op: &Operation, opts: &ExecuteOpts) -> Result<ExecuteResult> {
    let mut result = ExecuteResult::default();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = match RustEditor::new(&content) {
            Ok(editor) => editor,
            Err(e) => {
                if files.len() == 1 {
                    return Err(e)
                        .with_context(|| format!("Failed to parse {}", file_path.display()));
                }
                result
                    .parse_errors
                    .push((file_path.clone(), format!("{}", e)));
                continue;
            }
        };

        match editor.apply_operation(op) {
            Ok(op_result) => {
                if let Some(unmatched) = op_result.unmatched_qualified_paths {
                    for (path, count) in unmatched {
                        *result.unmatched_qualified_paths.entry(path).or_insert(0) += count;
                    }
                }

                if op_result.changed {
                    result.total_modifications += op_result.modified_nodes.len();
                    let new_content = editor.to_string();

                    if opts.apply {
                        let write_path = opts.output.as_ref().unwrap_or(file_path);
                        std::fs::write(write_path, &new_content)
                            .with_context(|| format!("Failed to write {}", write_path.display()))?;
                    }

                    result.changes.push(FileChange {
                        path: file_path.clone(),
                        old_content: content,
                        new_content,
                        modified_nodes: op_result.modified_nodes,
                    });

                    if let Some(limit) = opts.limit
                        && result.total_modifications >= limit {
                            result.limit_hit = true;
                            break;
                        }
                }
            }
            Err(e) => {
                if files.len() == 1 {
                    return Err(e);
                }
                result.last_error = Some(format!("{}", e));
            }
        }
    }

    Ok(result)
}

/// Like `execute` but records a revertible run.
///
/// Falls back to plain `execute` if `apply` is false or `output` is set (state tracking only
/// applies to in-place writes). On success, populates `run_id` and `files_modified`.
///
/// `command_line` is stored verbatim in the run metadata so users can recall
/// what produced a given run; pass `String::new()` if the caller has no
/// meaningful command line to report.
pub fn execute_with_state(
    files: &[PathBuf],
    op: &Operation,
    opts: &ExecuteOpts,
    local_state: bool,
    command_line: String,
) -> Result<ExecuteResult> {
    if !opts.apply || opts.output.is_some() {
        return execute(files, op, opts);
    }

    let run_id = generate_run_id();
    let state_dir = get_state_dir(local_state)?;
    let mut result = ExecuteResult::default();

    for file_path in files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let mut editor = match RustEditor::new(&content) {
            Ok(editor) => editor,
            Err(e) => {
                if files.len() == 1 {
                    return Err(e)
                        .with_context(|| format!("Failed to parse {}", file_path.display()));
                }
                result
                    .parse_errors
                    .push((file_path.clone(), format!("{}", e)));
                continue;
            }
        };

        match editor.apply_operation(op) {
            Ok(op_result) => {
                if let Some(unmatched) = op_result.unmatched_qualified_paths {
                    for (path, count) in unmatched {
                        *result.unmatched_qualified_paths.entry(path).or_insert(0) += count;
                    }
                }

                if op_result.changed {
                    result.total_modifications += op_result.modified_nodes.len();
                    let new_content = editor.to_string();

                    let hash_before = hash_file(file_path)?;
                    save_backup_nodes(file_path, &op_result.modified_nodes, &run_id, &state_dir)?;

                    std::fs::write(file_path, &new_content)
                        .with_context(|| format!("Failed to write {}", file_path.display()))?;

                    let hash_after = hash_file(file_path)?;

                    result.files_modified.push(FileModification {
                        path: file_path.clone(),
                        hash_before,
                        hash_after,
                        backup_nodes: op_result.modified_nodes.clone(),
                    });
                    result.changes.push(FileChange {
                        path: file_path.clone(),
                        old_content: content,
                        new_content,
                        modified_nodes: op_result.modified_nodes,
                    });

                    if let Some(limit) = opts.limit
                        && result.total_modifications >= limit {
                            result.limit_hit = true;
                            break;
                        }
                }
            }
            Err(e) => {
                if files.len() == 1 {
                    return Err(e);
                }
                result.last_error = Some(format!("{}", e));
            }
        }
    }

    if !result.files_modified.is_empty() {
        let metadata = RunMetadata {
            run_id: run_id.clone(),
            timestamp: chrono::Utc::now(),
            command: command_line,
            operation: op.kind_name().to_string(),
            files_modified: result.files_modified.clone(),
            status: RunStatus::Applied,
            can_revert: true,
        };
        save_run_metadata(&metadata, &state_dir)?;
        result.run_id = Some(run_id);
    }

    Ok(result)
}
