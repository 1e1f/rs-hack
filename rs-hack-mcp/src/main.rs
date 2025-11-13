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
