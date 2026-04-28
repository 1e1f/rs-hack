//! @arch:layer(kg_store)
//! @arch:role(resolve)
//!
//! Pass 3 — cross-file `Imports` edge resolution.
//!
//! Per-file indexers (Pass 2) collect import paths onto the file node's
//! `imports` property as newline-joined strings:
//!
//! * **Rust** — `crate::foo::Bar`, `super::baz`, `external::Lib` (full
//!   `use` paths).
//! * **TypeScript / TSX** — the literal `from "..."` specifier on
//!   `import` and re-export statements (`./Foo`, `react`, `@/lib/x`).
//!
//! This pass walks every file in the store, expands the per-language
//! prefixes against the file's on-disk location, and emits an `Imports`
//! edge for each path that resolves to another in-store File node.
//!
//! Resolution scope is intentionally conservative:
//!
//! * Targets are **File** nodes only — coarser than item-level but cheap
//!   and unambiguous. v2 can refine to specific items once the daemon
//!   tracks `mod foo;` → file pairings explicitly.
//! * Rust: external crate paths (anything that doesn't start with
//!   `crate`/`super`/`self`) are skipped. Eligible crate names could be
//!   pulled from `Cargo.toml` later if we want intra-workspace edges.
//! * Rust: `mod foo;` declarations aren't followed structurally — we
//!   infer the module-tree from the on-disk file layout (`src/foo.rs` or
//!   `src/foo/mod.rs`). Inline `mod foo { ... }` blocks aren't covered;
//!   their use statements are also not collected (Walker skips nested
//!   `Item::Use` for the same reason).
//! * TS: bare specifiers are dropped unless a `tsconfig.json` `paths`
//!   entry maps them into the rig. The resolver doesn't try to find
//!   `node_modules` packages — those would be external dependency edges
//!   we can't usefully render today.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::NodeId;
use yah_kg::kind::{CommonKind, Lang, NodeKind};

use crate::store::Store;

const IMPORTS_KEY: &str = "imports";

/// Walk every Rust file's `imports` property and emit `Imports` edges
/// to the file each `use` path resolves to. Returns the number of edges
/// added so callers can include the count in their delta accounting.
///
/// Idempotent: `EdgeId` is a content hash so re-running re-emits the
/// same ids and the store dedupes them.
pub fn resolve_rust_imports(store: &mut Store) -> usize {
    let crates = collect_crates(store);
    let mut emitted = 0usize;
    for crate_idx in &crates {
        for file in &crate_idx.files {
            let Some(paths) = lookup_imports(store, file.id) else {
                continue;
            };
            let current_module = current_module_path(&file.rel, &crate_idx.crate_dir);
            for raw in paths.split('\n') {
                let path = raw.trim();
                if path.is_empty() {
                    continue;
                }
                let Some(target_file) = resolve_path(path, &current_module, crate_idx) else {
                    continue;
                };
                if target_file.id == file.id {
                    // Self-import is meaningless; can happen if `self::foo` is
                    // used trivially. Skip rather than emit a self-loop.
                    continue;
                }
                let edge = EdgeOut {
                    id: EdgeId::compute(file.id, target_file.id, &EdgeKind::Imports),
                    from: file.id,
                    to: target_file.id,
                    kind: EdgeKind::Imports,
                    annotations: vec![],
                };
                if store.upsert_edge(edge) {
                    emitted += 1;
                }
            }
        }
    }
    emitted
}

/// Drop every `Imports` edge whose source is `file_rel`, regardless of
/// language. Caller is responsible for re-running the resolvers
/// afterwards (typically via the daemon's incremental reindex path).
pub fn drop_imports_from(store: &mut Store, file_rel: &str) {
    for file_id in file_node_ids(store, file_rel) {
        let edges = store.neighbors(
            file_id,
            yah_kg::rpc::Direction::Out,
            Some(&[EdgeKind::Imports]),
        );
        for e in edges {
            store.remove_edge(e.id);
        }
    }
}

