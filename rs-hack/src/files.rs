//! File discovery: glob/dir traversal and exclusion filtering for `.rs` files,
//! plus the kind→node-type expansion used by `find` and friends.

use std::path::PathBuf;

use anyhow::{Context, Result};
use glob::glob;
use walkdir::WalkDir;

pub fn collect_rust_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    collect_rust_files_with_exclusions(paths, &[])
}

pub fn collect_rust_files_with_exclusions(
    paths: &[PathBuf],
    exclude_patterns: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        let path_str = path.to_string_lossy();

        if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
            for entry in glob(&path_str).context("Failed to parse glob pattern")? {
                match entry {
                    Ok(file_path) => {
                        if file_path.is_file()
                            && file_path.extension().and_then(|s| s.to_str()) == Some("rs")
                        {
                            files.push(file_path);
                        }
                    }
                    Err(e) => eprintln!("Warning: Error reading glob entry: {}", e),
                }
            }
        } else if path.is_file() {
            if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
            {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    if !exclude_patterns.is_empty() {
        files.retain(|file| {
            let file_str = file.to_string_lossy();
            !exclude_patterns.iter().any(|pattern| {
                if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches(&file_str))
                        .unwrap_or(false)
                } else {
                    file_str.contains(pattern.as_str())
                }
            })
        });
    }

    Ok(files)
}

pub fn expand_kind_to_node_types(kind: &str) -> Vec<&'static str> {
    match kind {
        "struct" => vec!["struct", "struct-literal"],
        "function" => vec!["function", "function-call", "method-call", "impl-method", "trait-method"],
        "enum" => vec!["enum", "enum-usage"],
        "match" => vec!["match-arm"],
        "identifier" => vec!["identifier"],
        "type" => vec!["type-ref", "type-alias"],
        "macro" => vec!["macro-call"],
        "const" => vec!["const", "static"],
        "trait" => vec!["trait", "trait-impl"],
        "mod" => vec!["mod"],
        "use" => vec!["use"],
        _ => vec![],
    }
}
