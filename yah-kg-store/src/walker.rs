//! @arch:layer(kg)
//! @arch:role(graph)
//!
//! Directory walker that drives a registry of `LanguageIndexer`s.
//!
//! This is the daemon's Pass 1: walk the rig, emit `Directory` and `File`
//! nodes with `Contains` edges, then dispatch each file to the matching
//! indexer for Pass 2 (in-file structure). Indexers are pure; the walker
//! owns the file I/O.

use crate::sink::StoreSink;
use crate::store::Store;
use std::path::Path;
use walkdir::WalkDir;
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::indexer::{IndexError, LanguageIndexer};
use yah_kg::kind::{CommonKind, Lang, NodeKind};

/// Maps file extensions to the indexer that should handle them.
pub struct IndexerRegistry {
    indexers: Vec<Box<dyn LanguageIndexer>>,
}

impl Default for IndexerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexerRegistry {
    pub fn new() -> Self {
        Self { indexers: Vec::new() }
    }

    pub fn register(&mut self, indexer: Box<dyn LanguageIndexer>) {
        self.indexers.push(indexer);
    }

    /// First-match-wins lookup by file extension (lowercased, no dot).
    pub fn for_extension(&self, ext: &str) -> Option<&dyn LanguageIndexer> {
        let needle = ext.to_lowercase();
        for ix in &self.indexers {
            if ix.extensions().iter().any(|e| e.eq_ignore_ascii_case(&needle)) {
                return Some(ix.as_ref());
            }
        }
        None
    }

    pub fn languages(&self) -> Vec<Lang> {
        self.indexers.iter().map(|i| i.lang()).collect()
    }
}

#[derive(Debug, Default, Clone)]
pub struct WalkSummary {
    pub files_seen: u32,
    pub files_indexed: u32,
    pub files_skipped: u32,
    pub parse_errors: u32,
}

/// Walk `root`, emit Directory/File nodes + Contains edges, dispatch each
/// recognized file to its indexer. Symlinks are not followed; hidden
/// directories (leading `.`) and `target/` are skipped.
pub fn walk_and_index(
    root: &Path,
    store: &mut Store,
    registry: &IndexerRegistry,
) -> Result<WalkSummary, IndexError> {
    let mut summary = WalkSummary::default();
    let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    for entry in WalkDir::new(&root_canon)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped(e.path(), &root_canon))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        let rel = relativize(path, &root_canon);

        if entry.file_type().is_dir() {
            push_directory(&rel, path, &root_canon, store);
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }
        summary.files_seen += 1;

        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            summary.files_skipped += 1;
            continue;
        };
        let Some(indexer) = registry.for_extension(ext) else {
            summary.files_skipped += 1;
            continue;
        };

        let file_node = push_file(&rel, indexer.lang(), path, &root_canon, store);

        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return Err(IndexError::Io(format!("{}: {}", path.display(), e)));
            }
        };

        let mut sink = StoreSink::new(store);
        match indexer.index_file(Path::new(&rel), &src, &mut sink) {
            Ok(()) => summary.files_indexed += 1,
            Err(_) => summary.parse_errors += 1,
        }

        // Best-effort: link file → top-level items the indexer just produced
        // is the indexer's job, not ours. We don't synthesize Contains here.
        let _ = file_node;
    }
    Ok(summary)
}

fn is_skipped(path: &Path, root: &Path) -> bool {
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

fn relativize(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn push_directory(rel: &str, abs: &Path, root: &Path, store: &mut Store) {
    let label = abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
        .to_string();
    let qualified = if rel.is_empty() { ".".to_string() } else { rel.to_string() };
    let id = NodeId::compute(Lang::Rust, &qualified, &qualified);
    let node = NodeRef {
        id,
        lang: Lang::Rust, // language-agnostic but Lang must be assigned
        kind: NodeKind::Common(CommonKind::Directory),
        label,
        qualified: qualified.clone(),
        file: qualified.clone(),
        span: Span::point(1, 1),
        synthetic: false,
    };
    store.upsert_node(node);

    if let Some(parent) = abs.parent() {
        if parent.starts_with(root) && parent != abs {
            let parent_rel = relativize(parent, root);
            let parent_qualified = if parent_rel.is_empty() {
                ".".to_string()
            } else {
                parent_rel
            };
            let parent_id = NodeId::compute(Lang::Rust, &parent_qualified, &parent_qualified);
            let edge = EdgeOut {
                id: EdgeId::compute(parent_id, id, &EdgeKind::Contains),
                from: parent_id,
                to: id,
                kind: EdgeKind::Contains,
                annotations: vec![],
            };
            store.upsert_edge(edge);
        }
    }
}

fn push_file(rel: &str, lang: Lang, abs: &Path, root: &Path, store: &mut Store) -> NodeId {
    let label = abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
        .to_string();
    let id = NodeId::compute(lang, rel, rel);
    let node = NodeRef {
        id,
        lang,
        kind: NodeKind::Common(CommonKind::File),
        label,
        qualified: rel.to_string(),
        file: rel.to_string(),
        span: Span::point(1, 1),
        synthetic: false,
    };
    store.upsert_node(node);

    if let Some(parent) = abs.parent() {
        let parent_rel = relativize(parent, root);
        let parent_qualified = if parent_rel.is_empty() {
            ".".to_string()
        } else {
            parent_rel
        };
        let parent_id = NodeId::compute(Lang::Rust, &parent_qualified, &parent_qualified);
        let edge = EdgeOut {
            id: EdgeId::compute(parent_id, id, &EdgeKind::Contains),
            from: parent_id,
            to: id,
            kind: EdgeKind::Contains,
            annotations: vec![],
        };
        store.upsert_edge(edge);
    }
    id
}
