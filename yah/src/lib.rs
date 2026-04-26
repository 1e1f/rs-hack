//! @arch:layer(core)
//! @arch:role(refactor)
//!
//! yah — AI-agent harness library.
//!
//! Composed of:
//! - `core` refactoring primitives (operations, visitor, editor, diff, surgical, state)
//! - `arch` — architecture knowledge graph extracted from source annotations
//! - `mcp` — JSON-RPC stdio bridge for AI agents
//!
//! Subcommands: `yah hack` (refactor), `yah board` (tickets), `yah arch` (graph),
//! `yah mcp` (MCP server). Short binaries `yahh`/`yahb`/`yaha` ship as bin shims.

pub mod operations;
pub mod visitor;
pub mod editor;
pub mod diff;
pub mod path_resolver;
pub mod surgical;
pub mod state;

pub mod arch;
pub mod mcp;

#[cfg(test)]
mod tests;

pub use operations::*;
pub use editor::RustEditor;
pub use diff::{generate_unified_diff, print_diff, print_summary_diff, DiffStats};
pub use surgical::{Replacement, apply_surgical_edits};
pub use state::*;
