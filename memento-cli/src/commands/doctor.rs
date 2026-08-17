use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::client;
use crate::config as memento_config;
use crate::config::{
    self, AppleNotesConfig, DaemonConfigFile, DatabaseImportSourceConfig,
    DocumentImportSourceConfig, IcloudSyncConfig, ScheduledJobConfig, SessionImportConfig,
    VaultSyncConfigFile, VaultSyncRunnerConfig, WhatsappImportConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckLevel {
    Pass,
    Warn,
    Fail,
}

impl CheckLevel {
    fn label(self) -> colored::ColoredString {
        match self {
            Self::Pass => "PASS".green().bold(),
            Self::Warn => "WARN".yellow().bold(),
            Self::Fail => "FAIL".red().bold(),
        }
    }
}

#[derive(Debug, Clone)]
struct CheckResult {
    level: CheckLevel,
    label: String,
    detail: String,
}

impl CheckResult {
    fn pass(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: CheckLevel::Pass,
            label: label.into(),
            detail: detail.into(),
        }
    }

    fn warn(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: CheckLevel::Warn,
            label: label.into(),
            detail: detail.into(),
        }
    }

    fn fail(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: CheckLevel::Fail,
            label: label.into(),
            detail: detail.into(),
        }
    }
}

pub async fn run() -> Result<()> {
    let mut checks = Vec::new();

    let daemon_config = match config::load_daemon_config() {
        Ok(Some(daemon_config)) => {
            checks.push(CheckResult::pass(
                "daemon.toml",
                format!("loaded {}", memento_config::daemon_config_path().display()),
            ));
            Some(daemon_config)
        }
        Ok(None) => {
            checks.push(CheckResult::fail(
                "daemon.toml",
                "missing; run `memento init` first",
            ));
            None
        }
        Err(err) => {
            checks.push(CheckResult::fail(
                "daemon.toml",
                format!("invalid config: {err}"),
            ));
            None
        }
    };

    if let Some(config) = daemon_config.as_ref() {
        checks.extend(check_daemon_config(config));
        let vault_sync_path = config
            .vault
            .as_ref()
            .map(|vault| memento_config::expand_user_path(&vault.vault_sync_config))
            .unwrap_or_else(memento_config::default_vault_sync_config_path);
        match config::load_vault_sync_config(&vault_sync_path) {
            Ok(Some(parsed)) => {
                checks.push(CheckResult::pass(
                    "vault_sync.toml",
                    format!("loaded {}", vault_sync_path.display()),
                ));
                checks.extend(check_vault_sync_config(&parsed));
            }
            Ok(None) => checks.push(CheckResult::fail(
                "vault_sync.toml",
                format!("missing {}", vault_sync_path.display()),
            )),
            Err(err) => checks.push(CheckResult::fail(
                "vault_sync.toml",
                format!("invalid config: {err}"),
            )),
        }
    }

    checks.extend(check_runtime().await);

    print_report(&checks);

    let failures = checks
        .iter()
        .filter(|check| check.level == CheckLevel::Fail)
        .count();
    if failures > 0 {
        anyhow::bail!("doctor found {failures} failure(s)");
    }

    Ok(())
}

