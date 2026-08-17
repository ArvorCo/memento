use anyhow::Result;
use colored::Colorize;

use crate::client;
use crate::QueryOutput;

pub async fn run(
    question: &str,
    limit: usize,
    output: QueryOutput,
    max_content_chars: usize,
) -> Result<()> {
    let body = serde_json::json!({
        "query": question,
        "top_k": limit,
    });

    let resp = client::post("/query", &body.to_string()).await?;
    let data: serde_json::Value = serde_json::from_str(&resp)?;
    match output {
        QueryOutput::Json => {
            println!("{}", serde_json::to_string(&data)?);
            return Ok(());
        }
        QueryOutput::Compact => {
            println!(
                "{}",
                serde_json::to_string(&compact_response(&data, max_content_chars))?
            );
            return Ok(());
        }
        QueryOutput::Human => {}
    }

    let answer = data["answer"].as_str().unwrap_or("");
    let confidence = data["confidence"].as_f64().unwrap_or(0.0);
    let key_concepts = data["key_concepts"].as_array();
    let results = data["results"].as_array();

    if results.map(|r| r.is_empty()).unwrap_or(true) {
        println!("{} No results for: \"{}\"", "??".yellow(), question);
        return Ok(());
    }

    let results = results.unwrap();
    if !answer.is_empty() {
        println!("{}", "Answer".bold().cyan());
        println!("{}", "-".repeat(70));
        println!("{}", answer);
        println!();
    }

    if let Some(concepts) = key_concepts {
        let concepts: Vec<&str> = concepts
            .iter()
            .filter_map(|value| value.as_str())
            .take(6)
            .collect();
        if !concepts.is_empty() {
            println!("{} {}", "Concepts:".bold(), concepts.join(", ").dimmed());
            println!();
        }
    }

    println!(
        "{} Results for: {}  [confidence: {:.0}%]",
        ">>".cyan(),
        format!("\"{}\"", question).bold(),
        confidence * 100.0
    );
    println!("{}", "-".repeat(70));

    for (i, r) in results.iter().enumerate() {
        let score = r["score"].as_f64().unwrap_or(0.0);
        let content = r["content"].as_str().unwrap_or("");
        let source = r["source_path"].as_str().unwrap_or("");

        let score_pct = (score * 100.0) as u32;
        let score_str = if score_pct >= 75 {
            format!("{}%", score_pct).green().to_string()
        } else if score_pct >= 50 {
            format!("{}%", score_pct).yellow().to_string()
        } else {
            format!("{}%", score_pct).red().to_string()
        };

        println!(
            "\n  {} {}  {}",
            format!("[{}]", i + 1).bold(),
            score_str,
            source.dimmed()
        );

        for line in content.lines().take(6) {
            println!("  {}", line);
        }
        let total_lines = content.lines().count();
        if total_lines > 6 {
            println!("  {} [{} more lines]", "...".dimmed(), total_lines - 6);
        }
    }

    println!("\n{}", "-".repeat(70));
    println!("  {} results", results.len().to_string().bold());

    Ok(())
}

fn compact_response(data: &serde_json::Value, max_content_chars: usize) -> serde_json::Value {
    let evidence = data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|result| {
            serde_json::json!({
                "path": result["source_path"],
                "chunk": result["chunk_index"],
                "score": result["score"],
                "excerpt": truncate_chars(result["content"].as_str().unwrap_or(""), max_content_chars),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "answer": data["answer"],
        "confidence": data["confidence"],
        "concepts": data["key_concepts"],
        "evidence": evidence,
    })
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated = value.chars().take(limit).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_output_bounds_unicode_evidence_and_preserves_provenance() {
        let data = serde_json::json!({
            "answer": "grounded",
            "confidence": 0.8,
            "key_concepts": ["wiki"],
            "results": [{
                "source_path": "notes/ação.md",
                "chunk_index": 2,
                "score": 0.9,
                "content": "áβcdef"
            }]
        });

        let compact = compact_response(&data, 3);

        assert_eq!(compact["evidence"][0]["path"], "notes/ação.md");
        assert_eq!(compact["evidence"][0]["excerpt"], "áβc…");
    }
}
