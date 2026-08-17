use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationCheckpoint {
    pub operation: String,
    pub source_type: String,
    pub source_key: String,
    pub phase: String,
    pub status: String,
    pub total_files: u64,
    pub processed_files: u64,
    pub total_batches: u64,
    pub completed_batches: u64,
    pub chunks_written: u64,
    pub added_files: u64,
    pub updated_files: u64,
    pub removed_files: u64,
    pub current_file_path: Option<String>,
    pub current_file_size_bytes: Option<u64>,
    pub started_unix_ms: u128,
    pub updated_unix_ms: u128,
}

impl OperationCheckpoint {
    pub fn new(
        operation: impl Into<String>,
        source_type: impl Into<String>,
        source_key: impl Into<String>,
    ) -> Self {
        let now = unix_timestamp_now_ms();
        Self {
            operation: operation.into(),
            source_type: source_type.into(),
            source_key: source_key.into(),
            phase: "starting".to_string(),
            status: "running".to_string(),
            total_files: 0,
            processed_files: 0,
            total_batches: 0,
            completed_batches: 0,
            chunks_written: 0,
            added_files: 0,
            updated_files: 0,
            removed_files: 0,
            current_file_path: None,
            current_file_size_bytes: None,
            started_unix_ms: now,
            updated_unix_ms: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_unix_ms = unix_timestamp_now_ms();
    }
}

pub struct OperationTracker {
    path: PathBuf,
}

impl OperationTracker {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("runtime").join("active-operation.json"),
        }
    }

    pub fn load(&self) -> Result<Option<OperationCheckpoint>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let payload = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let checkpoint = serde_json::from_str(&payload)
            .with_context(|| format!("failed to decode {}", self.path.display()))?;
        Ok(Some(checkpoint))
    }

    pub fn save(&self, checkpoint: &OperationCheckpoint) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("operation checkpoint path must have parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;

        let tmp_path = self.path.with_extension("tmp");
        let payload = serde_json::to_vec_pretty(checkpoint)?;
        fs::write(&tmp_path, payload)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &self.path)
            .with_context(|| format!("failed to publish {}", self.path.display()))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)
                .with_context(|| format!("failed to remove {}", self.path.display()))?;
        }
        Ok(())
    }
}

fn unix_timestamp_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_roundtrips_and_clears_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let tracker = OperationTracker::new(dir.path());

        let mut checkpoint = OperationCheckpoint::new("sync", "obsidian", "/tmp/vault");
        checkpoint.phase = "ingesting".to_string();
        checkpoint.total_files = 42;
        checkpoint.current_file_path = Some("/tmp/vault/giant.md".to_string());
        checkpoint.current_file_size_bytes = Some(8 * 1024 * 1024);
        tracker.save(&checkpoint).unwrap();

        let loaded = tracker.load().unwrap().unwrap();
        assert_eq!(loaded.phase, "ingesting");
        assert_eq!(loaded.total_files, 42);
        assert_eq!(
            loaded.current_file_path.as_deref(),
            Some("/tmp/vault/giant.md")
        );
        assert_eq!(loaded.current_file_size_bytes, Some(8 * 1024 * 1024));

        tracker.clear().unwrap();
        assert!(tracker.load().unwrap().is_none());
    }
}
