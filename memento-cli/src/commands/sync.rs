use anyhow::Result;
use colored::Colorize;

use crate::client;

pub async fn run(source: &str, path: Option<&str>, json_output: bool) -> Result<()> {
    println!("{} Syncing {} ...", ">>".cyan(), source.bold());

    let body = serde_json::json!({
        "source": source,
        "path": path,
    });

    let resp = client::post("/sync", &body.to_string()).await?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&resp)?)?
        );
        return Ok(());
    }
    let data: serde_json::Value = serde_json::from_str(&resp)?;

    let chunks = data["chunks_synced"].as_u64().unwrap_or(0);
    let removed_chunks = data["removed_chunks"].as_u64().unwrap_or(0);
    let added_files = data["added_files"].as_u64().unwrap_or(0);
    let updated_files = data["updated_files"].as_u64().unwrap_or(0);
    let removed_files = data["removed_files"].as_u64().unwrap_or(0);
    let unchanged_files = data["unchanged_files"].as_u64().unwrap_or(0);
    let coherence = data["coherence_after"].as_f64().unwrap_or(0.0);
    let eigenvectors = data["eigenvectors_computed"].as_u64().unwrap_or(0);

    println!(
        "{}  Synced {} chunks from {}.",
        "OK".green().bold(),
        chunks.to_string().bold(),
        source.cyan()
    );
    println!(
        "   files: +{} ~{} -{} ={}  removed chunks: {}  coherence: {:.3}  eigenvectors: {}",
        added_files.to_string().bold(),
        updated_files.to_string().bold(),
        removed_files.to_string().bold(),
        unchanged_files.to_string().bold(),
        removed_chunks.to_string().bold(),
        coherence,
        eigenvectors.to_string().bold()
    );

    Ok(())
}
