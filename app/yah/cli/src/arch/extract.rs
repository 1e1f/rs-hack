//! @arch:layer(arch)
//! @arch:role(extract)
//! @arch:role(parser)
//! @yah:ticket(R001-T2, "Scan non-Rust files for @yah: annotations (md, ts, toml, yaml)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:handoff("P1+P2+P3 landed. AnnotationTarget::File { path, anchor } variant added; line_extract walks ts/tsx/js/jsx/md/toml/yaml with per-language comment-prefix tables (TS: //, //!, ///, /**, /*, *; MD: empty prefix with code-fence skipping; TOML/YAML: #). Block grouping mirrors Rust's per-doc-block: contiguous comment-prefixed (or non-blank for MD) lines share an anchor. Wired into extract_from_workspace via extension dispatch. Tests: 10 new integration tests in rs-hack-arch/tests/non_rust_extract.rs (TS module block, // and /** */, MD with/without code fences, TOML/YAML, blank-line block break, File target id format, @arch on MD). Smoke verified: rs-hack board status / show R002-T1 surfaces with source ./hack-board/src/server.ts:1. P3 archive endpoint: hack-board/src/server.ts::stripHackAnnotations now dispatches via annotationStripRulesFor(filePath) — covers .rs/.ts/.tsx/.js/.jsx/.md/.toml/.yaml/.yml. Validated all rules with an inline bun smoke test (15/15 pass). Drive-by: dogfood test_hack_tickets_from_workspace was asserting on archived R001 — rewrote to bind to lex-first live ticket so it survives ID turnover.")
//! @yah:cleanup("setHackStatus (drag-and-drop column move) at hack-board/src/server.ts:889 still uses Rust-only //! / /// regexes — TS/MD tickets will silently no-op when dragged between columns. Same per-extension dispatch pattern as stripHackAnnotations applies; refactor both to share annotationStripRulesFor.")
//! @yah:cleanup("compute_workspace_hash now hashes non-Rust files too, so cached-graph invalidation should be correct — but no test covers that.")
//! @yah:verify("cargo test -p yah")
//! @yah:verify("yah board show R002-T1 — surfaces with source ./hack-board/src/server.ts:1")
//! @yah:verify("Smoke archive: bun run hack-board/src/server.ts in a test workspace, drag a .ts ticket to Review, click archive — should strip @yah: lines from the .ts file and append an `archived` event to .yah/events/<shard>.jsonl")
//! @arch:see(.yah/arch/authored/ts-annotation-scanning.md)
//!
//! Annotation extraction from Rust source files.
//! Uses syn to parse source and extract `@arch:` annotations from doc comments.

use crate::arch::annotation::{AnnotationTarget, ArchAnnotation, ArchKind};
use anyhow::{Context, Result};
use std::path::Path;
use syn::{Attribute, File, Item, Meta};
use walkdir::WalkDir;

/// Extract all annotations from a workspace.
pub fn extract_from_workspace(root: impl AsRef<Path>) -> Result<Vec<ArchAnnotation>> {
    extract_from_workspace_with_options(root, false)
}

/// Extract with verbose option.
pub fn extract_from_workspace_verbose(root: impl AsRef<Path>, verbose: bool) -> Result<Vec<ArchAnnotation>> {
    extract_from_workspace_with_options(root, verbose)
}

