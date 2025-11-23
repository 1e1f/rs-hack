use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc, Duration};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::operations::BackupNode;

/// Generates a short unique run ID (7 characters, like git)
pub fn generate_run_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let hash = blake3::hash(&timestamp.to_le_bytes());
    let hex = hash.to_hex();
    hex.as_str()[..7].to_string()
}

/// Get the state directory path
///
/// Priority order:
/// 1. Environment variable RS_HACK_STATE_DIR (highest priority)
/// 2. --local-state flag (uses ./.rs-hack)
/// 3. Global default (uses system data directory)
pub fn get_state_dir(local: bool) -> Result<PathBuf> {
    // Priority 1: Check environment variable
    if let Ok(custom_dir) = std::env::var("RS_HACK_STATE_DIR") {
        return Ok(PathBuf::from(custom_dir));
    }

    // Priority 2: Local state flag
    if local {
        // Use project-local .rs-hack directory
        let current_dir = std::env::current_dir()?;
        Ok(current_dir.join(".rs-hack"))
    } else {
        // Priority 3: Use user's home directory (default)
        let proj_dirs = ProjectDirs::from("com", "rs-hack", "rs-hack")
            .context("Could not determine project directories")?;
        Ok(proj_dirs.data_dir().to_path_buf())
    }
}

/// Compute blake3 hash of a file
pub fn hash_file(path: &Path) -> Result<String> {
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file for hashing: {}", path.display()))?;
    let hash = blake3::hash(&content);
    Ok(hash.to_hex().to_string())
}

/// File modification metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    pub path: PathBuf,
    pub hash_before: String,
    pub hash_after: String,
    pub backup_nodes: Vec<BackupNode>, // AST nodes that were modified
}

/// Status of a run
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Applied,
    Reverted,
}

/// Metadata about a single run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub run_id: String,
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub operation: String,
    pub files_modified: Vec<FileModification>,
    pub status: RunStatus,
    pub can_revert: bool,
}

/// Index of all runs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunsIndex {
    pub runs: HashMap<String, RunMetadata>,
}

impl RunsIndex {
    pub fn load(state_dir: &Path) -> Result<Self> {
        let index_path = state_dir.join("runs.json");
        if !index_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&index_path)
            .context("Failed to read runs index")?;

        let index: RunsIndex = serde_json::from_str(&content)
            .map_err(|e| {
                if e.to_string().contains("missing field") {
                    eprintln!("⚠️  Incompatible state format detected from previous rs-hack version.");
                    eprintln!("   The state directory will be reset.");
                    eprintln!("   Location: {}", state_dir.display());
                }
                anyhow::anyhow!("Failed to parse runs index: {}", e)
            })?;
        Ok(index)
    }

    /// Load index, or reset state if incompatible format detected
    pub fn load_or_reset(state_dir: &Path) -> Result<Self> {
        match Self::load(state_dir) {
            Ok(index) => Ok(index),
            Err(e) if e.to_string().contains("missing field") => {
                eprintln!("🔄 Resetting incompatible state format...");
                // Delete the old state directory
                if state_dir.exists() {
                    fs::remove_dir_all(state_dir)
                        .context("Failed to remove old state directory")?;
                }
                eprintln!("✓ State directory cleared");
                Ok(Self::default())
            }
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, state_dir: &Path) -> Result<()> {
        fs::create_dir_all(state_dir)?;
        let index_path = state_dir.join("runs.json");
        let content = serde_json::to_string_pretty(self)?;

        // Atomic write using temp file
        let temp_path = state_dir.join("runs.json.tmp");
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(temp_path, index_path)?;
        Ok(())
    }

    pub fn add_run(&mut self, run: RunMetadata) {
        self.runs.insert(run.run_id.clone(), run);
    }

    #[allow(dead_code)]
    pub fn get_run(&self, run_id: &str) -> Option<&RunMetadata> {
        self.runs.get(run_id)
    }

    pub fn get_run_mut(&mut self, run_id: &str) -> Option<&mut RunMetadata> {
        self.runs.get_mut(run_id)
    }

    pub fn get_sorted_runs(&self) -> Vec<&RunMetadata> {
        let mut runs: Vec<_> = self.runs.values().collect();
        runs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        runs
    }
}

