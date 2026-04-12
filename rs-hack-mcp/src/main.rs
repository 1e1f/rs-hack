//! @arch:layer(mcp)
//! @arch:role(bridge)
//! @arch:thread(async_io)
//! @arch:depends_on(cli, reason = "shells out to rs-hack CLI binary")
//!
//! MCP server entry point. Initializes tracing and runs the
//! JSON-RPC stdio server that bridges AI tool calls to rs-hack CLI.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber;

mod mcp;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    info!("Starting rs-hack MCP server v{}", env!("CARGO_PKG_VERSION"));

    // Create and run the MCP server using stdio
    let server = mcp::Server::new();
    server.run().await?;

    Ok(())
}