fn extract_from_workspace_with_options(root: impl AsRef<Path>, verbose: bool) -> Result<Vec<ArchAnnotation>> {
    let root = root.as_ref();
    if verbose {
        eprintln!("Scanning: {}", std::fs::canonicalize(root).map(|p| p.display().to_string()).unwrap_or_else(|_| root.display().to_string()));
    }
    let mut annotations = Vec::new();
    let mut file_count = 0;

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_target_dir(e))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        let extracted = match ext {
            Some("rs") => Some(extract_from_file(path)),
            Some("ts" | "tsx" | "js" | "jsx") => {
                Some(line_extract(path, TS_PREFIXES, false))
            }
            Some("md") => Some(line_extract(path, MD_PREFIXES, true)),
            Some("toml" | "yaml" | "yml") => {
                Some(line_extract(path, TOML_PREFIXES, false))
            }
            _ => None,
        };
        if let Some(result) = extracted {
            file_count += 1;
            match result {
                Ok(file_annotations) => {
                    if verbose && !file_annotations.is_empty() {
                        eprintln!("  {} -> {} annotations", path.display(), file_annotations.len());
                    }
                    annotations.extend(file_annotations);
                }
                Err(e) => {
                    if verbose {
                        eprintln!("Warning: Failed to parse {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    if verbose {
        eprintln!("Scanned {} files, found {} annotations", file_count, annotations.len());
    }
    Ok(annotations)
}

// Length-descending so longest match wins (//! before //).
const TS_PREFIXES: &[&str] = &["//!", "///", "/**", "//", "/*", "*"];
const TOML_PREFIXES: &[&str] = &["#"];
const MD_PREFIXES: &[&str] = &[""];

/// Extract `@yah:` / `@arch:` annotations from a non-Rust file using a
/// per-language comment-prefix table. Block-grouping rule: contiguous
/// comment lines (or, for empty-prefix formats like Markdown, contiguous
/// non-blank lines) share an `anchor` so multi-line headers like
///
/// ```text
/// //! @yah:ticket(R002-T1, "...")
/// //! @yah:status(review)
/// ```
///
/// roll up into one ticket. A non-comment / blank line / fence boundary
/// closes the block; the next annotation starts a fresh anchor.
pub fn line_extract(
    path: &Path,
    prefixes: &[&str],
    allow_fences: bool,
) -> Result<Vec<ArchAnnotation>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut annotations = Vec::new();
    let mut in_fence = false;
    let mut block_anchor: Option<usize> = None;
    let has_empty_prefix = prefixes.iter().any(|p| p.is_empty());

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        if allow_fences {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
                block_anchor = None;
                continue;
            }
            if in_fence {
                block_anchor = None;
                continue;
            }
        }

        let stripped = strip_any_prefix(line, prefixes);
        match stripped {
            Some(after_prefix) => {
                let trimmed = after_prefix.trim_start();
                let annotation_rest = trimmed
                    .strip_prefix("@arch:")
                    .or_else(|| trimmed.strip_prefix("@yah:"));
                if let Some(rest) = annotation_rest {
                    let anchor = *block_anchor.get_or_insert(line_num);
                    if let Some(kind) = parse_single_annotation(rest.trim()) {
                        annotations.push(ArchAnnotation {
                            file: path.to_path_buf(),
                            line: line_num,
                            target: AnnotationTarget::File {
                                path: path.to_path_buf(),
                                anchor,
                            },
                            kind,
                            doc_text: None,
                        });
                    }
                } else if has_empty_prefix && line.trim().is_empty() {
                    // Markdown: blank line breaks the block.
                    block_anchor = None;
                }
                // Otherwise: comment-prefixed continuation — keep anchor.
            }
            None => {
                // No comment prefix matched — block break.
                block_anchor = None;
            }
        }
    }

    Ok(annotations)
}

fn strip_any_prefix<'a>(line: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let trimmed = line.trim_start();
    for prefix in prefixes {
        if prefix.is_empty() {
            return Some(trimmed);
        }
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            return Some(rest);
        }
    }
    None
}

/// Extract annotations from a single Rust source file.
pub fn extract_from_file(path: impl AsRef<Path>) -> Result<Vec<ArchAnnotation>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    extract_from_source(&content, path)
}

/// Extract annotations from Rust source code.
pub fn extract_from_source(source: &str, file_path: &Path) -> Result<Vec<ArchAnnotation>> {
    let file: File = syn::parse_file(source)
        .with_context(|| format!("Failed to parse {}", file_path.display()))?;

    let module_path = file_path_to_module(file_path);
    let mut annotations = Vec::new();

    // Extract module-level annotations (//! comments)
    // First, collect ALL doc text and annotations from inner attrs
    let (module_annotations, module_doc) = extract_from_inner_attrs(&file.attrs, file_path, &module_path);

    // Attach doc text to first annotation if we have any
    if !module_annotations.is_empty() {
        let mut anns = module_annotations;
        if let Some(doc) = module_doc {
            anns[0].doc_text = Some(doc);
        }
        annotations.extend(anns);
    }

    // Extract annotations from items
    for item in &file.items {
        annotations.extend(extract_from_item(item, file_path, &module_path));
    }

    Ok(annotations)
}

