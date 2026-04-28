//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! KG snapshot persistence: write the daemon's in-memory state to disk on
//! demand, replay it on the next boot, and reconcile any files that have
//! been edited (or deleted) while the daemon wasn't running by walking
//! the rig and comparing mtime + size against the snapshot's recorded
//! [`FileFingerprint`].
//!
//! The snapshot file lives wherever the caller asks. The conventional
//! path is `<rig_root>/.yah/cache/snapshot.json` — see
//! [`default_snapshot_path`].

use crate::path::{canonicalize_root, is_eligible};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;
use yah_kg_anno::AnnotationIndexSnapshot;
use yah_kg_store::{IndexerRegistry, StoreSnapshot};

pub const SNAPSHOT_VERSION: u32 = 1;

/// On-disk fingerprint of one source file. Mtime + size are sufficient
/// for "did this file change while the daemon was offline?" — full
/// content hashing is a future optimization (the old `rs-hack-arch`
/// `source_hash` cache).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub mtime_secs: u64,
    pub mtime_nanos: u32,
    pub size: u64,
}

impl FileFingerprint {
    pub fn from_metadata(meta: &std::fs::Metadata) -> Option<Self> {
        let modified = meta.modified().ok()?;
        let dur = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        Some(FileFingerprint {
            mtime_secs: dur.as_secs(),
            mtime_nanos: dur.subsec_nanos(),
            size: meta.len(),
        })
    }
}

/// Top-level snapshot — store + annotations + the rig-relative file
/// fingerprints that produced them. The `rig_root` is captured so a
/// snapshot loaded against a different workspace fails fast rather
/// than silently restoring stale paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgSnapshot {
    pub version: u32,
    pub rig_root: PathBuf,
    pub fingerprints: HashMap<String, FileFingerprint>,
    pub store: StoreSnapshot,
    pub annotations: AnnotationIndexSnapshot,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("snapshot io: {0}")]
    Io(String),
    #[error("snapshot parse: {0}")]
    Parse(String),
    #[error("snapshot version mismatch: file {file}, expected {expected}")]
    Version { file: u32, expected: u32 },
    #[error("snapshot rig_root mismatch: file {file:?}, expected {expected:?}")]
    RigRoot { file: PathBuf, expected: PathBuf },
    #[error(transparent)]
    Store(#[from] yah_kg_store::SnapshotError),
}

/// Conventional snapshot location: `<rig_root>/.yah/cache/snapshot.json`.
pub fn default_snapshot_path(rig_root: &Path) -> PathBuf {
    rig_root.join(".yah").join("cache").join("snapshot.json")
}

/// Read and parse a snapshot file. Caller is responsible for verifying
/// `rig_root` against the running service's bound rig.
pub fn read_snapshot(path: &Path) -> Result<KgSnapshot, SnapshotError> {
    let bytes = std::fs::read(path).map_err(|e| SnapshotError::Io(e.to_string()))?;
    let snap: KgSnapshot =
        serde_json::from_slice(&bytes).map_err(|e| SnapshotError::Parse(e.to_string()))?;
    if snap.version != SNAPSHOT_VERSION {
        return Err(SnapshotError::Version {
            file: snap.version,
            expected: SNAPSHOT_VERSION,
        });
    }
    Ok(snap)
}

/// Write a snapshot atomically: write to a sibling `*.tmp`, fsync, rename.
/// Atomicity matters because a half-written snapshot would silently
/// corrupt the next boot's replay.
pub fn write_snapshot(path: &Path, snap: &KgSnapshot) -> Result<(), SnapshotError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SnapshotError::Io(e.to_string()))?;
    }
    let mut tmp = path.to_path_buf();
    let tmp_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(name) => format!(".{}.tmp", name),
        None => ".snapshot.tmp".to_string(),
    };
    tmp.set_file_name(tmp_name);
    let bytes =
        serde_json::to_vec(snap).map_err(|e| SnapshotError::Parse(e.to_string()))?;
    std::fs::write(&tmp, &bytes).map_err(|e| SnapshotError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| SnapshotError::Io(e.to_string()))?;
    Ok(())
}

/// Walk the rig and fingerprint every file the registry would index.
/// Mirrors [`yah_kg_store::walker::walk_and_index`]'s skip rules so the
/// fingerprint set lines up 1:1 with what got indexed.
///
/// Errors during stat are silent — a file we can't fingerprint is
/// treated the same as an absent one (the caller will reindex it).
pub fn fingerprint_rig(
    rig_root: &Path,
    registry: &IndexerRegistry,
) -> HashMap<String, FileFingerprint> {
    let canon = canonicalize_root(rig_root);
    let mut out = HashMap::new();
    for entry in WalkDir::new(&canon)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped(e.path(), &canon))
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(rel) = relativize(entry.path(), &canon) else {
            continue;
        };
        if !is_eligible(Path::new(&rel)) {
            continue;
        }
        let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if registry.for_extension(ext).is_none() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Some(fp) = FileFingerprint::from_metadata(&meta) else {
            continue;
        };
        out.insert(rel, fp);
    }
    out
}

fn relativize(path: &Path, root: &Path) -> Option<String> {
    let stripped = path.strip_prefix(root).ok()?;
    Some(stripped.to_string_lossy().replace('\\', "/"))
}

fn is_skipped(path: &Path, root: &Path) -> bool {
    if path == root {
        return false;
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if matches!(name, "target" | "node_modules" | ".git") {
        return true;
    }
    name.starts_with('.') && name != "."
}

/// Result of comparing the walked-rig fingerprints against a snapshot.
/// `unchanged` files keep their pre-loaded nodes; `changed` need a
/// reindex; `removed` files vanished while the daemon was offline.
#[derive(Debug, Default)]
pub struct ReconcilePlan {
    pub unchanged: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl ReconcilePlan {
    pub fn is_noop(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }
}

pub fn diff_fingerprints(
    snapshot_fps: &HashMap<String, FileFingerprint>,
    fresh_fps: &HashMap<String, FileFingerprint>,
) -> ReconcilePlan {
    let mut plan = ReconcilePlan::default();
    for (rel, fresh) in fresh_fps {
        match snapshot_fps.get(rel) {
            Some(old) if old == fresh => plan.unchanged.push(rel.clone()),
            Some(_) | None => plan.changed.push(rel.clone()),
        }
    }
    for rel in snapshot_fps.keys() {
        if !fresh_fps.contains_key(rel) {
            plan.removed.push(rel.clone());
        }
    }
    plan.changed.sort();
    plan.removed.sort();
    plan.unchanged.sort();
    plan
}