fn file_node_ids(store: &Store, file_rel: &str) -> Vec<NodeId> {
    store
        .lookup(file_rel, None)
        .into_iter()
        .filter(|id| {
            store
                .node_ref(*id)
                .map(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)))
                .unwrap_or(false)
        })
        .collect()
}

#[derive(Clone)]
struct FileEntry {
    id: NodeId,
    rel: String,
}

/// One crate's resolved layout: the directory under which `src/...`
/// sits, plus the set of Rust files inside it indexed by their crate-
/// relative module path (e.g. `["foo", "bar"]` for `src/foo/bar.rs`).
struct CrateIndex {
    /// Crate root directory, rig-relative. Empty string for a crate
    /// rooted at the rig (i.e. `src/lib.rs` at the top).
    crate_dir: String,
    files: Vec<FileEntry>,
    /// Module-path → file. Path segments are crate-relative (no `crate`
    /// prefix). The empty `Vec<String>` key is the crate root itself.
    by_module: HashMap<Vec<String>, FileEntry>,
}

fn collect_crates(store: &Store) -> Vec<CrateIndex> {
    // 1. Discover crate roots by spotting any indexed `src/lib.rs` or
    //    `src/main.rs`. The crate dir is everything before that suffix.
    let mut crate_dirs: Vec<String> = Vec::new();
    for n in store
        .all_node_refs()
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)) && n.lang == Lang::Rust)
    {
        if let Some(d) = crate_root_of(&n.file) {
            crate_dirs.push(d);
        }
    }
    crate_dirs.sort_by(|a, b| b.len().cmp(&a.len())); // longest first for prefix match
    crate_dirs.dedup();

    // 2. Bucket every Rust file into its longest-matching crate dir.
    let mut by_crate: HashMap<String, Vec<FileEntry>> = HashMap::new();
    for n in store
        .all_node_refs()
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)) && n.lang == Lang::Rust)
    {
        let Some(crate_dir) = match_crate_dir(&n.file, &crate_dirs) else {
            continue;
        };
        by_crate.entry(crate_dir).or_default().push(FileEntry {
            id: n.id,
            rel: n.file.clone(),
        });
    }

    let mut out = Vec::with_capacity(by_crate.len());
    for (crate_dir, files) in by_crate {
        let mut by_module: HashMap<Vec<String>, FileEntry> = HashMap::new();
        for f in &files {
            let path = current_module_path(&f.rel, &crate_dir);
            // Prefer `mod.rs` over a sibling `name.rs` when both end up at
            // the same module path (rare today, but a real Rust pattern).
            let prefer_mod = f.rel.ends_with("/mod.rs");
            match by_module.get(&path) {
                Some(existing) if existing.rel.ends_with("/mod.rs") && !prefer_mod => {}
                _ => {
                    by_module.insert(path, f.clone());
                }
            }
        }
        out.push(CrateIndex {
            crate_dir,
            files,
            by_module,
        });
    }
    out
}

/// `path/to/crate-dir` for a file that itself *is* a crate root
/// (`src/lib.rs` or `src/main.rs`). Returns `None` for any file that
/// isn't a crate root — use [`match_crate_dir`] to bucket non-root
/// files into the crate they live under.
fn crate_root_of(file_rel: &str) -> Option<String> {
    let stripped = strip_suffix(file_rel, "src/lib.rs")
        .or_else(|| strip_suffix(file_rel, "src/main.rs"))?;
    Some(stripped.trim_end_matches('/').to_string())
}

/// Pick the crate dir that a non-root file belongs to. `crate_dirs` is
/// expected to be sorted longest-first so `crates/foo` wins over `crates`
/// when both are present.
fn match_crate_dir(file_rel: &str, crate_dirs: &[String]) -> Option<String> {
    for dir in crate_dirs {
        if dir.is_empty() {
            // Top-of-rig crate: any file under `src/` belongs to it.
            if file_rel.starts_with("src/") {
                return Some(String::new());
            }
            continue;
        }
        let with_slash = format!("{}/", dir);
        if file_rel.starts_with(&with_slash) {
            // Must live under that crate's `src/` to count.
            let rest = &file_rel[with_slash.len()..];
            if rest.starts_with("src/") {
                return Some(dir.clone());
            }
        }
    }
    None
}

