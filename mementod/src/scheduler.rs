use crate::manager::{ImportRequest, MementoManager, SyncResponse};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tracing::{info, warn};

const STARTUP_JOB_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DaemonConfigFile {
    pub vault: Option<VaultConfig>,
    pub vault_sync_runner: Option<VaultSyncRunnerConfig>,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VaultConfig {
    pub root: String,
    pub vault_sync_config: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VaultSyncRunnerConfig {
    #[serde(default)]
    pub command: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default)]
    pub enabled: bool,
    pub default_interval: Option<String>,
    #[serde(default)]
    pub run_on_start: bool,
    #[serde(default)]
    pub batch_updates: bool,
    #[serde(default)]
    pub jobs: Vec<ScheduledJobConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduledJobConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(rename = "type")]
    pub job_type: String,
    pub config: String,
    pub command: String,
    pub interval: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerSnapshot {
    pub enabled: bool,
    pub jobs: Vec<ScheduledJobState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJobState {
    pub name: String,
    pub enabled: bool,
    pub job_type: String,
    pub interval: String,
    pub command: String,
    pub config: String,
    pub running: bool,
    pub last_started_unix_ms: Option<u64>,
    pub last_finished_unix_ms: Option<u64>,
    pub next_run_unix_ms: Option<u64>,
    pub last_duration_ms: Option<u64>,
    pub last_result: Option<String>,
}

fn default_enabled() -> bool {
    true
}

pub fn load_daemon_config(data_dir: &Path) -> Result<Option<DaemonConfigFile>> {
    let path = data_dir.join("config").join("daemon.toml");
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read daemon config {}", path.display()))?;
    let config = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse daemon config {}", path.display()))?;
    Ok(Some(config))
}

pub async fn start_scheduler(manager: Arc<MementoManager>, data_dir: PathBuf) -> Result<()> {
    let Some(config) = load_daemon_config(&data_dir)? else {
        manager
            .set_scheduler_snapshot(SchedulerSnapshot::default())
            .await;
        return Ok(());
    };

    let enabled = config.scheduler.enabled && !config.scheduler.jobs.is_empty();
    let snapshot = SchedulerSnapshot {
        enabled,
        jobs: config
            .scheduler
            .jobs
            .iter()
            .map(|job| ScheduledJobState {
                name: job.name.clone(),
                enabled: job.enabled,
                job_type: job.job_type.clone(),
                interval: job
                    .interval
                    .clone()
                    .or_else(|| config.scheduler.default_interval.clone())
                    .unwrap_or_else(|| "8h".to_string()),
                command: job.command.clone(),
                config: job.config.clone(),
                running: false,
                last_started_unix_ms: None,
                last_finished_unix_ms: None,
                next_run_unix_ms: None,
                last_duration_ms: None,
                last_result: None,
            })
            .collect(),
    };
    manager.set_scheduler_snapshot(snapshot).await;

    if !enabled {
        return Ok(());
    }

    let Some(vault) = config.vault.clone() else {
        warn!("scheduler enabled but [vault] section is missing");
        return Ok(());
    };
    let runner = config.vault_sync_runner.clone();

    for job in config.scheduler.jobs {
        let manager = Arc::clone(&manager);
        let vault = vault.clone();
        let runner = runner.clone();
        let default_interval = config.scheduler.default_interval.clone();
        let run_on_start = config.scheduler.run_on_start;
        let batch_updates = config.scheduler.batch_updates;
        tokio::spawn(async move {
            if let Err(err) = scheduler_loop(
                manager,
                vault,
                runner,
                job,
                default_interval,
                run_on_start,
                batch_updates,
            )
            .await
            {
                warn!("scheduler loop failed: {err}");
            }
        });
    }

    Ok(())
}

async fn scheduler_loop(
    manager: Arc<MementoManager>,
    vault: VaultConfig,
    runner: Option<VaultSyncRunnerConfig>,
    job: ScheduledJobConfig,
    default_interval: Option<String>,
    run_on_start: bool,
    batch_updates: bool,
) -> Result<()> {
    let interval_text = job
        .interval
        .clone()
        .or(default_interval)
        .unwrap_or_else(|| "8h".to_string());
    let interval = parse_interval(&interval_text)?;

    if !job.enabled {
        manager
            .update_scheduled_job(&job.name, |state| {
                state.last_result = Some("disabled".to_string());
            })
            .await;
        return Ok(());
    }

    if run_on_start {
        // Let the health/status surface become available before a feeder or
        // learning pass starts competing for CPU and runtime state.
        tokio::time::sleep(STARTUP_JOB_GRACE).await;
        run_job_once(&manager, &vault, runner.as_ref(), &job, batch_updates).await;
    }

    loop {
        manager
            .update_scheduled_job(&job.name, |state| {
                state.next_run_unix_ms = Some(now_unix_ms() + interval.as_millis() as u64);
            })
            .await;
        tokio::time::sleep(interval).await;
        run_job_once(&manager, &vault, runner.as_ref(), &job, batch_updates).await;
    }
}

async fn run_job_once(
    manager: &Arc<MementoManager>,
    vault: &VaultConfig,
    runner: Option<&VaultSyncRunnerConfig>,
    job: &ScheduledJobConfig,
    batch_updates: bool,
) {
    let started_at = now_unix_ms();
    manager
        .update_scheduled_job(&job.name, |state| {
            state.running = true;
            state.last_started_unix_ms = Some(started_at);
            state.last_result = None;
            state.next_run_unix_ms = None;
        })
        .await;

    let timer = Instant::now();
    let result = execute_job(manager, vault, runner, job, batch_updates).await;
    let finished_at = now_unix_ms();
    let duration_ms = timer.elapsed().as_millis() as u64;

    manager
        .update_scheduled_job(&job.name, |state| {
            state.running = false;
            state.last_finished_unix_ms = Some(finished_at);
            state.last_duration_ms = Some(duration_ms);
            state.last_result = Some(match &result {
                Ok(message) => format!("ok: {message}"),
                Err(err) => format!("error: {err}"),
            });
        })
        .await;

    match result {
        Ok(message) => info!("scheduler job {} completed: {}", job.name, message),
        Err(err) => warn!("scheduler job {} failed: {}", job.name, err),
    }
}

async fn execute_job(
    manager: &Arc<MementoManager>,
    vault: &VaultConfig,
    runner: Option<&VaultSyncRunnerConfig>,
    job: &ScheduledJobConfig,
    batch_updates: bool,
) -> Result<String> {
    match job.job_type.as_str() {
        "vault_sync" => run_vault_sync_job(manager, vault, runner, job, batch_updates).await,
        other => Err(anyhow::anyhow!("unsupported scheduler job type `{other}`")),
    }
}

async fn run_vault_sync_job(
    manager: &Arc<MementoManager>,
    vault: &VaultConfig,
    runner: Option<&VaultSyncRunnerConfig>,
    job: &ScheduledJobConfig,
    _batch_updates: bool,
) -> Result<String> {
    let feeder_summary = run_vault_sync_command(runner, job, vault).await?;
    let sync = manager
        .sync(&ImportRequest {
            source: "folder".to_string(),
            path: Some(vault.root.clone()),
        })
        .await?;
    Ok(summarize_job_result(&feeder_summary, &sync))
}

async fn run_vault_sync_command(
    runner: Option<&VaultSyncRunnerConfig>,
    job: &ScheduledJobConfig,
    vault: &VaultConfig,
) -> Result<String> {
    let config_path = if job.config.trim().is_empty() {
        vault.vault_sync_config.as_str()
    } else {
        job.config.as_str()
    };
    let (binary, args, working_dir) = build_vault_sync_command(runner)?;
    let mut command = Command::new(binary);
    command.args(args);
    command.args(["--config", config_path, &job.command]);
    if let Some(working_dir) = working_dir {
        command.current_dir(working_dir);
    }
    let output = command
        .env("MEMENTO_VAULT_ROOT", &vault.root)
        .output()
        .await
        .context("failed to launch vault_sync runner")?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(anyhow::anyhow!("vault_sync runner failed: {}", detail));
    }

    Ok(if stdout.is_empty() {
        "vault feeder completed".to_string()
    } else {
        stdout
            .lines()
            .last()
            .unwrap_or("vault feeder completed")
            .to_string()
    })
}

