pub mod operations;
pub mod visitor;
pub mod editor;
pub mod diff;
pub mod path_resolver;
pub mod surgical;

#[cfg(test)]
mod tests;

pub use operations::*;
pub use editor::RustEditor;
pub use diff::{generate_unified_diff, print_diff, print_summary_diff, DiffStats};
pub use surgical::{Replacement, apply_surgical_edits};