/// `file_rel`'s crate-relative module-path segments. Examples:
///
/// * `<crate>/src/lib.rs`            → `[]`
/// * `<crate>/src/main.rs`           → `[]`
/// * `<crate>/src/foo.rs`            → `["foo"]`
/// * `<crate>/src/foo/mod.rs`        → `["foo"]`
/// * `<crate>/src/foo/bar.rs`        → `["foo", "bar"]`
fn current_module_path(file_rel: &str, crate_dir: &str) -> Vec<String> {
    let stem = if crate_dir.is_empty() {
        file_rel
    } else if let Some(rest) = file_rel.strip_prefix(crate_dir) {
        rest.trim_start_matches('/')
    } else {
        return Vec::new();
    };
    let Some(rest) = stem.strip_prefix("src/") else {
        return Vec::new();
    };
    if rest == "lib.rs" || rest == "main.rs" {
        return Vec::new();
    }
    let segments = rest.trim_end_matches(".rs");
    let mut parts: Vec<String> = segments.split('/').map(|s| s.to_string()).collect();
    if parts.last().map(|s| s.as_str()) == Some("mod") {
        parts.pop();
    }
    parts
}

fn lookup_imports(store: &Store, file_id: NodeId) -> Option<String> {
    let full = store.node_full(file_id)?;
    full.properties.get(IMPORTS_KEY).cloned()
}

/// Resolve one `use` path to a target file within `crate_idx`. Returns
/// `None` for anything outside the local crate (external deps, leading
/// `::` absolute paths, unrecognized roots).
fn resolve_path(path: &str, current: &[String], crate_idx: &CrateIndex) -> Option<FileEntry> {
    let segs: Vec<&str> = path.split("::").collect();
    if segs.is_empty() {
        return None;
    }
    let (mut module_path, rest_start): (Vec<String>, usize) = match segs[0] {
        "crate" => (Vec::new(), 1),
        "self" => (current.to_vec(), 1),
        "super" => {
            let mut sups = 0usize;
            while segs.get(sups) == Some(&"super") {
                sups += 1;
            }
            if sups > current.len() {
                return None;
            }
            (current[..current.len() - sups].to_vec(), sups)
        }
        // Empty first segment = leading `::` (absolute external) — skip.
        "" => return None,
        // Anything else is an external dep root we don't try to map.
        _ => return None,
    };

    // Trim the trailing item / glob — we resolve to a *module* file.
    let mut tail: Vec<String> = segs[rest_start..]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if matches!(tail.last().map(String::as_str), Some("*")) {
        // Glob — remaining `tail` already names the source module.
        tail.pop();
    } else if !tail.is_empty() {
        // Plain item path — drop the imported item; its containing
        // module is what we want as the target file.
        tail.pop();
    }
    module_path.extend(tail);

    // Walk down: try the full module path first, then back off one
    // segment at a time. `use crate::foo::bar::Baz` may resolve either
    // to `foo/bar.rs` (Baz is an item there) or `foo.rs` (bar is a type
    // re-exported through foo).
    loop {
        if let Some(f) = crate_idx.by_module.get(&module_path) {
            return Some(f.clone());
        }
        if module_path.is_empty() {
            return None;
        }
        module_path.pop();
    }
}

fn strip_suffix<'a>(haystack: &'a str, suffix: &str) -> Option<&'a str> {
    if haystack == suffix {
        return Some("");
    }
    let with_slash = format!("/{}", suffix);
    haystack.strip_suffix(&with_slash)
}

// ---------- TypeScript / TSX ----------

