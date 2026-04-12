//! @arch:layer(mcp)
//! @arch:role(bridge)
//!
//! MCP module: protocol types, server loop, and tool registry.

mod protocol;
mod server;
mod tools;

pub use server::Server;
