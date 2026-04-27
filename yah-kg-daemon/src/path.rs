//! @arch:layer(kg_store)
//! @arch:role(graph)
//!
//! Path utilities. The store keys nodes by *rig-relative* paths with
//! forward slashes; the watcher and Tauri commands hand us *absolute*
//! paths off the host filesystem. This module is the canonical place
//! to canonicalize the rig root and translate between the two.

use std::path::{Path, PathBuf};

/// Canonicalize the rig root, falling back to the input on failure.
pub fn canonicalize_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// Convert an absolute path to a rig-relative `String` with forward
/// slashes. Returns `None` if the path is outside the rig root.
pub fn relativize(path: &Path, root: &Path) -> Option<String> {
    let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let stripped = canon_path.strip_prefix(root).ok()?;
    Some(stripped.to_string_lossy().replace('\\', "/"))
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