/// Walk every TS/TSX file's `imports` property and emit `Imports` edges
/// to the file each `from "..."` specifier resolves to. Returns the
/// number of edges added.
///
/// Resolution honors the nearest `tsconfig.json` (`baseUrl` + `paths`)
/// for non-relative specifiers, and falls back to relative-against-the-
/// importer for `./` and `../` paths. `node_modules` and other bare
/// specifiers without a matching `paths` entry are dropped — we don't
/// emit external-dep edges in v1.
///
/// Idempotent: same `EdgeId` content hash, store dedupes.
pub fn resolve_ts_imports(store: &mut Store, rig_root: &Path) -> usize {
    let configs = collect_tsconfigs(rig_root);
    let ts_files: Vec<(NodeId, String)> = store
        .all_node_refs()
        .filter(|n| matches!(n.kind, NodeKind::Common(CommonKind::File)) && n.lang == Lang::Ts)
        .map(|n| (n.id, n.file.clone()))
        .collect();
    // file-rel → NodeId for TS files, used to look up resolution targets.
    let file_index: HashMap<String, NodeId> = ts_files
        .iter()
        .map(|(id, rel)| (rel.clone(), *id))
        .collect();

    let mut emitted = 0usize;
    for (file_id, file_rel) in &ts_files {
        let Some(paths) = lookup_imports(store, *file_id) else {
            continue;
        };
        let cfg = nearest_tsconfig(&configs, file_rel);
        for raw in paths.split('\n') {
            let spec = raw.trim();
            if spec.is_empty() {
                continue;
            }
            let Some(target_rel) = resolve_ts_specifier(spec, file_rel, cfg, &file_index) else {
                continue;
            };
            let Some(target_id) = file_index.get(&target_rel) else {
                continue;
            };
            if *target_id == *file_id {
                // `./` to the same file (rare — `./Foo` from inside Foo.ts);
                // skip rather than emit a self-loop.
                continue;
            }
            let edge = EdgeOut {
                id: EdgeId::compute(*file_id, *target_id, &EdgeKind::Imports),
                from: *file_id,
                to: *target_id,
                kind: EdgeKind::Imports,
                annotations: vec![],
            };
            if store.upsert_edge(edge) {
                emitted += 1;
            }
        }
    }
    emitted
}

/// One parsed `tsconfig.json`. `config_dir` is the rig-relative directory
/// the tsconfig lives in; `base_url` is rig-relative too (the `baseUrl`
/// option resolved against `config_dir`). `paths` is the compilerOption
/// `paths` map normalized to `(pattern, [substitution, ...])`.
#[derive(Debug, Clone)]
struct TsConfig {
    config_dir: String,
    base_url: Option<String>,
    paths: Vec<(String, Vec<String>)>,
}

fn collect_tsconfigs(rig_root: &Path) -> Vec<TsConfig> {
    let mut out: Vec<TsConfig> = Vec::new();
    for entry in walkdir::WalkDir::new(rig_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped_for_tsconfig(e.path(), rig_root))
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != "tsconfig.json" {
            continue;
        }
        let abs = entry.path();
        let Ok(text) = std::fs::read_to_string(abs) else {
            continue;
        };
        let Some(parent) = abs.parent() else { continue };
        let config_dir = relativize_to_root(parent, rig_root);
        if let Some(cfg) = parse_tsconfig(&text, &config_dir) {
            out.push(cfg);
        }
    }
    // Longest config_dir first so nested tsconfigs win over outer ones.
    out.sort_by(|a, b| b.config_dir.len().cmp(&a.config_dir.len()));
    out
}

