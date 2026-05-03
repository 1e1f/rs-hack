//! Per-command lib API for rs-hack subcommands.
//!
//! Each module exposes `pub fn run(args: …Args) -> Result<…Result>` so embedders (MCP server,
//! yah, tests) can drive the same logic without shelling out. The CLI in `main.rs` is a thin
//! clap → struct → `run()` translator that adds rendering on top.

pub mod doc_coverage;
pub mod find;
pub mod match_audit;
pub mod neighbors;
pub mod summary;
