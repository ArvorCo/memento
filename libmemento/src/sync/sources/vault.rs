use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use walkdir::{DirEntry, WalkDir};

use crate::sync::graph::extract_wikilinks;
use crate::sync::sources::{normalize_relative_path, wildcard_match};
use crate::sync::{SyncCandidate, SyncDocument, SyncSource, SyncSourceType};

pub const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "Pods",
    ".next",
    ".turbo",
    "__pycache__",
    ".pytest_cache",
    ".venv",
    "venv",
    "dist",
    "build",
    ".build",
    "DerivedData",
    ".playwright-mcp",
    ".opencode",
    ".amp",
    ".codex",
    ".factory",
    ".claude",
    ".gemini",
    "target",
];

pub const PROTECTED_PATTERNS: &[&str] = &[
    "_*_hub.md",
    "MOC - *.md",
    "MOC*.md",
    "*Hub*.md",
    "* Hub.md",
    "Skills Hub.md",
];

#[derive(Debug, Clone)]
pub struct VaultSource {
    id: String,
    description: String,
    source_root: PathBuf,
    dest_root: PathBuf,
}

impl VaultSource {
    pub fn new(source_root: impl Into<PathBuf>, dest_root: impl Into<PathBuf>) -> Self {
        Self {
            id: "vault".into(),
            description: "Obsidian vault markdown source".into(),
            source_root: source_root.into(),
            dest_root: dest_root.into(),
        }
    }

    pub fn is_protected_path(path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        PROTECTED_PATTERNS
            .iter()
            .any(|pattern| wildcard_match(pattern, &normalized))
    }

    fn include_entry(entry: &DirEntry) -> bool {
        if entry.depth() == 0 {
            return true;
        }

        if entry.file_type().is_dir() {
            let name = entry.file_name().to_string_lossy();
            !EXCLUDED_DIRS
                .iter()
                .any(|excluded| excluded == &name.as_ref())
        } else {
            true
        }
    }

    fn title_from_content(path: &Path, content: &str) -> String {
        content
            .lines()
            .find_map(|line| line.strip_prefix("# ").map(str::trim))
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled".into())
    }
}

impl SyncSource for VaultSource {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    async fn scan(&self) -> Result<Vec<SyncCandidate>> {
        let mut candidates = Vec::new();
        for entry in WalkDir::new(&self.source_root)
            .into_iter()
            .filter_entry(Self::include_entry)
        {
            let entry = entry.with_context(|| {
                format!("failed while scanning vault {}", self.source_root.display())
            })?;

            if !entry.file_type().is_file() {
                continue;
            }

            if entry.path().extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }

            let metadata = entry.metadata().with_context(|| {
                format!("failed to read metadata for {}", entry.path().display())
            })?;
            let entry_path = entry.path().to_path_buf();
            let relative = entry_path
                .strip_prefix(&self.source_root)
                .with_context(|| {
                    format!(
                        "failed to strip source prefix {} from {}",
                        self.source_root.display(),
                        entry_path.display()
                    )
                })?
                .to_path_buf();
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);

            candidates.push(SyncCandidate {
                source_path: entry_path,
                dest_path: self.dest_root.join(relative),
                mtime,
                size: metadata.len(),
            });
        }

        candidates.sort_by(|left, right| left.dest_path.cmp(&right.dest_path));
        Ok(candidates)
    }

    async fn read(&self, candidate: &SyncCandidate) -> Result<SyncDocument> {
        let content = fs::read_to_string(&candidate.source_path)
            .with_context(|| format!("failed to read {}", candidate.source_path.display()))?;
        let title = Self::title_from_content(&candidate.source_path, &content);
        let path = candidate
            .dest_path
            .strip_prefix(&self.dest_root)
            .ok()
            .map(normalize_relative_path)
            .unwrap_or_else(|| normalize_relative_path(&candidate.dest_path));
        let links = extract_wikilinks(&path, &content);

        Ok(SyncDocument {
            path,
            content,
            frontmatter: None,
            title,
            tags: Vec::new(),
            links,
            source_type: SyncSourceType::ObsidianVault,
            mtime: candidate.mtime,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scan_skips_excluded_directories_and_collects_markdown_files() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let dest_root = temp.path().join("dest");
        fs::create_dir_all(source_root.join("notes")).unwrap();
        fs::create_dir_all(source_root.join("node_modules")).unwrap();
        fs::write(source_root.join("notes").join("keep.md"), "# Keep").unwrap();
        fs::write(source_root.join("notes").join("skip.txt"), "skip").unwrap();
        fs::write(
            source_root.join("node_modules").join("ignore.md"),
            "# Ignore",
        )
        .unwrap();

        let source = VaultSource::new(&source_root, &dest_root);
        let candidates = source.scan().await.unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].dest_path,
            dest_root.join("notes").join("keep.md")
        );
    }

    #[tokio::test]
    async fn read_extracts_title_and_links_from_markdown() {
        let temp = tempfile::tempdir().unwrap();
        let source_root = temp.path().join("source");
        let dest_root = temp.path().join("dest");
        fs::create_dir_all(source_root.join("notes")).unwrap();
        let source_path = source_root.join("notes").join("keep.md");
        fs::write(&source_path, "# Hello\nSee [[World|Planet]].").unwrap();

        let source = VaultSource::new(&source_root, &dest_root);
        let candidate = SyncCandidate {
            source_path,
            dest_path: dest_root.join("notes").join("keep.md"),
            mtime: 33,
            size: 29,
        };
        let document = source.read(&candidate).await.unwrap();

        assert_eq!(document.title, "Hello");
        assert_eq!(document.links.len(), 1);
        assert_eq!(document.links[0].display.as_deref(), Some("Planet"));
    }

    #[test]
    fn protected_patterns_match_expected_paths() {
        assert!(VaultSource::is_protected_path(Path::new("_acme_hub.md")));
        assert!(VaultSource::is_protected_path(Path::new("Skills Hub.md")));
        assert!(!VaultSource::is_protected_path(Path::new(
            "projects/acme/notes.md"
        )));
    }
}