fn is_skipped_for_tsconfig(path: &Path, root: &Path) -> bool {
    if path == root {
        return false;
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name == "target" || name == "node_modules" || name == ".git" {
        return true;
    }
    name.starts_with('.') && name != "."
}

fn relativize_to_root(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Parse a `tsconfig.json` body and pull out just the bits Pass 3 uses:
/// `compilerOptions.baseUrl` and `compilerOptions.paths`. Both are
/// optional; a tsconfig with neither still gets returned so its config
/// dir registers (relative imports inside it still resolve).
///
/// Returns `None` only on JSON parse failure. Comments and trailing
/// commas (the JSONC dialect TypeScript actually accepts) aren't
/// supported — fixtures and the dogfooded `yah-ui/tsconfig.json` are
/// plain JSON, which is enough for v1.
fn parse_tsconfig(text: &str, config_dir: &str) -> Option<TsConfig> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let opts = v.get("compilerOptions");
    let base_url = opts
        .and_then(|o| o.get("baseUrl"))
        .and_then(|s| s.as_str())
        .map(|s| join_norm(config_dir, s));
    let mut paths: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(map) = opts.and_then(|o| o.get("paths")).and_then(|p| p.as_object()) {
        for (pattern, subs) in map {
            let Some(arr) = subs.as_array() else { continue };
            let mut subs_out: Vec<String> = Vec::new();
            for s in arr {
                if let Some(s) = s.as_str() {
                    subs_out.push(s.to_string());
                }
            }
            if !subs_out.is_empty() {
                paths.push((pattern.clone(), subs_out));
            }
        }
        // Longer patterns win — a more specific match (`@components/*`)
        // should beat a catch-all (`*`).
        paths.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    }
    Some(TsConfig {
        config_dir: config_dir.to_string(),
        base_url,
        paths,
    })
}

fn nearest_tsconfig<'a>(configs: &'a [TsConfig], file_rel: &str) -> Option<&'a TsConfig> {
    configs.iter().find(|c| starts_with_dir(file_rel, &c.config_dir))
}

fn starts_with_dir(file_rel: &str, dir: &str) -> bool {
    if dir.is_empty() {
        return true;
    }
    let with_slash = format!("{}/", dir);
    file_rel.starts_with(&with_slash) || file_rel == dir
}

/// Resolve one TS specifier to a rig-relative file path. Tries the
/// configured candidate base, then `.ts` / `.tsx` / `.d.ts` extensions
/// and `index.*` fallbacks. Returns `None` for anything that doesn't
/// land on a TS file we already indexed.
fn resolve_ts_specifier(
    spec: &str,
    importer_rel: &str,
    cfg: Option<&TsConfig>,
    file_index: &HashMap<String, NodeId>,
) -> Option<String> {
    let importer_dir = parent_dir(importer_rel);
    let candidates: Vec<String> = if spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".." {
        vec![join_norm(&importer_dir, spec)]
    } else {
        let mut out: Vec<String> = Vec::new();
        if let Some(cfg) = cfg {
            // 1. tsconfig `paths` patterns — substitute `*` if present.
            for (pattern, subs) in &cfg.paths {
                if let Some(captured) = match_paths_pattern(pattern, spec) {
                    let base = cfg.base_url.as_deref().unwrap_or(&cfg.config_dir);
                    for sub in subs {
                        let resolved = sub.replace('*', captured);
                        out.push(join_norm(base, &resolved));
                    }
                }
            }
            // 2. baseUrl fallback — bare `foo` resolves to `<baseUrl>/foo`
            //    even without an explicit paths mapping.
            if out.is_empty() {
                if let Some(base) = &cfg.base_url {
                    out.push(join_norm(base, spec));
                }
            }
        }
        out
    };

    for base in &candidates {
        if let Some(rel) = try_resolve_with_extensions(base, file_index) {
            return Some(rel);
        }
    }
    None
}

/// Match `spec` against a tsconfig `paths` pattern. Patterns may contain
/// at most one `*` (the TypeScript spec). Returns the captured substring,
/// `Some("")` if the pattern is exact and matches, or `None` on miss.
fn match_paths_pattern<'a>(pattern: &str, spec: &'a str) -> Option<&'a str> {
    if let Some(star) = pattern.find('*') {
        let prefix = &pattern[..star];
        let suffix = &pattern[star + 1..];
        if spec.starts_with(prefix) && spec.ends_with(suffix) && spec.len() >= prefix.len() + suffix.len() {
            return Some(&spec[prefix.len()..spec.len() - suffix.len()]);
        }
        None
    } else if pattern == spec {
        Some("")
    } else {
        None
    }
}

