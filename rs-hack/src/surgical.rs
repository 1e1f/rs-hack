/// Surgical edit module for making minimal, targeted changes to source code.
///
/// This module provides infrastructure for applying precise edits to source code
/// while preserving all formatting, comments, and whitespace.
///
/// Unlike the "reformat" approach which uses prettyplease to reformat the entire file,
/// surgical edits only modify the specific locations that need to change.

use proc_macro2::LineColumn;
use std::cmp::Ordering;

/// Represents a single textual replacement in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// Starting position (line, column) - 1-indexed for lines, 0-indexed for columns
    pub start: LineColumn,
    /// Ending position (line, column) - 1-indexed for lines, 0-indexed for columns
    pub end: LineColumn,
    /// The text to replace with
    pub new_text: String,
}

impl Replacement {
    pub fn new(start: LineColumn, end: LineColumn, new_text: String) -> Self {
        Self {
            start,
            end,
            new_text,
        }
    }
}

impl Ord for Replacement {
    fn cmp(&self, other: &Self) -> Ordering {
        // Sort by start position (line, then column)
        match self.start.line.cmp(&other.start.line) {
            Ordering::Equal => self.start.column.cmp(&other.start.column),
            other => other,
        }
    }
}

impl PartialOrd for Replacement {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Apply surgical edits to source code, preserving all formatting.
///
/// This function takes the original source code and a list of replacements,
/// and produces a new string with only those specific changes applied.
///
/// # Arguments
/// * `original_source` - The original source code
/// * `replacements` - List of replacements to apply (will be sorted automatically)
///
/// # Returns
/// The modified source code with only the specified changes applied
///
/// # Example
/// ```
/// use rs_hack::surgical::{Replacement, apply_surgical_edits};
/// use proc_macro2::LineColumn;
///
/// let source = "fn foo() {\n    let x = 1;\n}\n";
/// let replacements = vec![
///     Replacement::new(
///         LineColumn { line: 2, column: 12 },
///         LineColumn { line: 2, column: 13 },
///         "42".to_string(),
///     ),
/// ];
///
/// let result = apply_surgical_edits(source, replacements);
/// assert_eq!(result, "fn foo() {\n    let x = 42;\n}");
/// ```
pub fn apply_surgical_edits(
    original_source: &str,
    mut replacements: Vec<Replacement>,
) -> String {
    if replacements.is_empty() {
        return original_source.to_string();
    }

    // Sort replacements by position
    replacements.sort();

    // Validate no overlapping replacements
    for i in 1..replacements.len() {
        let prev = &replacements[i - 1];
        let curr = &replacements[i];

        if prev.end.line > curr.start.line ||
           (prev.end.line == curr.start.line && prev.end.column > curr.start.column) {
            panic!("Overlapping replacements detected: {:?} and {:?}", prev, curr);
        }
    }

    let lines: Vec<&str> = original_source.lines().collect();
    let mut result = String::new();

    let mut current_line = 1usize;  // 1-indexed to match proc_macro2
    let mut current_col = 0usize;    // 0-indexed

    for replacement in replacements {
        // Copy unchanged text up to this replacement

        // Copy full lines before the replacement
        while current_line < replacement.start.line {
            if current_line <= lines.len() {
                // Add any remaining text on current line
                if let Some(line) = lines.get(current_line - 1) {
                    if current_col < line.len() {
                        result.push_str(&line[current_col..]);
                    }
                }
                result.push('\n');
            }
            current_line += 1;
            current_col = 0;
        }

        // Copy partial line up to replacement start (on the same line)
        if current_line == replacement.start.line {
            if let Some(line) = lines.get(current_line - 1) {
                if current_col < replacement.start.column && replacement.start.column <= line.len() {
                    result.push_str(&line[current_col..replacement.start.column]);
                }
            }
        }

        // Apply the replacement
        result.push_str(&replacement.new_text);

        // Update position to after the replacement
        current_line = replacement.end.line;
        current_col = replacement.end.column;
    }

    // Copy remaining text after all replacements
    while current_line <= lines.len() {
        if let Some(line) = lines.get(current_line - 1) {
            if current_col < line.len() {
                result.push_str(&line[current_col..]);
            }
        }
        if current_line < lines.len() {
            result.push('\n');
        }
        current_line += 1;
        current_col = 0;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_replacement() {
        let source = "fn foo() {\n    let x = 1;\n}";
        let replacements = vec![
            Replacement::new(
                LineColumn { line: 2, column: 12 },
                LineColumn { line: 2, column: 13 },
                "42".to_string(),
            ),
        ];

        let result = apply_surgical_edits(source, replacements);
        assert_eq!(result, "fn foo() {\n    let x = 42;\n}");
    }

    #[test]
    fn test_multiple_replacements() {
        let source = "let a = 1;\nlet b = 2;";
        let replacements = vec![
            Replacement::new(
                LineColumn { line: 1, column: 8 },
                LineColumn { line: 1, column: 9 },
                "10".to_string(),
            ),
            Replacement::new(
                LineColumn { line: 2, column: 8 },
                LineColumn { line: 2, column: 9 },
                "20".to_string(),
            ),
        ];

        let result = apply_surgical_edits(source, replacements);
        assert_eq!(result, "let a = 10;\nlet b = 20;");
    }

    #[test]
    fn test_preserves_whitespace() {
        let source = "fn foo() {\n\n    // comment\n    let x = old;\n}";
        let replacements = vec![
            Replacement::new(
                LineColumn { line: 4, column: 12 },
                LineColumn { line: 4, column: 15 },
                "new".to_string(),
            ),
        ];

        let result = apply_surgical_edits(source, replacements);
        assert_eq!(result, "fn foo() {\n\n    // comment\n    let x = new;\n}");
    }

    #[test]
    fn test_no_replacements() {
        let source = "fn foo() {}\n";
        let replacements = vec![];

        let result = apply_surgical_edits(source, replacements);
        assert_eq!(result, source);
    }

    #[test]
    fn test_replacement_sorting() {
        let source = "let a = 1; let b = 2;";
        // Add replacements out of order
        let replacements = vec![
            Replacement::new(
                LineColumn { line: 1, column: 19 },
                LineColumn { line: 1, column: 20 },
                "20".to_string(),
            ),
            Replacement::new(
                LineColumn { line: 1, column: 8 },
                LineColumn { line: 1, column: 9 },
                "10".to_string(),
            ),
        ];

        let result = apply_surgical_edits(source, replacements);
        assert_eq!(result, "let a = 10; let b = 20;");
    }
}