fn check_daemon_config(config: &DaemonConfigFile) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    if let Some(runtime) = config.daemon.as_ref() {
        if let Some(data_dir) = runtime.data_dir.as_deref() {
            checks.push(check_parent_path(
                "daemon data dir",
                &memento_config::expand_user_path(data_dir),
            ));
        }
        if let Some(socket_path) = runtime.socket_path.as_deref() {
            checks.push(check_parent_path(
                "daemon socket path",
                &memento_config::expand_user_path(socket_path),
            ));
        }
    }

    if let Some(vault) = config.vault.as_ref() {
        let root = memento_config::expand_user_path(&vault.root);
        checks.push(check_existing_path("vault root", &root));
    } else {
        checks.push(CheckResult::fail(
            "[vault]",
            "missing vault root and vault_sync_config",
        ));
    }

    if config.scheduler.enabled {
        if config.scheduler.jobs.is_empty() {
            checks.push(CheckResult::fail("scheduler", "enabled but has zero jobs"));
        } else {
            checks.push(CheckResult::pass(
                "scheduler",
                format!(
                    "{} job(s), default interval {}, run_on_start={}, batch_updates={}",
                    config.scheduler.jobs.len(),
                    config
                        .scheduler
                        .default_interval
                        .as_deref()
                        .unwrap_or("unset"),
                    config.scheduler.run_on_start,
                    config.scheduler.batch_updates,
                ),
            ));
            for job in &config.scheduler.jobs {
                checks.push(check_scheduler_job(job, config));
            }
        }
    } else {
        checks.push(CheckResult::warn(
            "scheduler",
            "disabled; background updates will not run",
        ));
    }

    checks.push(check_runner_config(config.vault_sync_runner.as_ref()));
    checks
}

fn check_scheduler_job(job: &ScheduledJobConfig, config: &DaemonConfigFile) -> CheckResult {
    let interval = job
        .interval
        .as_deref()
        .or(config.scheduler.default_interval.as_deref())
        .unwrap_or("");
    if !job.enabled {
        return CheckResult::warn(
            format!("job {}", job.name),
            format!("disabled ({})", job.job_type),
        );
    }
    if parse_interval(interval).is_err() {
        return CheckResult::fail(
            format!("job {}", job.name),
            format!("invalid interval `{interval}`"),
        );
    }
    if job.job_type != "vault_sync" {
        return CheckResult::fail(
            format!("job {}", job.name),
            format!("unsupported job type `{}`", job.job_type),
        );
    }
    CheckResult::pass(
        format!("job {}", job.name),
        format!("{} every {} using {}", job.command, interval, job.config),
    )
}

fn check_runner_config(config: Option<&VaultSyncRunnerConfig>) -> CheckResult {
    let Some(config) = config else {
        return CheckResult::warn(
            "vault_sync_runner",
            "missing; daemon will rely on repo auto-detection fallback",
        );
    };

    if config.command.is_empty() {
        return CheckResult::warn(
            "vault_sync_runner",
            "optional feeder is not installed; direct `memento import` and `memento sync` remain available",
        );
    }

    let executable = &config.command[0];
    if !command_exists(executable) {
        return CheckResult::fail(
            "vault_sync_runner",
            format!("command `{executable}` is not on PATH"),
        );
    }

    if let Some(working_dir) = config.working_dir.as_deref() {
        let dir = memento_config::expand_user_path(working_dir);
        if !dir.exists() {
            return CheckResult::fail(
                "vault_sync_runner",
                format!("working_dir does not exist: {}", dir.display()),
            );
        }
        return CheckResult::pass(
            "vault_sync_runner",
            format!("{} (cwd {})", config.command.join(" "), dir.display()),
        );
    }

    CheckResult::warn(
        "vault_sync_runner",
        format!(
            "{} (no working_dir set; packaged runner preferred)",
            config.command.join(" ")
        ),
    )
}

