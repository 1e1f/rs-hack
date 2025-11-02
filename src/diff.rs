use std::path::Path;
use similar::{ChangeTag, TextDiff};

/// Represents statistics about a diff
#[derive(Debug, Default)]
pub struct DiffStats {
    pub files_changed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

impl DiffStats {
    pub fn add(&mut self, other: &DiffStats) {
        self.files_changed += other.files_changed;
        self.lines_added += other.lines_added;
        self.lines_removed += other.lines_removed;
    }

    pub fn print_summary(&self) {
        println!("\nSummary:");
        println!("Files changed: {}", self.files_changed);
        println!("Lines added: {}", self.lines_added);
        println!("Lines removed: {}", self.lines_removed);
    }
}

/// Generate a unified diff between original and modified content
///
/// Returns the unified diff string and statistics about the changes.
///
/// # Arguments
/// * `path` - The file path (used in diff headers)
/// * `original` - The original file content
/// * `modified` - The modified file content
/// * `context_lines` - Number of context lines to show (default is 3)
pub fn generate_unified_diff(
    path: &Path,
    original: &str,
    modified: &str,
    context_lines: usize,
) -> (String, DiffStats) {
    let diff = TextDiff::from_lines(original, modified);

    let mut output = String::new();
    let mut stats = DiffStats::default();

    // Generate unified diff format headers
    let path_str = path.display().to_string();
    output.push_str(&format!("--- {}\n", path_str));
    output.push_str(&format!("+++ {}\n", path_str));

    // Count changes for statistics
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => stats.lines_added += 1,
            ChangeTag::Delete => stats.lines_removed += 1,
            ChangeTag::Equal => {}
        }
    }

    // Generate the unified diff with context
    let unified = diff.unified_diff()
        .context_radius(context_lines)
        .to_string();

    output.push_str(&unified);

    if stats.lines_added > 0 || stats.lines_removed > 0 {
        stats.files_changed = 1;
    }

    (output, stats)
}

/// Print a unified diff to stdout
///
/// This is a convenience function that generates and prints a diff.
///
/// # Arguments
/// * `path` - The file path
/// * `original` - The original file content
/// * `modified` - The modified file content
///
/// Returns statistics about the diff.
pub fn print_diff(path: &Path, original: &str, modified: &str) -> DiffStats {
    let (diff_output, stats) = generate_unified_diff(path, original, modified, 3);

    // Only print if there are actual changes
    if stats.files_changed > 0 {
        print!("{}", diff_output);
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_unified_diff() {
        let original = "pub struct User {\n    id: u64,\n    name: String,\n}\n";
        let modified = "pub struct User {\n    id: u64,\n    age: u32,\n    name: String,\n}\n";
        let path = PathBuf::from("src/user.rs");

        let (diff, stats) = generate_unified_diff(&path, original, modified, 3);

        // Check that diff contains expected headers
        assert!(diff.contains("--- src/user.rs"));
        assert!(diff.contains("+++ src/user.rs"));

        // Check that diff contains the added line
        assert!(diff.contains("+    age: u32,"));

        // Check statistics
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.lines_added, 1);
        assert_eq!(stats.lines_removed, 0);
    }

    #[test]
    fn test_generate_unified_diff_no_changes() {
        let content = "pub struct User {\n    id: u64,\n}\n";
        let path = PathBuf::from("src/user.rs");

        let (diff, stats) = generate_unified_diff(&path, content, content, 3);

        // Should have headers but no hunks
        assert!(diff.contains("--- src/user.rs"));
        assert!(diff.contains("+++ src/user.rs"));

        // Statistics should show no changes
        assert_eq!(stats.files_changed, 0);
        assert_eq!(stats.lines_added, 0);
        assert_eq!(stats.lines_removed, 0);
    }

    #[test]
    fn test_generate_unified_diff_with_removal() {
        let original = "pub struct User {\n    id: u64,\n    name: String,\n    email: String,\n}\n";
        let modified = "pub struct User {\n    id: u64,\n    name: String,\n}\n";
        let path = PathBuf::from("src/user.rs");

        let (diff, stats) = generate_unified_diff(&path, original, modified, 3);

        // Check that diff contains the removed line
        assert!(diff.contains("-    email: String,"));

        // Check statistics
        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.lines_added, 0);
        assert_eq!(stats.lines_removed, 1);
    }

    #[test]
    fn test_diff_stats_add() {
        let mut stats1 = DiffStats {
            files_changed: 1,
            lines_added: 5,
            lines_removed: 2,
        };

        let stats2 = DiffStats {
            files_changed: 2,
            lines_added: 3,
            lines_removed: 1,
        };

        stats1.add(&stats2);

        assert_eq!(stats1.files_changed, 3);
        assert_eq!(stats1.lines_added, 8);
        assert_eq!(stats1.lines_removed, 3);
    }

    #[test]
    fn test_print_diff_returns_stats() {
        let original = "line1\nline2\n";
        let modified = "line1\nline2\nline3\n";
        let path = PathBuf::from("test.txt");

        let stats = print_diff(&path, original, modified);

        assert_eq!(stats.files_changed, 1);
        assert_eq!(stats.lines_added, 1);
        assert_eq!(stats.lines_removed, 0);
    }
}
