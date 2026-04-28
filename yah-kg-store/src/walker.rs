//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Directory walker that drives a registry of `LanguageIndexer`s.
//!
//! Pass 1: walk the rig, emit `Directory` and `File` nodes with `Contains`
//! edges; Pass 2: dispatch each file to the matching indexer for in-file
//! structure. Indexers are pure; the walker owns the file I/O.
//!
//! Also exposes [`reindex_file`] for the daemon's incremental update path:
//! snapshots a file's nodes, removes them, re-runs the walker on that one
//! file, and returns a [`FileDelta`] describing what changed so the daemon
//! can fan out `ArchEvent`s on its broadcast channel.

use crate::sink::StoreSink;
use crate::store::Store;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use yah_kg::edge::{EdgeId, EdgeKind, EdgeOut};
use yah_kg::event::ChangedField;
use yah_kg::ids::{NodeId, NodeRef, Span};
use yah_kg::indexer::{IndexError, LanguageIndexer};
use yah_kg::kind::{CommonKind, Lang, NodeKind};
use yah_kg::rpc::Direction;

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

/// Result of an incremental single-file reindex. Field meanings match the
/// `ArchEvent` payloads the daemon will emit for each.
#[derive(Debug, Default, Clone)]
pub struct FileDelta {
    pub nodes_added: Vec<NodeRef>,
    pub nodes_removed: Vec<NodeId>,
    pub nodes_changed: Vec<(NodeId, Vec<ChangedField>)>,
    pub edges_added: Vec<EdgeOut>,
    pub edges_removed: Vec<EdgeId>,
}

impl FileDelta {
    pub fn is_empty(&self) -> bool {
        self.nodes_added.is_empty()
            && self.nodes_removed.is_empty()
            && self.nodes_changed.is_empty()
            && self.edges_added.is_empty()
            && self.edges_removed.is_empty()
    }
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

        match index_one_file(&rel, path, &root_canon, store, registry)? {
            FileOutcome::Indexed => summary.files_indexed += 1,
            FileOutcome::Skipped => summary.files_skipped += 1,
            FileOutcome::ParseError => summary.parse_errors += 1,
        }
    }
    // Pass 3: now that every file's nodes + `imports` properties are in
    // the store, walk the rig and emit cross-file `Imports` edges.
    crate::resolve::resolve_rust_imports(store);
    crate::resolve::resolve_ts_imports(store, &root_canon);
    Ok(summary)
}

/// Incremental reindex of one file at `file_rel` (relative to `rig_root`).
///
/// 1. Snapshots the before-state for the file (nodes + incident edges).
/// 2. Removes those nodes (the store auto-cleans incident edges).
/// 3. Re-runs the walker for that single file, restoring the parent
///    directory chain and re-dispatching to the language indexer.
/// 4. Diffs before/after and returns a [`FileDelta`].
///
/// If the file is missing from disk, the function still wipes the
/// before-state and returns a delta with `nodes_removed` populated — that
/// covers the file-deleted case.
pub fn reindex_file(
    rig_root: &Path,
    file_rel: &str,
    store: &mut Store,
    registry: &IndexerRegistry,
) -> Result<FileDelta, IndexError> {
    let rig_canon = rig_root
        .canonicalize()
        .unwrap_or_else(|_| rig_root.to_path_buf());
    let abs = rig_canon.join(file_rel);

    // 1. Snapshot before-state.
    let before_nodes: HashMap<NodeId, NodeRef> = store
        .lookup(file_rel, None)
        .into_iter()
        .filter_map(|id| store.node_ref(id).map(|n| (id, n.clone())))
        .collect();
    let mut before_edges: HashMap<EdgeId, EdgeOut> = HashMap::new();
    for id in before_nodes.keys() {
        for e in store.neighbors(*id, Direction::Both, None) {
            before_edges.insert(e.id, e);
        }
    }

    // 2. Remove the file's nodes.
    for id in before_nodes.keys() {
        store.remove_node(*id);
    }

    // 3. Re-emit if the file still exists.
    if abs.is_file() {
        ensure_directory_chain(file_rel, &rig_canon, store);
        let _ = index_one_file(file_rel, &abs, &rig_canon, store, registry)?;
    }

    // Pass 3: this file's import set may have changed and other files'
    // imports may now resolve to a node that's been re-issued. Drop and
    // re-resolve cheaply: full pass is O(files), edges dedupe by id.
    crate::resolve::drop_imports_from(store, file_rel);
    crate::resolve::resolve_rust_imports(store);
    crate::resolve::resolve_ts_imports(store, &rig_canon);

    // 4. Snapshot after-state and diff.
    let after_nodes: HashMap<NodeId, NodeRef> = store
        .lookup(file_rel, None)
        .into_iter()
        .filter_map(|id| store.node_ref(id).map(|n| (id, n.clone())))
        .collect();
    let mut after_edges: HashMap<EdgeId, EdgeOut> = HashMap::new();
    for id in after_nodes.keys() {
        for e in store.neighbors(*id, Direction::Both, None) {
            after_edges.insert(e.id, e);
        }
    }

    Ok(diff(&before_nodes, &after_nodes, &before_edges, &after_edges))
}

