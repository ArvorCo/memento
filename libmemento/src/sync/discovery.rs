//! Shared local corpus discovery for ingest, sync, and retrieval evaluation.

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const IGNORE_FILE_NAME: &str = ".mementoignore";

/// A non-empty document selected by the same rules used for local ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDocument {
    pub path: PathBuf,
    pub size: u64,
    pub modified_unix_ms: u128,
}

/// Discover supported documents below `root` in deterministic path order.
pub fn discover_documents(root: &Path) -> Result<Vec<DiscoveredDocument>> {
    let ignore_rules = IgnoreRules::load(root)?;
    let mut documents = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir() && should_skip_indexing_dir(&name) {
                return false;
            }
            !ignore_rules.is_ignored(entry.path(), entry.file_type().is_dir())
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
    {
        if ignore_rules.is_ignored(entry.path(), false) || !is_supported_document(entry.path()) {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.len() == 0 {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        documents.push(DiscoveredDocument {
            path: entry.into_path(),
            size: metadata.len(),
            modified_unix_ms: modified
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        });
    }

    documents.sort_by(|left, right| normalized_path(&left.path).cmp(&normalized_path(&right.path)));
    Ok(documents)
}

/// Whether Memento's generic file/folder/Obsidian importer can parse this path.
pub fn is_supported_document(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(name, ".DS_Store" | ".vault_index.db") {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "md" | "markdown"
            | "pdf"
            | "txt"
            | "canvas"
            | "json"
            | "jsonl"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "rs"
            | "py"
            | "sh"
            | "zsh"
            | "bash"
            | "yaml"
            | "yml"
            | "toml"
            | "csv"
            | "sql"
            | "html"
            | "css"
            | "xml"
            | "env"
            | "local"
            | "mdx"
            | "conf"
    ) || name.starts_with(".env")
}

/// Built-in exclusions for dependency, VCS, editor, and generated-output trees.
pub fn should_skip_indexing_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".obsidian"
            | ".entire"
            | "node_modules"
            | "target"
            | ".next"
            | "dist"
            | "build"
            | "coverage"
            | ".venv"
            | "venv"
            | "vendor"
    )
}

#[derive(Debug, Clone)]
struct IgnoreRules {
    root: PathBuf,
    matcher: GlobSet,
}

impl IgnoreRules {
    fn load(root: &Path) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        let path = root.join(IGNORE_FILE_NAME);
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            for line in content.lines() {
                for pattern in expand_pattern(line) {
                    builder.add(
                        Glob::new(&pattern)
                            .with_context(|| format!("invalid ignore pattern `{pattern}`"))?,
                    );
                }
            }
        }
        Ok(Self {
            root: root.to_path_buf(),
            matcher: builder.build().context("failed to build ignore matcher")?,
        })
    }

    fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        if relative.as_os_str().is_empty() {
            return false;
        }
        let relative = normalized_path(relative);
        self.matcher.is_match(&relative)
            || (is_dir && self.matcher.is_match(format!("{relative}/")))
    }
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn expand_pattern(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Vec::new();
    }
    let anchored = trimmed.starts_with('/');
    let dir_only = trimmed.ends_with('/');
    let core = trimmed.trim_start_matches('/').trim_end_matches('/');
    if core.is_empty() {
        return Vec::new();
    }

    let direct = if dir_only {
        format!("{core}/**")
    } else {
        core.to_string()
    };
    let mut patterns = vec![direct];
    if !anchored {
        if core.contains('/') {
            patterns.push(if dir_only {
                format!("**/{core}/**")
            } else {
                format!("**/{core}")
            });
        } else {
            patterns.push(format!("**/{core}"));
            if dir_only {
                patterns.push(format!("**/{core}/**"));
            }
        }
    }
    patterns.sort();
    patterns.dedup();
    patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_applies_ignore_and_builtin_rules() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("nested")).unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("keep.md"), "# Keep").unwrap();
        fs::write(dir.path().join("nested").join("skip.log"), "skip").unwrap();
        fs::write(dir.path().join("node_modules").join("junk.md"), "junk").unwrap();
        fs::write(dir.path().join(".mementoignore"), "*.log\n").unwrap();

        let documents = discover_documents(dir.path()).unwrap();

        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].path, dir.path().join("keep.md"));
    }

    #[test]
    fn supported_extensions_are_case_insensitive_and_cross_platform() {
        assert!(is_supported_document(Path::new(r"C:\Vault\Note.MD")));
        assert!(is_supported_document(Path::new("/vault/.env.local")));
        assert!(!is_supported_document(Path::new("/vault/image.png")));
    }
}