/// Extract annotations and doc text from all inner attributes together.
fn extract_from_inner_attrs(
    attrs: &[Attribute],
    file: &Path,
    module: &str,
) -> (Vec<ArchAnnotation>, Option<String>) {
    let target = AnnotationTarget::Module {
        path: module.to_string(),
    };

    let mut annotations = Vec::new();
    let mut all_doc_lines = Vec::new();

    for attr in attrs {
        // Check if this is a doc attribute
        if !attr.path().is_ident("doc") {
            continue;
        }

        // Get the doc string content
        let doc_content = match &attr.meta {
            Meta::NameValue(nv) => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    s.value()
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Parse each line
        for line in doc_content.lines() {
            let trimmed = line.trim();
            let annotation_rest = trimmed
                .strip_prefix("@arch:")
                .or_else(|| trimmed.strip_prefix("@yah:"));
            if let Some(rest) = annotation_rest {
                if let Some(kind) = parse_single_annotation(rest.trim()) {
                    annotations.push(ArchAnnotation {
                        file: file.to_path_buf(),
                        line: attr.pound_token.span.start().line,
                        target: target.clone(),
                        kind,
                        doc_text: None,
                    });
                }
            } else {
                // Collect non-annotation lines
                all_doc_lines.push(line.to_string());
            }
        }
    }

    // Combine doc lines, trimming leading/trailing empty lines
    let doc_text = all_doc_lines.join("\n");
    let doc_text = doc_text.trim().to_string();
    let doc_text = if doc_text.is_empty() { None } else { Some(doc_text) };

    (annotations, doc_text)
}

/// Extract annotations from a single item.
fn extract_from_item(item: &Item, file: &Path, module: &str) -> Vec<ArchAnnotation> {
    match item {
        Item::Struct(s) => {
            let target = AnnotationTarget::Struct {
                name: s.ident.to_string(),
                module: module.to_string(),
            };
            extract_from_attrs(&s.attrs, file, target)
        }
        Item::Enum(e) => {
            let target = AnnotationTarget::Enum {
                name: e.ident.to_string(),
                module: module.to_string(),
            };
            extract_from_attrs(&e.attrs, file, target)
        }
        Item::Fn(f) => {
            let target = AnnotationTarget::Function {
                name: f.sig.ident.to_string(),
                module: module.to_string(),
            };
            extract_from_attrs(&f.attrs, file, target)
        }
        Item::Impl(i) => {
            let self_ty = quote::quote!(#(#i.self_ty)).to_string();
            let trait_name = i.trait_.as_ref().map(|(_, path, _)| {
                path.segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::")
            });
            let target = AnnotationTarget::Impl {
                self_ty,
                trait_name,
                module: module.to_string(),
            };
            extract_from_attrs(&i.attrs, file, target)
        }
        Item::Mod(m) => {
            let nested_module = format!("{}::{}", module, m.ident);
            let mut annotations = Vec::new();

            // Module's own attributes
            let target = AnnotationTarget::Module {
                path: nested_module.clone(),
            };
            annotations.extend(extract_from_attrs(&m.attrs, file, target));

            // Items inside the module
            if let Some((_, items)) = &m.content {
                for item in items {
                    annotations.extend(extract_from_item(item, file, &nested_module));
                }
            }

            annotations
        }
        _ => Vec::new(),
    }
}

/// Extract annotations from outer attributes (/// comments).
fn extract_from_attrs(
    attrs: &[Attribute],
    file: &Path,
    target: AnnotationTarget,
) -> Vec<ArchAnnotation> {
    let mut annotations = Vec::new();
    let mut all_doc_lines = Vec::new();

    for attr in attrs {
        // Check if this is a doc attribute
        if !attr.path().is_ident("doc") {
            continue;
        }

        // Get the doc string content
        let doc_content = match &attr.meta {
            Meta::NameValue(nv) => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    s.value()
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Parse each line
        for line in doc_content.lines() {
            let trimmed = line.trim();
            let annotation_rest = trimmed
                .strip_prefix("@arch:")
                .or_else(|| trimmed.strip_prefix("@yah:"));
            if let Some(rest) = annotation_rest {
                if let Some(kind) = parse_single_annotation(rest.trim()) {
                    annotations.push(ArchAnnotation {
                        file: file.to_path_buf(),
                        line: attr.pound_token.span.start().line,
                        target: target.clone(),
                        kind,
                        doc_text: None,
                    });
                }
            } else {
                // Collect non-annotation lines
                all_doc_lines.push(line.to_string());
            }
        }
    }

    // Combine doc lines, trimming leading/trailing empty lines
    if !annotations.is_empty() && !all_doc_lines.is_empty() {
        let doc_text = all_doc_lines.join("\n");
        let doc_text = doc_text.trim().to_string();
        if !doc_text.is_empty() {
            annotations[0].doc_text = Some(doc_text);
        }
    }

    annotations
}

/// Parse a single @arch:key(value) annotation.
fn parse_single_annotation(s: &str) -> Option<ArchKind> {
    // Handle parameterless annotations
    if !s.contains('(') {
        return Some(ArchKind::parse(s.trim(), ""));
    }

    // Parse key(value) format
    let paren_start = s.find('(')?;
    let paren_end = s.rfind(')')?;

    if paren_end <= paren_start {
        return None;
    }

    let key = s[..paren_start].trim();
    let value = s[paren_start + 1..paren_end].trim();

    Some(ArchKind::parse(key, value))
}

/// Convert a file path to a Rust module path.
fn file_path_to_module(path: &Path) -> String {
    // Walk components and slice from the rightmost `src` boundary so that
    // both relative (`src/foo/bar.rs`, `crates/koda/core/src/state.rs`) and
    // absolute (`/Users/leif/.../crates/koda/core/src/state.rs`) inputs
    // produce a clean module path. Falling back to the leading-prefix
    // strip alone produced bogus modules like `::Users::leif::ss::…` for
    // absolute paths (every `/` became `::`).
    let comps: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
            // Drop RootDir, CurDir, ParentDir, Prefix — they're filesystem
            // artifacts, not module path segments.
            _ => None,
        })
        .collect();

    // Find the rightmost Cargo-known boundary (`src`, `tests`, `examples`,
    // `benches`); module path = components after it. If no boundary exists
    // and `crates/` is the leading segment, drop just that. Otherwise fall
    // back to the filename stem so absolute paths like
    // `/Users/leif/.../foo.rs` don't leak the whole filesystem path.
    let boundary = ["src", "tests", "examples", "benches"];
    let module_segments: Vec<&str> = if let Some(idx) = comps
        .iter()
        .rposition(|s| boundary.contains(&s.as_str()))
    {
        comps[idx + 1..].iter().map(|s| s.as_str()).collect()
    } else if comps.first().map(|s| s.as_str()) == Some("crates") {
        comps[1..].iter().map(|s| s.as_str()).collect()
    } else if path.is_absolute() {
        // Absolute path with no recognizable Cargo boundary — fall back
        // to filename stem to avoid leaking `/Users/leif/...` segments.
        comps.last().map(|s| vec![s.as_str()]).unwrap_or_default()
    } else {
        comps.iter().map(|s| s.as_str()).collect()
    };

    let mut joined = module_segments.join("/");
    if let Some(stripped) = joined.strip_suffix(".rs") {
        joined = stripped.to_string();
    }
    if let Some(stripped) = joined.strip_suffix("/mod") {
        joined = stripped.to_string();
    }
    joined.replace('/', "::")
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    // Don't filter out the root entry (depth 0)
    if entry.depth() == 0 {
        return false;
    }
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };
    if !name.starts_with('.') {
        return false;
    }
    // Allowlist dot-dirs that legitimately carry source-with-annotations.
    // `.github` houses CI workflow YAML — natural home for release-infra
    // tickets; without this exception, `board open --file
    // .github/workflows/ci.yml` writes an annotation the scanner can never
    // see, and the next allocator pass re-issues the ID it just gave out.
    // Files (e.g. `.gitignore`) at depth 1 have no children to recurse into,
    // so allowing them is harmless and keeps the rule simple.
    let allow = matches!(name, ".github");
    !allow
}

