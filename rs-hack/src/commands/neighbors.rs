//! `neighbors` command: pure filesystem discovery of related files.
//! No AST parsing — finds siblings, twin dirs, and test files for any .rs file.

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug)]
pub struct NeighborsReport {
    pub target: PathBuf,
    pub siblings: Vec<PathBuf>,
    pub twin_files: Vec<PathBuf>,
    pub test_files: Vec<PathBuf>,
}

pub fn run(path: &Path) -> Result<NeighborsReport> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let parent = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // --- Siblings: other .rs files in same directory ---
    let mut siblings = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let ep = entry.path();
            if ep == path {
                continue;
            }
            if ep.extension().and_then(|s| s.to_str()) == Some("rs") {
                siblings.push(ep);
            }
        }
    }
    siblings.sort();

    // --- Twin dirs: walk up 2 ancestor levels, find sibling dirs whose name
    //     is the input parent's name + digit suffix, collect matching filenames ---
    let mut twin_files = Vec::new();

    let parent_name = parent
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // grandparent (level 1 up from parent) and great-grandparent (level 2)
    let mut search_roots: Vec<PathBuf> = Vec::new();
    if let Some(grandparent) = parent.parent() {
        search_roots.push(grandparent.to_path_buf());
        if let Some(great) = grandparent.parent() {
            search_roots.push(great.to_path_buf());
        }
    }

    for root in &search_roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let ep = entry.path();
                if !ep.is_dir() {
                    continue;
                }
                let dir_name = ep
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                // Twin: parent_name + at least one digit suffix
                if dir_name.starts_with(&parent_name)
                    && dir_name.len() > parent_name.len()
                    && dir_name[parent_name.len()..].chars().all(|c| c.is_ascii_digit())
                {
                    // Walk into this twin dir looking for files whose stem matches
                    collect_matching_files(&ep, &stem, &mut twin_files);
                }
            }
        }
    }
    twin_files.sort();

    // --- Tests: nearest `tests/` dir at or above parent's grandparent ---
    let mut test_files = Vec::new();

    let mut search = Some(parent);
    while let Some(dir) = search {
        let tests_dir = dir.join("tests");
        if tests_dir.is_dir() {
            collect_matching_files(&tests_dir, &stem, &mut test_files);
            break;
        }
        search = dir.parent();
    }
    test_files.sort();

    Ok(NeighborsReport {
        target: path,
        siblings,
        twin_files,
        test_files,
    })
}

/// Walk `dir` recursively and collect all .rs files whose stem contains `stem`.
fn collect_matching_files(dir: &Path, stem: &str, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let ep = entry.path();
            if ep.is_dir() {
                collect_matching_files(&ep, stem, out);
            } else if ep.extension().and_then(|s| s.to_str()) == Some("rs") {
                let file_stem = ep
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if file_stem.contains(stem) {
                    out.push(ep);
                }
            }
        }
    }
}
