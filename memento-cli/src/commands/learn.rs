use anyhow::Result;
use colored::Colorize;

use crate::client;

pub async fn run(json_output: bool) -> Result<()> {
    if !json_output {
        println!("{} Recomputing eigenvectors...", ">>".cyan());
    }

    let resp = client::post("/learn", "{}").await?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&resp)?)?
        );
        return Ok(());
    }
    let data: serde_json::Value = serde_json::from_str(&resp)?;

    let before = data["coherence_before"].as_f64().unwrap_or(0.0);
    let after = data["coherence_after"].as_f64().unwrap_or(0.0);
    let eigenvectors = data["eigenvectors_computed"].as_u64().unwrap_or(0);

    println!(
        "{}  Learned! Coherence: {:.3} -> {:.3}  ({} eigenvectors)",
        "OK".green().bold(),
        before,
        after,
        eigenvectors.to_string().bold()
    );

    Ok(())
}