fn is_target_dir(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name();
    // Skip target dir and common non-source / build-output directories.
    // `dist`, `build`, `.next`, `.turbo` cover JS/TS bundlers — without
    // them, bundles that inline source comments (e.g. esbuild's main.js)
    // produce phantom duplicate annotations.
    matches!(
        name.to_str(),
        Some(
            "target" | "rust" | "vendor" | "node_modules"
                | "dist" | "build" | ".next" | ".turbo"
        )
    )
}

/// Compute a hash of the source files for caching.
pub fn compute_workspace_hash(root: impl AsRef<Path>) -> Result<String> {
    use blake3::Hasher;

    let mut hasher = Hasher::new();

    for entry in WalkDir::new(root.as_ref())
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_target_dir(e))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let scanned = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "ts" | "tsx" | "js" | "jsx" | "md" | "toml" | "yaml" | "yml")
        );
        if scanned {
            if let Ok(content) = std::fs::read(path) {
                hasher.update(&content);
            }
        }
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layer() {
        let result = parse_single_annotation("layer(vivarium)");
        assert!(matches!(result, Some(ArchKind::Layer(s)) if s == "vivarium"));
    }

    #[test]
    fn test_parse_thread_flow() {
        let result = parse_single_annotation("thread(any -> audio)");
        assert!(matches!(
            result,
            Some(ArchKind::Thread(crate::arch::annotation::ThreadSpec::Flow { from, to }))
            if from == "any" && to == "audio"
        ));
    }

    #[test]
    fn test_parse_qos_with_latency() {
        let result = parse_single_annotation("qos(realtime:20ms)");
        assert!(matches!(
            result,
            Some(ArchKind::Qos(spec)) if spec.class == "realtime" && spec.max_latency_ms == Some(20)
        ));
    }

    #[test]
    fn test_parse_produces() {
        let result = parse_single_annotation("produces(impulse:NoteOn, impulse:SetParam)");
        if let Some(ArchKind::Produces(specs)) = result {
            assert_eq!(specs.len(), 2);
            assert_eq!(specs[0].category, "impulse");
            assert_eq!(specs[0].name, "NoteOn");
        } else {
            panic!("Expected Produces");
        }
    }

    #[test]
    fn test_parse_gateway() {
        let result = parse_single_annotation("gateway");
        assert!(matches!(result, Some(ArchKind::Gateway)));
    }

    #[test]
    fn test_parse_bridge() {
        let result = parse_single_annotation("bridge(midi -> impulse)");
        assert!(matches!(
            result,
            Some(ArchKind::Bridge { from, to }) if from == "midi" && to == "impulse"
        ));
    }

    #[test]
    fn test_file_path_to_module() {
        assert_eq!(
            file_path_to_module(Path::new("src/vivarium/impulse.rs")),
            "vivarium::impulse"
        );
        // Multi-crate workspace path slices from the rightmost `src` so the
        // module path matches what `cargo` would call it.
        assert_eq!(
            file_path_to_module(Path::new("crates/koda/core/src/state.rs")),
            "state"
        );
    }

    #[test]
    fn test_file_path_to_module_absolute() {
        // Regression: absolute paths used to produce `::Users::leif::ss::…`
        // because every `/` became `::`. The rightmost-boundary slice fixes it.
        assert_eq!(
            file_path_to_module(Path::new(
                "/Users/leif/ss/yah/yah/src/arch/mcp.rs"
            )),
            "arch::mcp"
        );
        assert_eq!(
            file_path_to_module(Path::new(
                "/Users/leif/ss/nt_alt/crates/spill/src/banana.rs"
            )),
            "banana"
        );
        // tests/ is a Cargo boundary too — was leaking the absolute prefix.
        assert_eq!(
            file_path_to_module(Path::new(
                "/Users/leif/ss/noisetable/crates/vivarium/banana_nodes/tests/chip_type_args.rs"
            )),
            "chip_type_args"
        );
    }

    #[test]
    fn test_file_path_to_module_mod_rs() {
        assert_eq!(
            file_path_to_module(Path::new("src/foo/mod.rs")),
            "foo"
        );
    }

    #[test]
    fn test_parse_note() {
        let result = parse_single_annotation("note(Design decision: use channels)");
        assert!(matches!(result, Some(ArchKind::Note(s)) if s == "Design decision: use channels"));
    }

    #[test]
    fn test_parse_see() {
        let result = parse_single_annotation("see(docs/architecture.md)");
        assert!(matches!(result, Some(ArchKind::See(s)) if s == "docs/architecture.md"));
    }

    #[test]
    fn test_doc_text_capture() {
        let source = r#"
//! @arch:layer(vivarium)
//! @arch:role(synthesis)
//!
//! # Overview
//!
//! This is the synthesis engine.
//! It handles audio processing.

pub fn process() {}
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        assert!(!annotations.is_empty());

        // The first annotation should have doc_text
        let first = &annotations[0];
        assert!(first.doc_text.is_some());
        let doc = first.doc_text.as_ref().unwrap();
        assert!(doc.contains("# Overview"));
        assert!(doc.contains("synthesis engine"));
    }

    #[test]
    fn test_doc_text_combined_from_multiple_attrs() {
        let source = r#"
//! @arch:layer(core)
//! First line of doc.
//! @arch:role(runtime)
//! Second line of doc.

pub struct Engine;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        assert!(!annotations.is_empty());

        // Check that doc text is captured
        let first = &annotations[0];
        assert!(first.doc_text.is_some());
        let doc = first.doc_text.as_ref().unwrap();
        assert!(doc.contains("First line"));
        assert!(doc.contains("Second line"));
    }

    #[test]
    fn test_full_integration_demo() {
        // This demonstrates the complete workflow for replacing architecture docs
        let source = r#"
//! @arch:layer(vivarium)
//! @arch:role(synthesis)
//! @arch:thread(audio)
//! @arch:qos(realtime:20ms)
//! @arch:note(Core synthesis engine - no heap allocations in audio path)
//! @arch:see(docs/synthesis.md)
//!
//! # Banana: Local Continuous Signal Processing
//!
//! This module handles continuous local synthesis calculations.
//! Unlike Impulse (network events), Banana operates at audio rate
//! with zero network latency.
//!
//! ## Key Responsibilities
//!
//! - Continuous synthesis parameters
//! - Voice state management
//! - Real-time modulation

pub mod engine;
"#;
        let annotations = extract_from_source(source, Path::new("banana/src/lib.rs")).unwrap();
        let graph = crate::arch::graph::ArchGraph::from_annotations(annotations);
        let ctx = crate::arch::query::get_file_context(&graph, "banana");

        // Verify all data is captured
        assert_eq!(ctx.layer.as_deref(), Some("vivarium"));
        assert!(ctx.roles.contains(&"synthesis".to_string()));
        assert!(ctx.qos.as_deref() == Some("realtime"));
        assert!(ctx.notes.iter().any(|n| n.contains("no heap allocations")));
        assert!(ctx.see_also.iter().any(|s| s.contains("synthesis.md")));
        assert!(ctx.doc.as_ref().unwrap().contains("Banana: Local Continuous"));
        assert!(ctx.doc.as_ref().unwrap().contains("Key Responsibilities"));

        // Verify markdown output includes everything
        let md = ctx.to_markdown("banana/src/lib.rs");
        assert!(md.contains("**Layer**: vivarium"));
        assert!(md.contains("**Roles**: synthesis"));
        assert!(md.contains("Design notes"));
        assert!(md.contains("See also"));
        assert!(md.contains("Documentation"));
    }
}
