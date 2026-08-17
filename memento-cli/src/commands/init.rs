use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use crate::client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Auto,
    Mac,
    Linux,
    Windows,
}

impl Preset {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "auto" => Ok(Self::Auto),
            "mac" => Ok(Self::Mac),
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            other => Err(anyhow::anyhow!(
                "unsupported preset `{other}`; expected auto|mac|linux|windows"
            )),
        }
    }

    fn detect() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    fn resolve(self) -> Self {
        match self {
            Self::Auto => Self::detect(),
            other => other,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mac => "mac",
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

#[derive(Debug, Clone)]
struct SourceToggle {
    path: PathBuf,
    enabled_by_default: bool,
}

#[derive(Debug, Clone)]
struct VaultSyncRunnerPlan {
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

impl VaultSyncRunnerPlan {
    fn is_available(&self) -> bool {
        !self.command.is_empty()
    }
}

#[derive(Debug, Clone)]
struct InitPlan {
    preset: Preset,
    data_dir: PathBuf,
    config_dir: PathBuf,
    vault_root: PathBuf,
    sync_state_dir: PathBuf,
    daemon_config_path: PathBuf,
    vault_sync_config_path: PathBuf,
    schedule_interval: String,
    documents_root: Option<PathBuf>,
    desktop_root: Option<PathBuf>,
    workspace_root: PathBuf,
    vault_sync_runner: VaultSyncRunnerPlan,
    codex: SourceToggle,
    droid: SourceToggle,
    claude: SourceToggle,
    chatgpt: SourceToggle,
    whatsapp: SourceToggle,
    icloud_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteOutcome {
    Created,
    Updated,
    Reused,
}

pub async fn run(
    preset: &str,
    vault_root: Option<&str>,
    schedule: &str,
    force: bool,
) -> Result<()> {
    let requested_preset = Preset::parse(preset)?;
    let plan = build_plan(requested_preset, vault_root, schedule)?;

    fs::create_dir_all(&plan.config_dir)?;
    fs::create_dir_all(&plan.vault_root)?;
    fs::create_dir_all(&plan.sync_state_dir)?;

    let daemon_outcome = write_if_needed(
        &plan.daemon_config_path,
        &render_daemon_config(&plan),
        force,
    )?;
    let feeder_outcome = write_if_needed(
        &plan.vault_sync_config_path,
        &render_vault_sync_config(&plan),
        force,
    )?;

    println!("{}", "Memento Onboarding".bold().cyan());
    println!("{}", "-".repeat(52));
    println!(
        "  {:<18} {}",
        "Preset:".dimmed(),
        plan.preset.as_str().bold()
    );
    println!(
        "  {:<18} {}",
        "Vault Root:".dimmed(),
        plan.vault_root.display().to_string().bold()
    );
    println!(
        "  {:<18} {}",
        "Schedule:".dimmed(),
        format!("every {}", plan.schedule_interval).bold()
    );
    println!(
        "  {:<18} {} ({})",
        "Daemon Config:".dimmed(),
        plan.daemon_config_path.display().to_string().bold(),
        describe_outcome(daemon_outcome)
    );
    println!(
        "  {:<18} {} ({})",
        "Vault Config:".dimmed(),
        plan.vault_sync_config_path.display().to_string().bold(),
        describe_outcome(feeder_outcome)
    );
    println!("{}", "-".repeat(52));
    println!("{}", "Detected Sources".bold().yellow());
    print_source("Codex", &plan.codex);
    print_source("Claude", &plan.claude);
    print_source("Droid", &plan.droid);
    print_source("ChatGPT", &plan.chatgpt);
    print_source("WhatsApp", &plan.whatsapp);
    if let Some(path) = &plan.icloud_root {
        println!(
            "  {:<12} {}  {}",
            "iCloud:".dimmed(),
            if path.exists() {
                "available".green().bold()
            } else {
                "disabled".yellow().bold()
            },
            path.display()
        );
    }

    match client::ensure_daemon_running(std::time::Duration::from_secs(30)).await {
        Ok(()) => println!(
            "\n{} {}",
            "OK".green().bold(),
            "Daemon reachable. Run `memento doctor` and `memento status` next.".bold()
        ),
        Err(err) => println!(
            "\n{} daemon was not started automatically: {}",
            "WARN".yellow().bold(),
            err
        ),
    }

    println!(
        "  Feeder Runner: {}",
        format_runner_command(
            &plan.vault_sync_runner,
            &plan.vault_sync_config_path,
            "run-all"
        )
        .bold()
    );
    println!("  Next learn run: {}", "memento learn".bold());

    Ok(())
}

fn build_plan(
    requested_preset: Preset,
    vault_root: Option<&str>,
    schedule: &str,
) -> Result<InitPlan> {
    let home = dirs::home_dir().context("Could not determine home directory for onboarding")?;
    let data_dir = std::env::var_os("MEMENTO_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".memento"));
    let config_dir = data_dir.join("config");
    let preset = requested_preset.resolve();
    let vault_root = vault_root
        .map(PathBuf::from)
        .unwrap_or_else(|| default_vault_root(&home, preset));
    let sync_state_dir = data_dir.join("sync");
    let daemon_config_path = config_dir.join("daemon.toml");
    let vault_sync_config_path = config_dir.join("vault_sync.toml");
    let documents_root = preferred_existing(
        &home,
        match preset {
            Preset::Mac => &["Documents"][..],
            Preset::Linux => &["Documents"][..],
            Preset::Windows => &["Documents"][..],
            Preset::Auto => &["Documents"][..],
        },
    );
    let desktop_root = preferred_existing(
        &home,
        match preset {
            Preset::Mac | Preset::Windows => &["Desktop"][..],
            Preset::Linux | Preset::Auto => &[][..],
        },
    );
    let workspace_root = default_workspace_root(&home, preset);
    let downloads_root = home.join("Downloads");

    Ok(InitPlan {
        preset,
        data_dir,
        config_dir,
        vault_root,
        sync_state_dir,
        daemon_config_path,
        vault_sync_config_path,
        schedule_interval: schedule.to_string(),
        documents_root,
        desktop_root,
        workspace_root,
        vault_sync_runner: resolve_vault_sync_runner(),
        codex: detect_source(home.join(".codex").join("sessions")),
        droid: detect_source(home.join(".factory").join("sessions")),
        claude: detect_source(home.join(".claude").join("projects")),
        chatgpt: detect_source(downloads_root.join("chatgpt-export")),
        whatsapp: detect_whatsapp_source(downloads_root),
        icloud_root: if preset == Preset::Mac {
            Some(home.join("Library/Mobile Documents/com~apple~CloudDocs"))
        } else {
            None
        },
    })
}

fn default_vault_root(home: &Path, preset: Preset) -> PathBuf {
    match preset {
        Preset::Windows => home.join("MementoVault"),
        Preset::Mac | Preset::Linux | Preset::Auto => home.join("MementoVault"),
    }
}

fn default_workspace_root(home: &Path, preset: Preset) -> PathBuf {
    let candidates: &[&str] = match preset {
        Preset::Mac => &["Developer", "Projects", "Workspace"],
        Preset::Linux => &["Projects", "Developer", "workspace"],
        Preset::Windows => &["source/repos", "Projects", "Developer"],
        Preset::Auto => &["Projects"],
    };
    preferred_existing(home, candidates).unwrap_or_else(|| home.join(candidates[0]))
}

fn preferred_existing(home: &Path, candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|candidate| home.join(candidate))
        .find(|path| path.exists())
}

fn detect_source(path: PathBuf) -> SourceToggle {
    let enabled_by_default = path.exists();
    SourceToggle {
        path,
        enabled_by_default,
    }
}

fn detect_whatsapp_source(path: PathBuf) -> SourceToggle {
    let enabled_by_default = path.exists()
        && fs::read_dir(&path)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter_map(|entry| entry.file_name().into_string().ok())
            .any(|name| {
                let lower = name.to_lowercase();
                lower.contains("whatsapp") && (lower.ends_with(".zip") || lower.ends_with(".txt"))
            });

    SourceToggle {
        path,
        enabled_by_default,
    }
}

fn render_daemon_config(plan: &InitPlan) -> String {
    format!(
        concat!(
            "# Generated by `memento init`\n",
            "# Edit this file to change runtime or schedule defaults.\n\n",
            "[daemon]\n",
            "data_dir = \"{}\"\n",
            "socket_path = \"{}\"\n",
            "http_enabled = false\n",
            "http_host = \"127.0.0.1\"\n",
            "http_port = 8765\n",
            "allow_remote_http = false\n\n",
            "[vault]\n",
            "root = \"{}\"\n",
            "vault_sync_config = \"{}\"\n\n",
            "[vault_sync_runner]\n",
            "command = [{}]\n",
            "{}\n",
            "[scheduler]\n",
            "enabled = {}\n",
            "default_interval = \"{}\"\n",
            "run_on_start = {}\n",
            "batch_updates = true\n\n",
            "[[scheduler.jobs]]\n",
            "name = \"vault-sync\"\n",
            "enabled = {}\n",
            "type = \"vault_sync\"\n",
            "config = \"{}\"\n",
            "command = \"run-all\"\n",
            "interval = \"{}\"\n",
        ),
        toml_path(&plan.data_dir),
        toml_path(&plan.data_dir.join("memento.sock")),
        toml_path(&plan.vault_root),
        toml_path(&plan.vault_sync_config_path),
        plan.vault_sync_runner
            .command
            .iter()
            .map(|part| format!("\"{}\"", toml_string(part)))
            .collect::<Vec<_>>()
            .join(", "),
        plan.vault_sync_runner
            .working_dir
            .as_ref()
            .map(|path| format!("working_dir = \"{}\"", toml_path(path)))
            .unwrap_or_else(|| "# working_dir intentionally unset".to_string()),
        plan.vault_sync_runner.is_available(),
        plan.schedule_interval,
        plan.vault_sync_runner.is_available(),
        plan.vault_sync_runner.is_available(),
        toml_path(&plan.vault_sync_config_path),
        plan.schedule_interval,
    )
}

fn render_vault_sync_config(plan: &InitPlan) -> String {
    let mut out = String::new();
    writeln!(out, "# Generated by `memento init`").unwrap();
    writeln!(
        out,
        "# This file controls imports, syncs, and connector paths."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "[vault]").unwrap();
    writeln!(out, "root = \"{}\"", toml_path(&plan.vault_root)).unwrap();
    writeln!(out, "state_dir = \"{}\"", toml_path(&plan.sync_state_dir)).unwrap();
    writeln!(out).unwrap();

    if let Some(path) = &plan.documents_root {
        append_markdown_root(&mut out, "documents", path, "documents", &[".md", ".txt"]);
    }
    if let Some(path) = &plan.desktop_root {
        append_markdown_root(&mut out, "desktop", path, "desktop", &[".md", ".txt"]);
    }
    append_markdown_root(
        &mut out,
        "workspace",
        &plan.workspace_root,
        "projects",
        &[".md"],
    );

    writeln!(out, "[linking]").unwrap();
    writeln!(out, "enabled = true").unwrap();
    writeln!(out, "default_project_prefix = \"projects\"").unwrap();
    writeln!(out, "hub_filename = \"_memento_hub.md\"").unwrap();
    writeln!(out, "root_hub = \"_memento.md\"").unwrap();
    writeln!(out, "tag_hubs = true").unwrap();
    writeln!(out, "min_tag_documents = 2").unwrap();
    writeln!(out, "inject_navigation = true").unwrap();
    writeln!(out, "exclude_dirs = [\".git\", \".obsidian\", \".trash\"]").unwrap();
    writeln!(out).unwrap();

    if let Some(path) = &plan.documents_root {
        writeln!(out, "[[document_import.sources]]").unwrap();
        writeln!(out, "name = \"personal-documents\"").unwrap();
        writeln!(out, "enabled = false").unwrap();
        writeln!(out, "source = \"{}\"", toml_path(path)).unwrap();
        writeln!(out, "destination = \"converted/documents\"").unwrap();
        writeln!(out, "manifest = \"documents-personal.json\"").unwrap();
        writeln!(
            out,
            "include_extensions = [\".pdf\", \".doc\", \".docx\", \".odt\", \".rtf\", \".pptx\", \".xlsx\", \".html\", \".csv\", \".json\", \".ipynb\"]"
        )
        .unwrap();
        writeln!(
            out,
            "exclude_dirs = [\".git\", \".obsidian\", \"node_modules\", \".venv\", \"venv\"]"
        )
        .unwrap();
        writeln!(out, "preserve_raw = false").unwrap();
        writeln!(out, "delete_removed = true").unwrap();
        writeln!(out, "tags = [\"documents\", \"imported\"]").unwrap();
        writeln!(out, "max_file_bytes = 104857600").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "[[database_import.sources]]").unwrap();
    writeln!(out, "name = \"example-sqlite\"").unwrap();
    writeln!(out, "enabled = false").unwrap();
    writeln!(out, "driver = \"sqlite\"").unwrap();
    writeln!(
        out,
        "database = \"{}\"",
        toml_path(&plan.data_dir.join("example.db"))
    )
    .unwrap();
    writeln!(
        out,
        "query = \"SELECT id, title, body, updated_at FROM notes\""
    )
    .unwrap();
    writeln!(out, "destination = \"databases/example\"").unwrap();
    writeln!(out, "manifest = \"database-example.json\"").unwrap();
    writeln!(out, "id_column = \"id\"").unwrap();
    writeln!(out, "title_column = \"title\"").unwrap();
    writeln!(out, "content_columns = [\"body\"]").unwrap();
    writeln!(out, "metadata_columns = [\"updated_at\"]").unwrap();
    writeln!(out, "updated_at_column = \"updated_at\"").unwrap();
    writeln!(out, "tags = [\"database\", \"notes\"]").unwrap();
    writeln!(out, "delete_removed = true").unwrap();
    writeln!(out).unwrap();

    append_session_import(&mut out, "codex", "Codex", &plan.codex, "converted/codex");
    append_session_import(&mut out, "droid", "Droid", &plan.droid, "converted/droid");
    append_session_import_with_extra(
        &mut out,
        "claude",
        "Claude",
        &plan.claude,
        "converted/claude",
        Some("exclude_path_fragments = [\"subagents\"]"),
    );
    append_session_import(
        &mut out,
        "chatgpt",
        "ChatGPT",
        &plan.chatgpt,
        "converted/chatgpt",
    );

    if let Some(icloud_root) = &plan.icloud_root {
        writeln!(out, "[icloud_sync]").unwrap();
        writeln!(out, "enabled = {}", icloud_root.exists()).unwrap();
        writeln!(out, "root = \"{}\"", toml_path(icloud_root)).unwrap();
        writeln!(out).unwrap();
        writeln!(out, "[[icloud_sync.folders]]").unwrap();
        writeln!(out, "name = \"documents\"").unwrap();
        writeln!(out, "source = \"Documents\"").unwrap();
        writeln!(out, "raw_destination = \"raw/icloud/documents\"").unwrap();
        writeln!(
            out,
            "converted_destination = \"converted/icloud/documents\""
        )
        .unwrap();
        writeln!(out, "include_markdown = true").unwrap();
        writeln!(out, "include_text = true").unwrap();
        writeln!(out, "convert_doc = true").unwrap();
        writeln!(out, "convert_docx = true").unwrap();
        writeln!(out, "convert_pptx = false").unwrap();
        writeln!(out, "convert_pdf = true").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "[apple_notes]").unwrap();
        writeln!(out, "enabled = false").unwrap();
        writeln!(out, "destination = \"converted/apple-notes\"").unwrap();
        writeln!(out, "include_index = true").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "[whatsapp_import]").unwrap();
    writeln!(out, "enabled = {}", plan.whatsapp.enabled_by_default).unwrap();
    writeln!(out, "source = \"{}\"", toml_path(&plan.whatsapp.path)).unwrap();
    writeln!(out, "destination = \"whatsapp\"").unwrap();
    writeln!(out, "manifest = \"whatsapp_manifest.json\"").unwrap();
    writeln!(out, "default_category = \"outros\"").unwrap();

    out
}

fn append_markdown_root(
    out: &mut String,
    name: &str,
    source: &Path,
    destination: &str,
    extensions: &[&str],
) {
    writeln!(out, "[[markdown_sync.roots]]").unwrap();
    writeln!(out, "name = \"{name}\"").unwrap();
    writeln!(out, "source = \"{}\"", toml_path(source)).unwrap();
    writeln!(out, "destination = \"{destination}\"").unwrap();
    writeln!(
        out,
        "include_extensions = [{}]",
        extensions
            .iter()
            .map(|ext| format!("\"{ext}\""))
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "exclude_dirs = [\".git\", \"node_modules\", \"Pods\", \".next\", \".turbo\", \"__pycache__\", \".pytest_cache\", \".venv\", \"venv\", \"dist\", \"build\", \".build\", \"DerivedData\", \".playwright-mcp\", \".opencode\", \".amp\", \".codex\", \".factory\", \".claude\", \".gemini\"]"
    )
    .unwrap();
    writeln!(
        out,
        "protected_globs = [\"**/_*_hub.md\", \"**/MOC - *.md\", \"**/*Hub*.md\", \"**/* Hub.md\"]"
    )
    .unwrap();
    writeln!(out).unwrap();
}

fn append_session_import(
    out: &mut String,
    key: &str,
    label: &str,
    source: &SourceToggle,
    destination: &str,
) {
    append_session_import_with_extra(out, key, label, source, destination, None);
}

fn append_session_import_with_extra(
    out: &mut String,
    key: &str,
    label: &str,
    source: &SourceToggle,
    destination: &str,
    extra: Option<&str>,
) {
    writeln!(out, "[session_import.{key}]").unwrap();
    writeln!(out, "enabled = {}", source.enabled_by_default).unwrap();
    writeln!(out, "source = \"{}\"", toml_path(&source.path)).unwrap();
    writeln!(out, "destination = \"{destination}\"").unwrap();
    writeln!(out, "manifest = \"{}_manifest.json\"", key).unwrap();
    writeln!(out, "label = \"{label}\"").unwrap();
    writeln!(out, "source_tag = \"{key}\"").unwrap();
    writeln!(
        out,
        "file_glob = \"{}\"",
        if key == "chatgpt" {
            "conversations*.json"
        } else {
            "*.jsonl"
        }
    )
    .unwrap();
    if let Some(extra) = extra {
        writeln!(out, "{extra}").unwrap();
    }
    writeln!(out).unwrap();
}

fn write_if_needed(path: &Path, contents: &str, force: bool) -> Result<WriteOutcome> {
    if path.exists() {
        let existing = fs::read_to_string(path)
            .with_context(|| format!("Failed to read existing config {}", path.display()))?;
        if existing == contents {
            return Ok(WriteOutcome::Reused);
        }
        if !force {
            return Ok(WriteOutcome::Reused);
        }
        fs::write(path, contents)
            .with_context(|| format!("Failed to overwrite config {}", path.display()))?;
        return Ok(WriteOutcome::Updated);
    }

    fs::write(path, contents)
        .with_context(|| format!("Failed to write config {}", path.display()))?;
    Ok(WriteOutcome::Created)
}

fn resolve_vault_sync_runner() -> VaultSyncRunnerPlan {
    if let Some(repo_root) = resolve_repo_root() {
        return VaultSyncRunnerPlan {
            command: vec![
                "uv".to_string(),
                "run".to_string(),
                "python".to_string(),
                "-m".to_string(),
                "tools.vault_sync.cli".to_string(),
            ],
            working_dir: Some(repo_root),
        };
    }

    if command_exists("memento-vault-sync") {
        return VaultSyncRunnerPlan {
            command: vec!["memento-vault-sync".to_string()],
            working_dir: None,
        };
    }

    VaultSyncRunnerPlan {
        command: Vec::new(),
        working_dir: None,
    }
}

fn format_runner_command(
    runner: &VaultSyncRunnerPlan,
    config_path: &Path,
    command: &str,
) -> String {
    if runner.command.is_empty() {
        return "not installed; direct `memento sync` remains available".to_string();
    }
    let mut parts = runner.command.clone();
    parts.push("--config".to_string());
    parts.push(config_path.display().to_string());
    parts.push(command.to_string());
    parts.join(" ")
}

fn resolve_repo_root() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    for ancestor in current_dir.ancestors() {
        if ancestor.join("tools/vault_sync/cli.py").exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(command).exists();
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path_var).any(|dir| dir.join(command).exists())
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn describe_outcome(outcome: WriteOutcome) -> colored::ColoredString {
    match outcome {
        WriteOutcome::Created => "created".green().bold(),
        WriteOutcome::Updated => "updated".yellow().bold(),
        WriteOutcome::Reused => "reused".cyan().bold(),
    }
}

fn print_source(label: &str, source: &SourceToggle) {
    println!(
        "  {:<12} {}  {}",
        format!("{label}:").dimmed(),
        if source.enabled_by_default {
            "enabled".green().bold()
        } else {
            "disabled".yellow().bold()
        },
        source.path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_config_includes_schedule_and_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = InitPlan {
            preset: Preset::Linux,
            data_dir: tmp.path().join(".memento"),
            config_dir: tmp.path().join(".memento/config"),
            vault_root: tmp.path().join("vault"),
            sync_state_dir: tmp.path().join(".memento/sync"),
            daemon_config_path: tmp.path().join(".memento/config/daemon.toml"),
            vault_sync_config_path: tmp.path().join(".memento/config/vault_sync.toml"),
            schedule_interval: "8h".to_string(),
            documents_root: Some(tmp.path().join("Documents")),
            desktop_root: None,
            workspace_root: tmp.path().join("Projects"),
            vault_sync_runner: VaultSyncRunnerPlan {
                command: vec!["uv".to_string()],
                working_dir: Some(tmp.path().to_path_buf()),
            },
            codex: detect_source(tmp.path().join(".codex/sessions")),
            droid: detect_source(tmp.path().join(".factory/sessions")),
            claude: detect_source(tmp.path().join(".claude/projects")),
            chatgpt: detect_source(tmp.path().join("Downloads/chatgpt-export")),
            whatsapp: detect_source(tmp.path().join("Downloads")),
            icloud_root: None,
        };

        let rendered = render_daemon_config(&plan);
        assert!(rendered.contains("[scheduler]"));
        assert!(rendered.contains("[vault_sync_runner]"));
        assert!(rendered.contains("command = [\"uv\"]"));
        assert!(rendered.contains("default_interval = \"8h\""));
        assert!(rendered.contains("command = \"run-all\""));
    }

    #[test]
    fn daemon_config_disables_scheduler_without_optional_feeder() {
        let tmp = tempfile::tempdir().unwrap();
        let mut plan = build_plan(
            Preset::Linux,
            Some(tmp.path().join("vault").to_str().unwrap()),
            "8h",
        )
        .unwrap();
        plan.vault_sync_runner = VaultSyncRunnerPlan {
            command: Vec::new(),
            working_dir: None,
        };

        let rendered = render_daemon_config(&plan);

        assert!(rendered.contains("command = []"));
        assert!(rendered.contains("enabled = false"));
        assert!(rendered.contains("run_on_start = false"));
        assert!(format_runner_command(
            &plan.vault_sync_runner,
            &plan.vault_sync_config_path,
            "run-all"
        )
        .contains("not installed"));
    }

    #[test]
    fn vault_sync_config_renders_connectors() {
        let tmp = tempfile::tempdir().unwrap();
        let mut plan = build_plan(
            Preset::Linux,
            Some(tmp.path().join("vault").to_str().unwrap()),
            "8h",
        )
        .unwrap();
        plan.codex.enabled_by_default = true;
        let rendered = render_vault_sync_config(&plan);
        assert!(rendered.contains("[session_import.codex]"));
        assert!(rendered.contains("[whatsapp_import]"));
        assert!(rendered.contains("[[markdown_sync.roots]]"));
        assert!(rendered.contains("[[document_import.sources]]"));
        assert!(rendered.contains("[[database_import.sources]]"));
        assert!(rendered.contains("hub_filename = \"_memento_hub.md\""));
    }
}