fn check_vault_sync_config(config: &VaultSyncConfigFile) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    if let Some(vault) = config.vault.as_ref() {
        checks.push(check_existing_path(
            "feeder vault root",
            &memento_config::expand_user_path(&vault.root),
        ));
        if let Some(state_dir) = vault.state_dir.as_deref() {
            checks.push(check_parent_path(
                "feeder state dir",
                &memento_config::expand_user_path(state_dir),
            ));
        }
    } else {
        checks.push(CheckResult::fail(
            "feeder [vault]",
            "missing vault root/state_dir section",
        ));
    }

    if config.markdown_sync.roots.is_empty() {
        checks.push(CheckResult::warn(
            "markdown roots",
            "none configured; only connector imports will run",
        ));
    } else {
        for root in &config.markdown_sync.roots {
            let source = memento_config::expand_user_path(&root.source);
            let mut check = check_existing_path(format!("markdown {}", root.name), &source);
            if let Some(destination) = root.destination.as_deref() {
                check.detail = format!("{} -> {}", check.detail, destination);
            }
            checks.push(check);
        }
    }

    if config.session_import.is_empty() {
        checks.push(CheckResult::warn("session imports", "none configured"));
    } else {
        for (name, source) in &config.session_import {
            checks.push(check_session_source(name, source));
        }
    }

    if let Some(linking) = config.linking.as_ref() {
        if linking.enabled {
            checks.push(CheckResult::pass(
                "wiki linker",
                format!(
                    "enabled (directory hub {}, root hub {}, tags={}, navigation={})",
                    linking.hub_filename.as_deref().unwrap_or("_memento_hub.md"),
                    linking.root_hub.as_deref().unwrap_or("_memento.md"),
                    linking.tag_hubs,
                    linking.inject_navigation,
                ),
            ));
        } else {
            checks.push(CheckResult::warn("wiki linker", "disabled"));
        }
    } else {
        checks.push(CheckResult::warn("wiki linker", "not configured"));
    }

    for source in &config.document_import.sources {
        checks.push(check_document_source(source));
    }
    for source in &config.database_import.sources {
        checks.push(check_database_source(source));
    }

    if let Some(icloud) = config.icloud_sync.as_ref() {
        checks.push(check_icloud_source(icloud));
    }
    if let Some(apple_notes) = config.apple_notes.as_ref() {
        checks.push(check_apple_notes(apple_notes));
    }
    if let Some(whatsapp) = config.whatsapp_import.as_ref() {
        checks.push(check_whatsapp_source(whatsapp));
    }

    checks
}

fn check_document_source(source: &DocumentImportSourceConfig) -> CheckResult {
    if !source.enabled {
        return CheckResult::warn(format!("documents {}", source.name), "disabled");
    }
    if !is_safe_vault_relative(&source.destination) {
        return CheckResult::fail(
            format!("documents {}", source.name),
            format!("destination escapes vault: {}", source.destination),
        );
    }
    let path = memento_config::expand_user_path(&source.source);
    if !path.exists() {
        return CheckResult::warn(
            format!("documents {}", source.name),
            format!("missing {}", path.display()),
        );
    }
    let needs_pandoc = source.include_extensions.iter().any(|extension| {
        matches!(
            extension.as_str(),
            ".doc" | ".docx" | ".odt" | ".ppt" | ".pptx" | ".rtf" | ".xlsx" | ".ipynb"
        )
    });
    if needs_pandoc && !command_exists("pandoc") {
        return CheckResult::warn(
            format!("documents {}", source.name),
            format!(
                "{} -> {}; pandoc missing, office conversions may fail",
                path.display(),
                source.destination
            ),
        );
    }
    CheckResult::pass(
        format!("documents {}", source.name),
        format!("{} -> {}", path.display(), source.destination),
    )
}

fn check_database_source(source: &DatabaseImportSourceConfig) -> CheckResult {
    if !source.enabled {
        return CheckResult::warn(format!("database {}", source.name), "disabled");
    }
    if !is_safe_vault_relative(&source.destination) {
        return CheckResult::fail(
            format!("database {}", source.name),
            format!("destination escapes vault: {}", source.destination),
        );
    }
    if source.driver == "sqlite" {
        let Some(database) = source.database.as_deref() else {
            return CheckResult::fail(format!("database {}", source.name), "sqlite path missing");
        };
        return check_existing_path(
            format!("database {}", source.name),
            &memento_config::expand_user_path(database),
        );
    }
    let Some(variable) = source.dsn_env.as_deref() else {
        return CheckResult::fail(
            format!("database {}", source.name),
            format!("{} requires dsn_env", source.driver),
        );
    };
    if env::var_os(variable).is_none() {
        return CheckResult::warn(
            format!("database {}", source.name),
            format!("environment variable {variable} is not set"),
        );
    }
    CheckResult::pass(
        format!("database {}", source.name),
        format!("{} via {variable} -> {}", source.driver, source.destination),
    )
}