fn diff(
    before_nodes: &HashMap<NodeId, NodeRef>,
    after_nodes: &HashMap<NodeId, NodeRef>,
    before_edges: &HashMap<EdgeId, EdgeOut>,
    after_edges: &HashMap<EdgeId, EdgeOut>,
) -> FileDelta {
    let mut delta = FileDelta::default();

    for (id, after) in after_nodes {
        match before_nodes.get(id) {
            None => delta.nodes_added.push(after.clone()),
            Some(before) => {
                let fields = changed_fields(before, after);
                if !fields.is_empty() {
                    delta.nodes_changed.push((*id, fields));
                }
            }
        }
    }
    for id in before_nodes.keys() {
        if !after_nodes.contains_key(id) {
            delta.nodes_removed.push(*id);
        }
    }

    for (id, edge) in after_edges {
        if !before_edges.contains_key(id) {
            delta.edges_added.push(edge.clone());
        }
    }
    let after_ids: HashSet<EdgeId> = after_edges.keys().copied().collect();
    for id in before_edges.keys() {
        if !after_ids.contains(id) {
            delta.edges_removed.push(*id);
        }
    }
    delta
}

fn changed_fields(before: &NodeRef, after: &NodeRef) -> Vec<ChangedField> {
    let mut fields = Vec::new();
    if before.span != after.span {
        fields.push(ChangedField::Span);
    }
    if before.label != after.label {
        fields.push(ChangedField::Label);
    }
    if before.qualified != after.qualified {
        fields.push(ChangedField::Qualified);
    }
    if before.file != after.file {
        fields.push(ChangedField::File);
    }
    fields
}

enum FileOutcome {
    Indexed,
    Skipped,
    ParseError,
}

fn index_one_file(
    rel: &str,
    abs: &Path,
    root: &Path,
    store: &mut Store,
    registry: &IndexerRegistry,
) -> Result<FileOutcome, IndexError> {
    let Some(ext) = abs.extension().and_then(|s| s.to_str()) else {
        return Ok(FileOutcome::Skipped);
    };
    let Some(indexer) = registry.for_extension(ext) else {
        return Ok(FileOutcome::Skipped);
    };

    let _ = push_file(rel, indexer.lang(), abs, root, store);

    let src = std::fs::read_to_string(abs)
        .map_err(|e| IndexError::Io(format!("{}: {}", abs.display(), e)))?;

    let mut sink = StoreSink::new(store);
    match indexer.index_file(Path::new(rel), &src, &mut sink) {
        Ok(()) => Ok(FileOutcome::Indexed),
        Err(_) => Ok(FileOutcome::ParseError),
    }
}

/// Walk up `file_rel`'s ancestor chain and re-emit any missing Directory
/// nodes (idempotent — already-present nodes dedupe). Used by
/// [`reindex_file`] to restore the parent edge after wiping the file's
/// nodes.
fn ensure_directory_chain(file_rel: &str, root: &Path, store: &mut Store) {
    // Walk every ancestor of file_rel under root, emitting upward.
    let mut parts: Vec<&str> = file_rel.split('/').collect();
    parts.pop(); // drop the file basename
    let mut acc = PathBuf::new();
    let mut acc_rel = String::new();
    push_root_directory(root, store);
    for part in parts {
        if !acc_rel.is_empty() {
            acc_rel.push('/');
        }
        acc_rel.push_str(part);
        acc.push(part);
        let abs = root.join(&acc);
        if abs.is_dir() {
            push_directory(&acc_rel, &abs, root, store);
        }
    }
}

fn push_root_directory(root: &Path, store: &mut Store) {
    push_directory("", root, root, store);
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
