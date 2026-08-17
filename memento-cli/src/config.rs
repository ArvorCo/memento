use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub fn data_dir() -> PathBuf {
    std::env::var_os("MEMENTO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not determine home directory")
                .join(".memento")
        })
}

pub fn daemon_config_path() -> PathBuf {
    data_dir().join("config").join("daemon.toml")
}

pub fn default_vault_sync_config_path() -> PathBuf {
    data_dir().join("config").join("vault_sync.toml")
}

pub fn expand_user_path(raw: &str) -> PathBuf {
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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DaemonConfigFile {
    pub daemon: Option<DaemonRuntimeConfig>,
    pub vault: Option<VaultConfig>,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    pub vault_sync_runner: Option<VaultSyncRunnerConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DaemonRuntimeConfig {
    pub data_dir: Option<String>,
    pub socket_path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VaultConfig {
    pub root: String,
    pub vault_sync_config: String,
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

#[derive(Debug, Clone, Default, Deserialize)]
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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VaultSyncRunnerConfig {
    #[serde(default)]
    pub command: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VaultSyncConfigFile {
    pub vault: Option<VaultSyncVaultConfig>,
    #[serde(default)]
    pub markdown_sync: MarkdownSyncConfig,
    #[serde(default)]
    pub session_import: BTreeMap<String, SessionImportConfig>,
    pub linking: Option<LinkingConfig>,
    #[serde(default)]
    pub document_import: DocumentImportConfig,
    #[serde(default)]
    pub database_import: DatabaseImportConfig,
    pub icloud_sync: Option<IcloudSyncConfig>,
    pub apple_notes: Option<AppleNotesConfig>,
    pub whatsapp_import: Option<WhatsappImportConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkingConfig {
    #[serde(default)]
    pub enabled: bool,
    pub hub_filename: Option<String>,
    pub root_hub: Option<String>,
    #[serde(default)]
    pub tag_hubs: bool,
    #[serde(default)]
    pub inject_navigation: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DocumentImportConfig {
    #[serde(default)]
    pub sources: Vec<DocumentImportSourceConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DocumentImportSourceConfig {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub source: String,
    pub destination: String,
    #[serde(default)]
    pub include_extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatabaseImportConfig {
    #[serde(default)]
    pub sources: Vec<DatabaseImportSourceConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatabaseImportSourceConfig {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub driver: String,
    pub database: Option<String>,
    pub dsn_env: Option<String>,
    pub destination: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VaultSyncVaultConfig {
    pub root: String,
    pub state_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MarkdownSyncConfig {
    #[serde(default)]
    pub roots: Vec<MarkdownRootConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MarkdownRootConfig {
    pub name: String,
    pub source: String,
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionImportConfig {
    #[serde(default)]
    pub enabled: bool,
    pub source: String,
    pub destination: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IcloudSyncConfig {
    #[serde(default)]
    pub enabled: bool,
    pub root: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppleNotesConfig {
    #[serde(default)]
    pub enabled: bool,
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WhatsappImportConfig {
    #[serde(default)]
    pub enabled: bool,
    pub source: Option<String>,
    pub destination: Option<String>,
}

fn default_enabled() -> bool {
    true
}

pub fn load_daemon_config() -> Result<Option<DaemonConfigFile>> {
    load_toml_file(&daemon_config_path())
}

pub fn load_vault_sync_config(path: &Path) -> Result<Option<VaultSyncConfigFile>> {
    load_toml_file(path)
}

fn load_toml_file<T>(path: &Path) -> Result<Option<T>>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config {}", path.display()))?;
    let parsed = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config {}", path.display()))?;
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_user_path_supports_tilde_prefix() {
        let expanded = expand_user_path("~/tmp/memento");
        assert!(expanded.ends_with("tmp/memento"));
        assert!(expanded.is_absolute());
    }

    #[test]
    fn daemon_config_deserializes_runner_and_jobs() {
        let raw = r#"
[vault]
root = "/tmp/vault"
vault_sync_config = "/tmp/vault_sync.toml"

[scheduler]
enabled = true
default_interval = "8h"

[[scheduler.jobs]]
name = "vault-sync"
type = "vault_sync"
config = "/tmp/vault_sync.toml"
command = "run-all"

[vault_sync_runner]
command = ["uv", "run", "python", "-m", "tools.vault_sync.cli"]
working_dir = "/tmp/repo"
"#;
        let parsed: DaemonConfigFile = toml::from_str(raw).unwrap();
        assert_eq!(parsed.scheduler.jobs.len(), 1);
        let runner = parsed.vault_sync_runner.unwrap();
        assert_eq!(runner.command[0], "uv");
        assert_eq!(runner.working_dir.as_deref(), Some("/tmp/repo"));
    }

    #[test]
    fn vault_sync_config_deserializes_new_sources_and_minimal_disabled_connectors() {
        let raw = r#"
[vault]
root = "/tmp/vault"

[linking]
enabled = true
hub_filename = "index.md"

[[document_import.sources]]
name = "research"
enabled = true
source = "/tmp/documents"
destination = "documents"
include_extensions = [".pdf"]

[[database_import.sources]]
name = "decisions"
enabled = true
driver = "sqlite"
database = "/tmp/decisions.db"
destination = "databases/decisions"

[icloud_sync]
enabled = false

[whatsapp_import]
enabled = false
"#;

        let parsed: VaultSyncConfigFile = toml::from_str(raw).unwrap();

        assert_eq!(parsed.document_import.sources[0].name, "research");
        assert_eq!(parsed.database_import.sources[0].driver, "sqlite");
        assert!(parsed.icloud_sync.unwrap().root.is_none());
        assert!(parsed.whatsapp_import.unwrap().source.is_none());
    }
}