fn is_safe_vault_relative(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
}

fn check_session_source(name: &str, source: &SessionImportConfig) -> CheckResult {
    if !source.enabled {
        return CheckResult::warn(format!("session {name}"), "disabled".to_string());
    }
    let mut check = check_existing_path(
        format!("session {}", source.label.as_deref().unwrap_or(name)),
        &memento_config::expand_user_path(&source.source),
    );
    if let Some(destination) = source.destination.as_deref() {
        check.detail = format!("{} -> {}", check.detail, destination);
    }
    check
}

fn check_icloud_source(config_value: &IcloudSyncConfig) -> CheckResult {
    if !config_value.enabled {
        return CheckResult::warn("icloud", "disabled");
    }
    let Some(root) = config_value.root.as_deref() else {
        return CheckResult::fail("icloud", "enabled but root is missing");
    };
    check_existing_path("icloud", &memento_config::expand_user_path(root))
}

fn check_apple_notes(config_value: &AppleNotesConfig) -> CheckResult {
    if !config_value.enabled {
        return CheckResult::warn("apple notes", "disabled");
    }
    if cfg!(target_os = "macos") {
        CheckResult::pass(
            "apple notes",
            format!(
                "enabled{}",
                config_value
                    .destination
                    .as_deref()
                    .map(|dest| format!(" -> {dest}"))
                    .unwrap_or_default()
            ),
        )
    } else {
        CheckResult::fail("apple notes", "enabled on non-macOS host")
    }
}

fn check_whatsapp_source(config_value: &WhatsappImportConfig) -> CheckResult {
    if !config_value.enabled {
        return CheckResult::warn("whatsapp", "disabled");
    }
    let Some(source) = config_value.source.as_deref() else {
        return CheckResult::fail("whatsapp", "enabled but source is missing");
    };
    let mut check = check_existing_path("whatsapp", &memento_config::expand_user_path(source));
    if let Some(destination) = config_value.destination.as_deref() {
        check.detail = format!("{} -> {}", check.detail, destination);
    }
    check
}

async fn check_runtime() -> Vec<CheckResult> {
    let mut checks = Vec::new();
    match client::get("/health").await {
        Ok(_) => {
            checks.push(CheckResult::pass("daemon health", "mementod is reachable"));
        }
        Err(err) => {
            if client::socket_path().exists() {
                checks.push(CheckResult::warn(
                    "daemon socket",
                    format!(
                        "socket exists at {} but health probe failed: {err}",
                        client::socket_path().display()
                    ),
                ));
            } else {
                checks.push(CheckResult::warn("daemon health", err.to_string()));
            }

            match client::ensure_daemon_running(Duration::from_secs(30)).await {
                Ok(()) => checks.push(CheckResult::pass(
                    "daemon autostart",
                    "started and became reachable",
                )),
                Err(start_err) => {
                    checks.push(CheckResult::fail(
                        "daemon autostart",
                        format!("failed to start: {start_err}"),
                    ));
                    return checks;
                }
            }
        }
    }

    match client::get("/status").await {
        Ok(resp) => match serde_json::from_str::<serde_json::Value>(&resp) {
            Ok(data) => {
                let chunks = data["total_chunks"].as_u64().unwrap_or(0);
                let sources = data["total_sources"].as_u64().unwrap_or(0);
                let scheduler = data["scheduled_jobs"]
                    .as_array()
                    .map(|jobs| jobs.len())
                    .unwrap_or(0);
                checks.push(CheckResult::pass(
                    "daemon status",
                    format!("{chunks} chunks, {sources} sources, {scheduler} scheduled job(s)"),
                ));
            }
            Err(err) => checks.push(CheckResult::fail(
                "daemon status",
                format!("invalid JSON payload: {err}"),
            )),
        },
        Err(err) => checks.push(CheckResult::fail(
            "daemon status",
            format!("status endpoint failed: {err}"),
        )),
    }

    checks
}

