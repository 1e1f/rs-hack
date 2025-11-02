pub mod operations;
pub mod visitor;
pub mod editor;
pub mod diff;

#[cfg(test)]
mod tests;

pub use operations::*;
pub use editor::RustEditor;
pub use diff::{generate_unified_diff, print_diff, DiffStats};