const TS_EXTS: &[&str] = &[".ts", ".tsx", ".d.ts"];
const TS_INDEX_NAMES: &[&str] = &["index.ts", "index.tsx", "index.d.ts"];

fn try_resolve_with_extensions(base: &str, file_index: &HashMap<String, NodeId>) -> Option<String> {
    // Specifier may already include the extension (rare in practice; most
    // bundlers forbid it). Honor it if so.
    if file_index.contains_key(base) {
        return Some(base.to_string());
    }
    for ext in TS_EXTS {
        let candidate = format!("{}{}", base, ext);
        if file_index.contains_key(&candidate) {
            return Some(candidate);
        }
    }
    for idx in TS_INDEX_NAMES {
        let candidate = if base.is_empty() {
            idx.to_string()
        } else {
            format!("{}/{}", base, idx)
        };
        if file_index.contains_key(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Join `base` and `rel` and normalize `.` / `..` segments. Both inputs
/// are POSIX-style (forward slashes); output is too. `base` may be empty
/// (rig root); a leading `/` on the result would be wrong because all our
/// paths are rig-relative, so we strip any leading slash.
fn join_norm(base: &str, rel: &str) -> String {
    let mut buf = PathBuf::new();
    if !base.is_empty() {
        for seg in base.split('/') {
            buf.push(seg);
        }
    }
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                buf.pop();
            }
            other => buf.push(other),
        }
    }
    let s = buf.to_string_lossy().replace('\\', "/");
    s.trim_start_matches('/').to_string()
}

fn parent_dir(file_rel: &str) -> String {
    match file_rel.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_crate_root_at_root_and_nested() {
        assert_eq!(crate_root_of("src/lib.rs"), Some(String::new()));
        assert_eq!(crate_root_of("src/main.rs"), Some(String::new()));
        assert_eq!(
            crate_root_of("yah-kg/src/lib.rs"),
            Some("yah-kg".to_string())
        );
        assert_eq!(
            crate_root_of("crates/foo/src/lib.rs"),
            Some("crates/foo".to_string())
        );
        // Non-root files should not register as a crate root.
        assert_eq!(crate_root_of("README.md"), None);
        assert_eq!(crate_root_of("src/foo.rs"), None);
    }

    #[test]
    fn match_crate_dir_picks_longest_prefix() {
        let dirs = vec!["crates/foo".to_string(), String::new()];
        assert_eq!(
            match_crate_dir("crates/foo/src/bar.rs", &dirs),
            Some("crates/foo".to_string())
        );
        // Top-of-rig crate catches files that don't match a nested crate.
        assert_eq!(
            match_crate_dir("src/bar.rs", &dirs),
            Some(String::new())
        );
        // Files outside any `src/` are unowned.
        assert_eq!(match_crate_dir("crates/foo/Cargo.toml", &dirs), None);
    }

    #[test]
    fn current_module_path_examples() {
        assert_eq!(
            current_module_path("yah-kg/src/lib.rs", "yah-kg"),
            Vec::<String>::new()
        );
        assert_eq!(
            current_module_path("yah-kg/src/foo.rs", "yah-kg"),
            vec!["foo".to_string()]
        );
        assert_eq!(
            current_module_path("yah-kg/src/foo/mod.rs", "yah-kg"),
            vec!["foo".to_string()]
        );
        assert_eq!(
            current_module_path("yah-kg/src/foo/bar.rs", "yah-kg"),
            vec!["foo".to_string(), "bar".to_string()]
        );
        assert_eq!(
            current_module_path("src/lib.rs", ""),
            Vec::<String>::new()
        );
    }
}
