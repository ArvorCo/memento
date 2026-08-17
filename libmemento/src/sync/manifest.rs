use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::sync::{ManifestEntry, SyncManifest};

impl SyncManifest {
    pub fn load(source_id: &str) -> Result<Self> {
        let path = Self::manifest_path(source_id);
        if !path.exists() {
            return Ok(Self {
                entries: Default::default(),
                last_sync: None,
                source_id: source_id.to_owned(),
            });
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        let mut manifest: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse manifest {}", path.display()))?;
        if manifest.source_id.is_empty() {
            manifest.source_id = source_id.to_owned();
        }
        Ok(manifest)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::manifest_path(&self.source_id);
        let parent = path
            .parent()
            .context("manifest path must have a parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn manifest_path(source_id: &str) -> PathBuf {
        manifest_root().join(format!(
            "sync_manifest_{}.json",
            sanitize_source_id(source_id)
        ))
    }

    pub fn needs_update(&self, path: impl AsRef<Path>, mtime: i64, size: u64) -> bool {
        let key = manifest_key(path.as_ref());
        match self.entries.get(&key) {
            Some(entry) => entry.mtime != mtime || entry.size != size,
            None => true,
        }
    }

    pub fn mark_synced(&mut self, path: impl AsRef<Path>, mtime: i64, size: u64) -> Result<()> {
        let key = manifest_key(path.as_ref());
        let synced_at = unix_timestamp_now();
        let hash = quick_file_fingerprint(path.as_ref())?;
        self.entries.insert(
            key,
            ManifestEntry {
                mtime,
                size,
                hash,
                synced_at,
            },
        );
        self.last_sync = Some(synced_at);
        Ok(())
    }

    pub fn remove(&mut self, path: impl AsRef<Path>) -> bool {
        let removed = self.entries.remove(&manifest_key(path.as_ref())).is_some();
        if removed {
            self.last_sync = Some(unix_timestamp_now());
        }
        removed
    }
}

fn manifest_root() -> PathBuf {
    if let Some(root) = env::var_os("MEMENTO_HOME") {
        return PathBuf::from(root);
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".memento");
    }

    PathBuf::from(".memento")
}

fn sanitize_source_id(source_id: &str) -> String {
    source_id
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

fn manifest_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unix_timestamp_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(_) => 0,
    }
}

fn quick_file_fingerprint(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open file for fingerprint {}", path.display()))?;
    let mut buffer = [0_u8; 4096];
    let bytes_read = file
        .read(&mut buffer)
        .with_context(|| format!("failed to read file for fingerprint {}", path.display()))?;
    let mut hasher = DefaultHasher::new();
    buffer[..bytes_read].hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn manifest_persists_to_expected_json_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        env::set_var("MEMENTO_HOME", temp.path());

        let file_path = temp.path().join("note.md");
        fs::write(&file_path, "# Hello").unwrap();

        let mut manifest = SyncManifest::load("vault").unwrap();
        manifest.mark_synced(&file_path, 10, 7).unwrap();
        manifest.save().unwrap();

        let reloaded = SyncManifest::load("vault").unwrap();
        let manifest_path = SyncManifest::manifest_path("vault");

        assert!(manifest_path.exists());
        assert!(!reloaded.needs_update(&file_path, 10, 7));
        env::remove_var("MEMENTO_HOME");
    }

    #[test]
    fn manifest_detects_changes_and_removals() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        env::set_var("MEMENTO_HOME", temp.path());

        let file_path = temp.path().join("nested").join("doc.md");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, "content").unwrap();

        let mut manifest = SyncManifest::load("vault").unwrap();
        assert!(manifest.needs_update(&file_path, 11, 7));

        manifest.mark_synced(&file_path, 11, 7).unwrap();
        assert!(!manifest.needs_update(&file_path, 11, 7));
        assert!(manifest.needs_update(&file_path, 12, 7));
        assert!(manifest.remove(&file_path));
        assert!(manifest.needs_update(&file_path, 11, 7));

        env::remove_var("MEMENTO_HOME");
    }
}
