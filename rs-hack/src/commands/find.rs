//! `find` command as a lib API. Returns structured matches; rendering (text,
//! snippets, hints) is the caller's job — see `main.rs` for the CLI renderer.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::editor::RustEditor;
use crate::files::{collect_rust_files_with_exclusions, expand_kind_to_node_types};
use crate::operations::{FieldLocation, InspectResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindArgs {
    pub paths: Vec<PathBuf>,
    pub exclude: Vec<String>,
    pub kind: Option<String>,
    pub node_type: Option<String>,
    pub name: Option<String>,
    pub variant: Option<String>,
    pub content_filter: Option<String>,
    pub field_name: Option<String>,
    pub include_comments: bool,
    /// Number of raw source lines to show before each snippet match (like grep -B N)
    #[serde(default)]
    pub context: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FindResult {
    Field { matches: Vec<FieldLocation> },
    Nodes { matches: Vec<InspectResult> },
}

impl FindResult {
    pub fn is_empty(&self) -> bool {
        match self {
            FindResult::Field { matches } => matches.is_empty(),
            FindResult::Nodes { matches } => matches.is_empty(),
        }
    }
}

pub fn run(args: &FindArgs) -> Result<FindResult> {
    let files = collect_rust_files_with_exclusions(&args.paths, &args.exclude)?;

    if let Some(field) = &args.field_name {
        return Ok(FindResult::Field {
            matches: find_field(&files, field)?,
        });
    }

    let node_types_to_search: Vec<Option<&str>> = if let Some(k) = &args.kind {
        let expanded = expand_kind_to_node_types(k);
        if expanded.is_empty() {
            anyhow::bail!(
                "Unknown kind '{}'. Valid kinds: struct, function, enum, match, identifier, type, macro, const, trait, mod, use",
                k
            );
        }
        expanded.into_iter().map(Some).collect()
    } else if let Some(nt) = &args.node_type {
        vec![Some(nt.as_str())]
    } else {
        vec![None]
    };

    let mut all_results: Vec<InspectResult> = Vec::new();

    for file in &files {
        let content = std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                continue;
            }
        };

        for node_type_to_search in &node_types_to_search {
            let mut results = editor.inspect(
                *node_type_to_search,
                args.name.as_deref(),
                args.variant.as_deref(),
                args.include_comments,
            )?;

            for result in &mut results {
                result.file_path = file.to_string_lossy().to_string();
            }

            if let Some(filter) = &args.content_filter {
                results.retain(|r| r.snippet.contains(filter));
            }

            all_results.extend(results);
        }
    }

    Ok(FindResult::Nodes {
        matches: all_results,
    })
}

/// Re-search across all node types — used by the CLI to suggest near-misses
/// when a typed search returns nothing. Exposed so embedders can offer the
/// same hint UX.
pub fn run_unfiltered_by_node_type(args: &FindArgs) -> Result<Vec<InspectResult>> {
    let files = collect_rust_files_with_exclusions(&args.paths, &args.exclude)?;
    let mut hint_results: Vec<InspectResult> = Vec::new();

    for file in &files {
        let content = std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut results =
            editor.inspect(None, args.name.as_deref(), args.variant.as_deref(), false)?;

        for result in &mut results {
            result.file_path = file.to_string_lossy().to_string();
        }

        if let Some(filter) = &args.content_filter {
            results.retain(|r| r.snippet.contains(filter));
        }

        hint_results.extend(results);
    }

    Ok(hint_results)
}

fn find_field(files: &[PathBuf], field: &str) -> Result<Vec<FieldLocation>> {
    let mut all_locations: Vec<FieldLocation> = Vec::new();

    for file in files {
        let content = std::fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {:?}", file))?;

        let editor = match RustEditor::new(&content) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("⚠️  Skipping {}: {}", file.display(), e);
                continue;
            }
        };
        let mut locations = editor.find_field_locations(field)?;
        for location in &mut locations {
            location.file_path = file.to_string_lossy().to_string();
        }
        all_locations.extend(locations);
    }

    Ok(all_locations)
}
