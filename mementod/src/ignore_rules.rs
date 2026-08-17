use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};

const IGNORE_FILE_NAME: &str = ".mementoignore";

#[derive(Debug, Clone)]
pub struct IgnoreRules {
    root: PathBuf,
    matcher: GlobSet,
}

impl IgnoreRules {
    pub fn load(root: &Path) -> Result<Self> {
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

    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        if relative.as_os_str().is_empty() {
            return false;
        }

        let relative = normalize_path(relative);
        if self.matcher.is_match(&relative) {
            return true;
        }

        if is_dir {
            self.matcher.is_match(format!("{relative}/"))
        } else {
            false
        }
    }
}

fn normalize_path(path: &Path) -> String {
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

    let mut patterns = Vec::new();
    let has_separator = core.contains('/');

    let direct = if dir_only {
        format!("{core}/**")
    } else {
        core.to_string()
    };
    patterns.push(direct);

    if !anchored {
        if has_separator {
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
    fn ignore_rules_match_root_and_nested_patterns() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".mementoignore"),
            "node_modules/\n*.log\n/build/output.json\n",
        )
        .unwrap();

        let rules = IgnoreRules::load(dir.path()).unwrap();

        assert!(rules.is_ignored(&dir.path().join("node_modules"), true));
        assert!(rules.is_ignored(&dir.path().join("foo").join("node_modules"), true));
        assert!(rules.is_ignored(&dir.path().join("debug.log"), false));
        assert!(rules.is_ignored(&dir.path().join("build").join("output.json"), false));
        assert!(!rules.is_ignored(&dir.path().join("memory").join("note.md"), false));
    }
}
