//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Path utilities. The store keys nodes by *rig-relative* paths with
//! forward slashes; the watcher and Tauri commands hand us *absolute*
//! paths off the host filesystem. This module is the canonical place
//! to canonicalize the rig root and translate between the two.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Canonicalize the rig root, falling back to the input on failure.
pub fn canonicalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// Convert an absolute path to a rig-relative `String` with forward
/// slashes. Returns `None` if the path is outside the rig root.
///
/// Plain `path.canonicalize()` won't do here: when the watcher tells
/// us about a *just-deleted* file, the syscall errors and falls back
/// to the un-symlinked input — which on macOS is `/var/...` instead
/// of `/private/var/...`, so the subsequent `strip_prefix` against
/// the canonical root misses and reindex silently no-ops. Walk up
/// until we find an ancestor that does exist, canonicalize that, and
/// reattach the missing tail.
pub fn relativize(path: &Path, root: &Path) -> Option<String> {
    let canon_path = canonicalize_existing_prefix(path);
    let stripped = canon_path.strip_prefix(root).ok()?;
    Some(stripped.to_string_lossy().replace('\\', "/"))
}

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    if let Ok(canon) = path.canonicalize() {
        return canon;
    }
    let mut popped: Vec<OsString> = Vec::new();
    let mut cursor = path;
    while let Some(parent) = cursor.parent() {
        if parent.as_os_str().is_empty() || parent == cursor {
            break;
        }
        let Some(name) = cursor.file_name() else {
            break;
        };
        popped.push(name.to_os_string());
        if let Ok(canon_parent) = parent.canonicalize() {
            let mut out = canon_parent;
            for seg in popped.iter().rev() {
                out.push(seg);
            }
            return out;
        }
        cursor = parent;
    }
    path.to_path_buf()
}

/// `true` when the path looks like one we should be reindexing — has an
/// extension and isn't inside a skipped directory (`target/`, `.git/`,
/// `node_modules/`).
pub fn is_eligible(path: &Path) -> bool {
    if path.extension().is_none() {
        return false;
    }
    for component in path.components() {
        let Some(s) = component.as_os_str().to_str() else {
            continue;
        };
        if matches!(s, "target" | ".git" | "node_modules") {
            return false;
        }
        if s.starts_with('.') && s != "." && s != ".." {
            return false;
        }
    }
    true
}