/// Save backup nodes to JSON files
pub fn save_backup_nodes(
    file_path: &Path,
    nodes: &[BackupNode],
    run_id: &str,
    state_dir: &Path,
) -> Result<()> {
    if nodes.is_empty() {
        return Ok(());
    }

    let backup_dir = state_dir.join(run_id);
    fs::create_dir_all(&backup_dir)?;

    // Create a safe file prefix based on the file path
    let safe_name = file_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("_");

    // Save each node as a separate JSON file
    for (idx, node) in nodes.iter().enumerate() {
        let node_filename = format!("{}__node_{}.json", safe_name, idx);
        let node_path = backup_dir.join(&node_filename);

        let json = serde_json::to_string_pretty(node)?;
        fs::write(&node_path, json)?;
    }

    Ok(())
}

/// Restore nodes from backup
///
/// This function restores AST nodes from backup by:
/// 1. Parsing the current file into an AST
/// 2. For each backup node, finding and replacing the corresponding node in the AST
/// 3. Writing the restored content back to the file
pub fn restore_from_nodes(
    file_path: &Path,
    nodes: &[BackupNode],
    _state_dir: &Path,
) -> Result<()> {
    use crate::editor::RustEditor;

    if nodes.is_empty() {
        return Ok(());
    }

    // Read current file content
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file for revert: {}", file_path.display()))?;

    // Parse into AST
    let mut editor = RustEditor::new(&content)?;

    // Separate struct-literal backups from others (they need special ordering)
    let (mut struct_literal_backups, other_backups): (Vec<_>, Vec<_>) = nodes.iter()
        .partition(|b| b.node_type == "struct-literal");

    // Sort struct-literal backups by counter in REVERSE order (process from end of file to beginning)
    // This ensures byte offsets remain valid as we restore
    struct_literal_backups.sort_by(|a, b| {
        let counter_a = a.identifier.split('#').nth(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        let counter_b = b.identifier.split('#').nth(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
        counter_b.cmp(&counter_a) // Reverse order
    });

    // Process struct-literal backups first (in reverse order)
    for backup in &struct_literal_backups {
        restore_struct_literal(&mut editor, backup)?;
    }

    // Then process other backups
    for backup in other_backups {
        match backup.node_type.as_str() {
            "ItemStruct" | "struct" => {
                restore_struct(&mut editor, backup)?;
            }
            "ItemEnum" | "enum" => {
                restore_enum(&mut editor, backup)?;
            }
            "ItemImpl" => {
                restore_impl(&mut editor, backup)?;
            }
            "ItemFn" | "function" => {
                // For match operations, we backup the whole function
                restore_function(&mut editor, backup)?;
            }
            "ExprStruct" => {
                // Struct literals are handled as part of the parent function
                // Skip individual struct literal restoration
            }
            "struct-literal" => {
                // Already handled above in the separate struct-literal processing
                // This case should never be reached
            }
            "ItemUse" => {
                // Use statements are simple, we can skip restoration
                // since they should be handled by other means
            }
            _ => {
                // For other node types, log a warning but don't fail
                eprintln!("Warning: Unsupported node type for revert: {}", backup.node_type);
            }
        }
    }

    // Write back the restored content
    fs::write(file_path, editor.to_string())
        .with_context(|| format!("Failed to write restored file: {}", file_path.display()))?;

    Ok(())
}

fn restore_struct(editor: &mut crate::editor::RustEditor, backup: &BackupNode) -> Result<()> {
    use syn::{parse_str, Item};

    // Parse the backup content
    let backup_item: Item = parse_str(&backup.original_content)
        .context("Failed to parse backup struct content")?;

    // Find the struct in the current AST by name
    let struct_index = editor.find_item_index("struct", &backup.identifier)
        .with_context(|| format!("Struct '{}' not found for revert", backup.identifier))?;

    // Replace with the backup using the editor's method
    editor.replace_item_at_index(struct_index, backup_item)?;

    Ok(())
}

fn restore_enum(editor: &mut crate::editor::RustEditor, backup: &BackupNode) -> Result<()> {
    use syn::{parse_str, Item};

    // Parse the backup content
    let backup_item: Item = parse_str(&backup.original_content)
        .context("Failed to parse backup enum content")?;

    // Find the enum in the current AST by name
    let enum_index = editor.find_item_index("enum", &backup.identifier)
        .with_context(|| format!("Enum '{}' not found for revert", backup.identifier))?;

    // Replace with the backup
    editor.replace_item_at_index(enum_index, backup_item)?;

    Ok(())
}

fn restore_impl(editor: &mut crate::editor::RustEditor, backup: &BackupNode) -> Result<()> {
    use syn::{parse_str, Item};

    // Parse the backup content
    let backup_item: Item = parse_str(&backup.original_content)
        .context("Failed to parse backup impl content")?;

    // Find impl block by matching on the self_ty
    let impl_index = editor.find_item_index("impl", &backup.identifier)
        .with_context(|| format!("Impl block for '{}' not found for revert", backup.identifier))?;

    // Replace with the backup
    editor.replace_item_at_index(impl_index, backup_item)?;

    Ok(())
}

fn restore_function(editor: &mut crate::editor::RustEditor, backup: &BackupNode) -> Result<()> {
    use syn::{parse_str, Item};

    // Parse the backup content
    let backup_item: Item = parse_str(&backup.original_content)
        .context("Failed to parse backup function content")?;

    // Find the function in the current AST by name
    let fn_index = editor.find_item_index("fn", &backup.identifier)
        .with_context(|| format!("Function '{}' not found for revert", backup.identifier))?;

    // Replace with the backup
    editor.replace_item_at_index(fn_index, backup_item)?;

    Ok(())
}

fn restore_struct_literal(editor: &mut crate::editor::RustEditor, backup: &BackupNode) -> Result<()> {
    use syn::{visit::Visit, ExprStruct, spanned::Spanned};
    use quote::ToTokens;

    // Extract the struct name and counter from the identifier (format: "StructName#counter" or "Enum::Variant#counter")
    let parts: Vec<&str> = backup.identifier.split('#').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid struct literal identifier: {}", backup.identifier);
    }
    let struct_name = parts[0];
    let target_counter: usize = parts[1].parse()
        .context("Invalid counter in struct literal identifier")?;

    // Parse the backup content as an expression
    let _backup_expr: ExprStruct = syn::parse_str(&backup.original_content)
        .context("Failed to parse backup struct literal content")?;

    // Find matching struct literal in the current file
    struct LiteralFinder<'a> {
        struct_name: &'a str,
        current_literals: Vec<(usize, usize, String)>, // (start_byte, end_byte, content)
        editor: &'a crate::editor::RustEditor,
    }

    impl<'ast, 'a> Visit<'ast> for LiteralFinder<'a> {
        fn visit_expr_struct(&mut self, node: &'ast ExprStruct) {
            // Check if this matches our target struct name
            let matches = if self.struct_name.contains("::") {
                // Enum variant case
                let path_str = node.path.segments.iter()
                    .map(|seg| seg.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                path_str == self.struct_name
            } else {
                // Simple struct case
                node.path.segments.len() == 1
                    && node.path.segments.last()
                        .map(|seg| seg.ident.to_string())
                        .as_ref() == Some(&self.struct_name.to_string())
            };

            if matches {
                let start = self.editor.span_to_byte_offset(node.span().start());
                let end = self.editor.span_to_byte_offset(node.span().end());
                let content = node.to_token_stream().to_string();
                self.current_literals.push((start, end, content));
            }

            syn::visit::visit_expr_struct(self, node);
        }
    }

    let mut finder = LiteralFinder {
        struct_name,
        current_literals: Vec::new(),
        editor,
    };

    let syntax_tree = editor.get_syntax_tree();
    finder.visit_file(syntax_tree);

    // Restore the specific occurrence identified by the counter
    if target_counter < finder.current_literals.len() {
        let (start, end, _) = finder.current_literals[target_counter];
        let backup_content = backup.original_content.trim();
        editor.replace_range(start, end, backup_content)?;
        Ok(())
    } else {
        // Struct literal no longer exists, which is okay for revert
        // (it might have been removed by the operation we're reverting)
        Ok(())
    }
}

/// Save run metadata
pub fn save_run_metadata(run: &RunMetadata, state_dir: &Path) -> Result<()> {
    fs::create_dir_all(state_dir)?;
    let metadata_path = state_dir.join(format!("{}.json", run.run_id));
    let content = serde_json::to_string_pretty(run)?;

    // Atomic write
    let temp_path = state_dir.join(format!("{}.json.tmp", run.run_id));
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);

    fs::rename(temp_path, metadata_path)?;

    // Update index
    let mut index = RunsIndex::load(state_dir)?;
    index.add_run(run.clone());
    index.save(state_dir)?;

    Ok(())
}

