//! Core library for AST-aware Rust refactoring.
//! Re-exports operations, editor, diff, surgical edits, and state management.

pub mod commands;
pub mod diff;
pub mod editor;
pub mod execute;
pub mod files;
pub mod operations;
pub mod path_resolver;
pub mod state;
pub mod surgical;
pub mod visitor;

#[cfg(test)]
mod tests;

pub use diff::{generate_unified_diff, print_diff, print_summary_diff, DiffStats};
pub use editor::RustEditor;
pub use operations::*;
pub use state::*;
pub use surgical::{apply_surgical_edits, Replacement};
