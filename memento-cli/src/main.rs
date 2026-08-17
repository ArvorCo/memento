use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

mod client;
mod commands;
mod config;

#[derive(Parser)]
#[command(
    name = "memento",
    about = "Memento CLI — semantic memory for AI agents",
    version = env!("CARGO_PKG_VERSION"),
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum QueryOutput {
    Human,
    Json,
    Compact,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate initial daemon and vault sync config
    #[command(visible_alias = "onboard")]
    Init {
        /// Platform preset: auto, mac, linux, windows
        #[arg(long, default_value = "auto")]
        preset: String,
        /// Vault root path to create or reuse
        #[arg(long)]
        vault_root: Option<String>,
        /// Default sync interval stored in daemon config
        #[arg(long, default_value = "8h")]
        schedule: String,
        /// Overwrite changed config files instead of reusing them
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Validate config, feeder paths, and runtime health
    Doctor,
    /// Import sessions or files into memory
    Import {
        /// Source: claude, codex, file, folder, obsidian
        source: String,
        /// Path (required for file/folder)
        path: Option<String>,
        /// Emit the daemon response as JSON
        #[arg(long)]
        json: bool,
    },
    /// Sync an existing source without duplicating content
    Sync {
        /// Source: claude, codex, file, folder, obsidian
        source: String,
        /// Path (required for file/folder/obsidian)
        path: Option<String>,
        /// Emit the daemon response as JSON
        #[arg(long)]
        json: bool,
    },
    /// Query your memories
    Query {
        /// Question or search query
        question: String,
        /// Max results
        #[arg(short, long, default_value = "5")]
        limit: usize,
        /// Output for humans, complete JSON, or token-efficient compact JSON
        #[arg(long, value_enum, default_value_t = QueryOutput::Human)]
        output: QueryOutput,
        /// Maximum characters of evidence per result in compact output
        #[arg(long, default_value_t = 800)]
        max_content_chars: usize,
    },
    /// Show memory status
    Status {
        /// Emit the daemon response as JSON
        #[arg(long)]
        json: bool,
    },
    /// Recompute eigenvectors (learn from ingested data)
    Learn {
        /// Emit the daemon response as JSON
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            preset,
            vault_root,
            schedule,
            force,
        } => commands::init::run(&preset, vault_root.as_deref(), &schedule, force).await,
        Commands::Doctor => commands::doctor::run().await,
        command => {
            client::ensure_daemon_running(std::time::Duration::from_secs(90)).await?;
            match command {
                Commands::Sync { source, path, json } => {
                    commands::sync::run(&source, path.as_deref(), json).await
                }
                Commands::Query {
                    question,
                    limit,
                    output,
                    max_content_chars,
                } => commands::query::run(&question, limit, output, max_content_chars).await,
                Commands::Status { json } => commands::status::run(json).await,
                Commands::Learn { json } => commands::learn::run(json).await,
                Commands::Import { source, path, json } => {
                    commands::import::run(&source, path.as_deref(), json).await
                }
                Commands::Doctor => unreachable!(),
                Commands::Init { .. } => unreachable!(),
            }
        }
    }
}
