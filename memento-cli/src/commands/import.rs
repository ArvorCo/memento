use anyhow::Result;
use colored::Colorize;

use crate::client;

pub async fn run(source: &str, path: Option<&str>, json_output: bool) -> Result<()> {
    println!("{} Importing from {} ...", ">>".cyan(), source.bold());

    let body = serde_json::json!({
        "source": source,
        "path": path,
    });

    let resp = client::post("/import", &body.to_string()).await?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&resp)?)?
        );
        return Ok(());
    }
    let data: serde_json::Value = serde_json::from_str(&resp)?;

    let chunks = data["chunks_imported"].as_u64().unwrap_or(0);
    println!(
        "{}  Imported {} chunks from {}.",
        "OK".green().bold(),
        chunks.to_string().bold(),
        source.cyan()
    );

    Ok(())
}
