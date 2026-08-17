use anyhow::Result;
use colored::Colorize;

use crate::client;
use crate::config;

fn onboarding_summary() -> Option<(String, String)> {
    let config = config::load_daemon_config().ok()??;
    let interval = config.scheduler.default_interval?;
    let sync_config = config
        .vault
        .map(|vault| vault.vault_sync_config)
        .unwrap_or_else(|| {
            config::default_vault_sync_config_path()
                .display()
                .to_string()
        });
    Some((interval, sync_config))
}

fn manual_sync_command() -> String {
    let Some(config) = config::load_daemon_config().ok().flatten() else {
        return format!(
            "uv run python -m tools.vault_sync.cli --config {} run-all",
            config::default_vault_sync_config_path().display()
        );
    };

    let sync_config = config
        .vault
        .as_ref()
        .map(|vault| vault.vault_sync_config.clone())
        .unwrap_or_else(|| {
            config::default_vault_sync_config_path()
                .display()
                .to_string()
        });
    let mut parts = config
        .vault_sync_runner
        .map(|runner| runner.command)
        .filter(|command| !command.is_empty())
        .unwrap_or_else(|| {
            vec![
                "uv".to_string(),
                "run".to_string(),
                "python".to_string(),
                "-m".to_string(),
                "tools.vault_sync.cli".to_string(),
            ]
        });
    parts.push("--config".to_string());
    parts.push(sync_config);
    parts.push("run-all".to_string());
    parts.join(" ")
}

fn format_relative_unix_ms(target_unix_ms: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let diff_ms = target_unix_ms.abs_diff(now);
    let total_minutes = diff_ms / 1000 / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    let detail = if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    };

    if target_unix_ms >= now {
        format!("in {detail}")
    } else {
        format!("{detail} ago")
    }
}

fn format_size_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1} GiB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