fn check_existing_path(label: impl Into<String>, path: &Path) -> CheckResult {
    let label = label.into();
    if path.exists() {
        CheckResult::pass(label, path.display().to_string())
    } else {
        CheckResult::warn(label, format!("missing {}", path.display()))
    }
}

fn check_parent_path(label: impl Into<String>, path: &Path) -> CheckResult {
    let label = label.into();
    let parent = path.parent().unwrap_or(path);
    if parent.exists() {
        CheckResult::pass(label, path.display().to_string())
    } else {
        CheckResult::warn(
            label,
            format!("parent directory missing {}", parent.display()),
        )
    }
}

fn parse_interval(raw: &str) -> Result<Duration> {
    let trimmed = raw.trim();
    if trimmed.len() < 2 {
        anyhow::bail!("invalid interval");
    }
    let (value, unit) = trimmed.split_at(trimmed.len() - 1);
    let amount: u64 = value.parse().context("interval amount is not numeric")?;
    if amount == 0 {
        anyhow::bail!("interval must be > 0");
    }
    match unit {
        "m" => Ok(Duration::from_secs(amount * 60)),
        "h" => Ok(Duration::from_secs(amount * 60 * 60)),
        "d" => Ok(Duration::from_secs(amount * 60 * 60 * 24)),
        _ => anyhow::bail!("unsupported interval unit"),
    }
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(command).exists();
    }

    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path_var).any(|dir| {
        let direct = dir.join(command);
        if direct.exists() {
            return true;
        }

        #[cfg(windows)]
        {
            if let Some(exts) = env::var_os("PATHEXT") {
                for ext in env::split_paths(&exts) {
                    let candidate = dir.join(format!("{}{}", command, ext.to_string_lossy()));
                    if candidate.exists() {
                        return true;
                    }
                }
            }
        }

        false
    })
}

fn print_report(checks: &[CheckResult]) {
    println!("{}", "Memento Doctor".bold().cyan());
    println!("{}", "-".repeat(52));
    for check in checks {
        println!(
            "  {} {:<18} {}",
            check.level.label(),
            format!("{}:", check.label).dimmed(),
            check.detail
        );
    }
    println!("{}", "-".repeat(52));

    let passes = checks
        .iter()
        .filter(|check| check.level == CheckLevel::Pass)
        .count();
    let warns = checks
        .iter()
        .filter(|check| check.level == CheckLevel::Warn)
        .count();
    let fails = checks
        .iter()
        .filter(|check| check.level == CheckLevel::Fail)
        .count();
    println!(
        "  Summary: {} pass, {} warn, {} fail",
        passes.to_string().green().bold(),
        warns.to_string().yellow().bold(),
        fails.to_string().red().bold()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_interval_accepts_minutes_hours_days() {
        assert_eq!(parse_interval("15m").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_interval("8h").unwrap(), Duration::from_secs(28800));
        assert_eq!(parse_interval("1d").unwrap(), Duration::from_secs(86400));
    }

    #[test]
    fn parse_interval_rejects_garbage() {
        assert!(parse_interval("0h").is_err());
        assert!(parse_interval("eight").is_err());
        assert!(parse_interval("5w").is_err());
    }

    #[test]
    fn runner_check_warns_when_optional_feeder_is_missing() {
        let check = check_runner_config(Some(&VaultSyncRunnerConfig {
            command: Vec::new(),
            working_dir: None,
        }));
        assert_eq!(check.level, CheckLevel::Warn);
    }
}
