mod client;
mod models;
mod server;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use server::MementoMcp;

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("memento-mcp {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "memento-mcp {}\n\nLocal stdio MCP server for mementod.",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        Some(argument) => anyhow::bail!("unknown argument: {argument}"),
        None => {}
    }
    let service = MementoMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