pub async fn run(json_output: bool) -> Result<()> {
    let resp = client::get("/status").await?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string(&serde_json::from_str::<serde_json::Value>(&resp)?)?
        );
        return Ok(());
    }
    let data: serde_json::Value = serde_json::from_str(&resp)?;

    let vocab = data["vocabulary_size"].as_u64().unwrap_or(0);
    let nnz = data["non_zero_count"].as_u64().unwrap_or(0);
    let coherence = data["coherence_score"].as_f64().unwrap_or(0.0);
    let chunks = data["total_chunks"].as_u64().unwrap_or(0);
    let sources = data["total_sources"].as_u64().unwrap_or(0);
    let graph_edges = data["document_graph_edges"].as_u64().unwrap_or(0);
    let domain = data["domain"].as_str().unwrap_or("default");
    let runtime_generation = data["runtime_manifest_generation"].as_u64().unwrap_or(0);
    let runtime_segments = data["runtime_segment_count"].as_u64().unwrap_or(0);
    let runtime_ready = data["runtime_segments_ready"].as_bool().unwrap_or(false);
    let runtime_graph_ready = data["runtime_graph_ready"].as_bool().unwrap_or(false);
    let runtime_embedding_ready = data["runtime_embedding_ready"].as_bool().unwrap_or(false);
    let active_operation = data["active_operation"].as_object();

    println!("{}", "Memento Status".bold().cyan());
    println!("{}", "-".repeat(40));
    println!("  {:<20} {}", "Domain:".dimmed(), domain.bold());
    println!(
        "  {:<20} {}",
        "Vocabulary:".dimmed(),
        vocab.to_string().bold()
    );
    println!(
        "  {:<20} {}",
        "Co-occurrences:".dimmed(),
        nnz.to_string().bold()
    );
    println!("  {:<20} {:.3}", "Coherence:".dimmed(), coherence);
    println!("  {:<20} {}", "Chunks:".dimmed(), chunks.to_string().bold());
    println!(
        "  {:<20} {}",
        "Sources:".dimmed(),
        sources.to_string().bold()
    );
    println!(
        "  {:<20} {}",
        "Document Links:".dimmed(),
        graph_edges.to_string().bold()
    );
    println!(
        "  {:<20} {}",
        "Runtime Kernel:".dimmed(),
        if runtime_ready {
            "ready".green().bold()
        } else {
            "legacy-only".yellow().bold()
        }
    );
    println!(
        "  {:<20} {}",
        "Manifest Gen:".dimmed(),
        runtime_generation.to_string().bold()
    );
    println!(
        "  {:<20} {}",
        "Segments:".dimmed(),
        runtime_segments.to_string().bold()
    );
    println!(
        "  {:<20} {}",
        "Graph Segment:".dimmed(),
        if runtime_graph_ready {
            "ready".green().bold()
        } else {
            "missing".yellow().bold()
        }
    );
    println!(
        "  {:<20} {}",
        "Embedding Seg:".dimmed(),
        if runtime_embedding_ready {
            "ready".green().bold()
        } else {
            "missing".yellow().bold()
        }
    );
    println!("{}", "-".repeat(40));

    if let Some((interval, sync_config)) = onboarding_summary() {
        println!("{}", "Configured Sync".bold().blue());
        println!(
            "  {:<20} {}",
            "Interval:".dimmed(),
            format!("every {interval}").bold()
        );
        println!("  {:<20} {}", "Config:".dimmed(), sync_config.bold());
        println!("{}", "-".repeat(40));
    }

    let scheduled_jobs = data["scheduled_jobs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if data["scheduler_enabled"].as_bool().unwrap_or(false) && !scheduled_jobs.is_empty() {
        println!("{}", "Scheduler".bold().magenta());
        for job in scheduled_jobs {
            let name = job["name"].as_str().unwrap_or("job");
            let interval = job["interval"].as_str().unwrap_or("unknown");
            let running = job["running"].as_bool().unwrap_or(false);
            let next_run = job["next_run_unix_ms"].as_u64();
            let last_result = job["last_result"].as_str().unwrap_or("pending");
            let label = if running {
                "running".yellow().bold()
            } else {
                "idle".green().bold()
            };
            println!(
                "  {:<20} {}  every {}",
                format!("{name}:").dimmed(),
                label,
                interval.bold()
            );
            if let Some(next_run) = next_run {
                println!(
                    "  {:<20} {}",
                    "Next Run:".dimmed(),
                    format_relative_unix_ms(next_run).bold()
                );
            }
            println!("  {:<20} {}", "Last Result:".dimmed(), last_result);
        }
        println!("{}", "-".repeat(40));
    }

    if let Some(operation) = active_operation {
        let kind = operation
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or("operation");
        let source_type = operation
            .get("source_type")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let phase = operation
            .get("phase")
            .and_then(|value| value.as_str())
            .unwrap_or("starting");
        let status = operation
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("running");
        let processed = operation
            .get("processed_files")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let total = operation
            .get("total_files")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let batches = operation
            .get("completed_batches")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let batch_total = operation
            .get("total_batches")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let chunks_written = operation
            .get("chunks_written")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let current_file_path = operation
            .get("current_file_path")
            .and_then(|value| value.as_str());
        let current_file_size = operation
            .get("current_file_size_bytes")
            .and_then(|value| value.as_u64());

        println!("{}", "Active Operation".bold().yellow());
        println!(
            "  {:<20} {} {} ({})",
            "Kind:".dimmed(),
            kind.bold(),
            source_type.cyan(),
            status
        );
        println!("  {:<20} {}", "Phase:".dimmed(), phase.bold());
        println!(
            "  {:<20} {}/{}",
            "Files:".dimmed(),
            processed.to_string().bold(),
            total.to_string().bold()
        );
        println!(
            "  {:<20} {}/{}",
            "Batches:".dimmed(),
            batches.to_string().bold(),
            batch_total.to_string().bold()
        );
        println!(
            "  {:<20} {}",
            "Chunks Written:".dimmed(),
            chunks_written.to_string().bold()
        );
        if let Some(path) = current_file_path {
            let label = current_file_size
                .map(format_size_bytes)
                .unwrap_or_else(|| "unknown size".to_string());
            println!("  {:<20} {}", "Current File:".dimmed(), path.bold());
            println!("  {:<20} {}", "Current Size:".dimmed(), label.bold());
        }
        println!("{}", "-".repeat(40));
    }

    if chunks == 0 {
        println!(
            "\n  Run {} if you have not configured Memento yet.",
            "memento init".bold()
        );
        println!(
            "  Run {} for a full health check and scheduler validation.",
            "memento doctor".bold()
        );
        println!(
            "  Run {} if you want an immediate feeder pass before waiting for the scheduler.",
            manual_sync_command().bold()
        );
    }

    Ok(())
}