fn summarize_job_result(feeder_summary: &str, sync: &SyncResponse) -> String {
    let mut message = format!(
        "{feeder_summary}; sync chunks={} files +{} ~{} -{} ={}",
        sync.chunks_synced,
        sync.added_files,
        sync.updated_files,
        sync.removed_files,
        sync.unchanged_files,
    );
    message.push_str(&format!("; learn coherence {:.3}", sync.coherence_after));
    message
}

fn parse_interval(raw: &str) -> Result<Duration> {
    let raw = raw.trim();
    if raw.len() < 2 {
        anyhow::bail!("invalid scheduler interval `{raw}`");
    }
    let (value, unit) = raw.split_at(raw.len() - 1);
    let amount: u64 = value
        .parse()
        .with_context(|| format!("invalid scheduler interval `{raw}`"))?;
    match unit {
        "m" => Ok(Duration::from_secs(amount * 60)),
        "h" => Ok(Duration::from_secs(amount * 60 * 60)),
        "d" => Ok(Duration::from_secs(amount * 60 * 60 * 24)),
        _ => Err(anyhow::anyhow!(
            "unsupported scheduler interval unit `{unit}` in `{raw}`"
        )),
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn build_vault_sync_command(
    runner: Option<&VaultSyncRunnerConfig>,
) -> Result<(String, Vec<String>, Option<PathBuf>)> {
    if let Some(runner) = runner {
        if runner.command.is_empty() {
            anyhow::bail!("vault_sync_runner.command is empty");
        }
        let working_dir = runner.working_dir.as_deref().map(expand_user_path);
        return Ok((
            runner.command[0].clone(),
            runner.command[1..].to_vec(),
            working_dir,
        ));
    }

    let repo_root = resolve_repo_root()
        .context("could not locate repo root containing tools/vault_sync/cli.py")?;
    Ok((
        "uv".to_string(),
        vec![
            "run".to_string(),
            "python".to_string(),
            "-m".to_string(),
            "tools.vault_sync.cli".to_string(),
        ],
        Some(repo_root),
    ))
}

fn expand_user_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn resolve_repo_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MEMENTO_REPO_ROOT").map(PathBuf::from) {
        if path.join("tools/vault_sync/cli.py").exists() {
            return Some(path);
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(root) = find_repo_root(&current_dir) {
            return Some(root);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            if ancestor.join("tools/vault_sync/cli.py").exists() {
                return Some(ancestor.to_path_buf());
            }
        }
    }

    None
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join("tools/vault_sync/cli.py").exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_interval_supports_minutes_hours_and_days() {
        assert_eq!(parse_interval("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_interval("8h").unwrap(), Duration::from_secs(28800));
        assert_eq!(parse_interval("1d").unwrap(), Duration::from_secs(86400));
    }

    #[test]
    fn parse_interval_rejects_invalid_values() {
        assert!(parse_interval("abc").is_err());
        assert!(parse_interval("15x").is_err());
    }

    #[test]
    fn build_vault_sync_command_uses_explicit_runner_when_present() {
        let (binary, args, working_dir) = build_vault_sync_command(Some(&VaultSyncRunnerConfig {
            command: vec!["memento-vault-sync".to_string(), "--verbose".to_string()],
            working_dir: Some("~/tmp".to_string()),
        }))
        .unwrap();
        assert_eq!(binary, "memento-vault-sync");
        assert_eq!(args, vec!["--verbose".to_string()]);
        assert!(working_dir.unwrap().ends_with("tmp"));
    }
}