/// Load run metadata
pub fn load_run_metadata(run_id: &str, state_dir: &Path) -> Result<RunMetadata> {
    let metadata_path = state_dir.join(format!("{}.json", run_id));

    if !metadata_path.exists() {
        bail!("Run {} not found", run_id);
    }

    let content = fs::read_to_string(&metadata_path)
        .context("Failed to read run metadata")?;
    let metadata: RunMetadata = serde_json::from_str(&content)
        .context("Failed to parse run metadata")?;
    Ok(metadata)
}

/// Revert a run
pub fn revert_run(run_id: &str, force: bool, state_dir: &Path) -> Result<()> {
    // Load run metadata
    let run = load_run_metadata(run_id, state_dir)?;

    // Check if already reverted
    if run.status == RunStatus::Reverted {
        bail!("Run {} has already been reverted", run_id);
    }

    if !run.can_revert {
        bail!("Run {} cannot be reverted", run_id);
    }

    // Verify files haven't changed (unless --force)
    if !force {
        for file in &run.files_modified {
            if !file.path.exists() {
                bail!("File {} no longer exists (use --force to ignore)", file.path.display());
            }

            let current_hash = hash_file(&file.path)?;
            if current_hash != file.hash_after {
                bail!(
                    "File {} has changed since run {} (use --force to ignore)\nExpected hash: {}\nCurrent hash: {}",
                    file.path.display(),
                    run_id,
                    file.hash_after,
                    current_hash
                );
            }
        }
    }

    // Restore from backups
    println!("Reverting {} file(s)...", run.files_modified.len());
    for file in &run.files_modified {
        restore_from_nodes(&file.path, &file.backup_nodes, state_dir)?;
        println!("  ✓ Restored: {}", file.path.display());
    }

    // Mark run as reverted
    let mut index = RunsIndex::load_or_reset(state_dir)?;
    if let Some(run_meta) = index.get_run_mut(run_id) {
        run_meta.status = RunStatus::Reverted;
        run_meta.can_revert = false;
    }
    index.save(state_dir)?;

    // Update individual metadata file
    let mut run = run;
    run.status = RunStatus::Reverted;
    run.can_revert = false;
    save_run_metadata(&run, state_dir)?;

    println!("✓ Run {} reverted successfully", run_id);
    Ok(())
}

/// Display run history
pub fn show_history(limit: usize, state_dir: &Path) -> Result<()> {
    let index = RunsIndex::load_or_reset(state_dir)?;
    let runs = index.get_sorted_runs();

    if runs.is_empty() {
        println!("No runs found");
        return Ok(());
    }

    println!("Recent runs (showing up to {}):\n", limit);

    for run in runs.iter().take(limit) {
        let status_str = match run.status {
            RunStatus::Applied => if run.can_revert { "[can revert]" } else { "[applied]" },
            RunStatus::Reverted => "[reverted]",
        };

        let files_str = if run.files_modified.len() == 1 {
            "1 file".to_string()
        } else {
            format!("{} files", run.files_modified.len())
        };

        println!(
            "{}  {}  {:20}  {:10}  {}",
            run.run_id,
            run.timestamp.format("%Y-%m-%d %H:%M"),
            truncate_str(&run.operation, 20),
            files_str,
            status_str
        );
    }

    Ok(())
}

/// Clean old state data
pub fn clean_old_state(keep_days: u32, state_dir: &Path) -> Result<()> {
    let index = RunsIndex::load_or_reset(state_dir)?;
    let cutoff = Utc::now() - Duration::days(keep_days as i64);

    let mut cleaned = 0;
    let mut new_index = RunsIndex::default();

    for run in index.runs.values() {
        if run.timestamp < cutoff {
            // Remove backup directory
            let backup_dir = state_dir.join(&run.run_id);
            if backup_dir.exists() {
                fs::remove_dir_all(&backup_dir)?;
            }

            // Remove metadata file
            let metadata_path = state_dir.join(format!("{}.json", run.run_id));
            if metadata_path.exists() {
                fs::remove_file(&metadata_path)?;
            }

            cleaned += 1;
        } else {
            new_index.add_run(run.clone());
        }
    }

    // Save updated index
    new_index.save(state_dir)?;

    println!("✓ Cleaned {} old run(s)", cleaned);
    Ok(())
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len-3])
    }
}

/// Get total size of state directory
#[allow(dead_code)]
pub fn get_state_size(state_dir: &Path) -> Result<u64> {
    if !state_dir.exists() {
        return Ok(0);
    }

    let mut total_size = 0u64;
    for entry in walkdir::WalkDir::new(state_dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total_size += entry.metadata()?.len();
        }
    }
    Ok(total_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_run_id() {
        let id1 = generate_run_id();
        let id2 = generate_run_id();

        assert_eq!(id1.len(), 7);
        assert_eq!(id2.len(), 7);
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_hash_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        fs::write(&file_path, "hello world")?;
        let hash1 = hash_file(&file_path)?;

        // Same content should produce same hash
        fs::write(&file_path, "hello world")?;
        let hash2 = hash_file(&file_path)?;
        assert_eq!(hash1, hash2);

        // Different content should produce different hash
        fs::write(&file_path, "goodbye world")?;
        let hash3 = hash_file(&file_path)?;
        assert_ne!(hash1, hash3);

        Ok(())
    }

    #[test]
    fn test_backup_nodes() -> Result<()> {
        use crate::operations::{BackupNode, NodeLocation};
        let temp_dir = TempDir::new()?;
        let state_dir = temp_dir.path().join("state");
        let file_path = temp_dir.path().join("test.rs");

        // Create a backup node
        let node = BackupNode {
            node_type: "ItemStruct".to_string(),
            identifier: "User".to_string(),
            original_content: "pub struct User { id: u64 }".to_string(),
            location: NodeLocation {
                line: 1,
                column: 0,
                end_line: 1,
                end_column: 27,
            },
        };

        // Save backup nodes
        let run_id = "abc1234";
        save_backup_nodes(&file_path, &[node.clone()], run_id, &state_dir)?;

        // Verify backup file exists
        let backup_dir = state_dir.join(run_id);
        assert!(backup_dir.exists());

        // Verify we can read the backup
        let node_files: Vec<_> = fs::read_dir(&backup_dir)?.collect();
        assert_eq!(node_files.len(), 1);

        Ok(())
    }

    #[test]
    fn test_runs_index() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let state_dir = temp_dir.path().join("state");

        let run = RunMetadata {
            run_id: "abc1234".to_string(),
            timestamp: Utc::now(),
            command: "add-struct-field".to_string(),
            operation: "AddStructField".to_string(),
            files_modified: vec![],
            status: RunStatus::Applied,
            can_revert: true,
        };

        // Save run
        save_run_metadata(&run, &state_dir)?;

        // Load and verify
        let loaded = load_run_metadata("abc1234", &state_dir)?;
        assert_eq!(loaded.run_id, "abc1234");
        assert_eq!(loaded.operation, "AddStructField");

        // Check index
        let index = RunsIndex::load(&state_dir)?;
        assert_eq!(index.runs.len(), 1);
        assert!(index.get_run("abc1234").is_some());

        Ok(())
    }
}
