use anyhow::{Context, Result};
use libmemento::format::{ChunkId, DocId, SourceRecord, StoredChunk, StoredDocument};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySnapshot {
    pub domain: String,
    pub documents: Vec<StoredDocument>,
    pub next_doc_id: DocId,
    pub next_chunk_id: ChunkId,
    pub chunks: Vec<StoredChunk>,
    pub sources: Vec<SourceRecord>,
}

pub struct RecoverySnapshotStore {
    path: PathBuf,
}

impl RecoverySnapshotStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("runtime").join("ingest-recovery.bin"),
        }
    }

    pub fn load(&self) -> Result<Option<RecoverySnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let file = fs::File::open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        let mut decoder = zstd::Decoder::new(file).map_err(std::io::Error::other)?;
        let snapshot = bincode::deserialize_from(&mut decoder).map_err(std::io::Error::other)?;
        Ok(Some(snapshot))
    }

    pub fn save(&self, snapshot: &RecoverySnapshot) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("recovery snapshot path must have parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;

        let tmp_path = self.path.with_extension("tmp");
        let file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        let mut encoder = zstd::Encoder::new(file, 1).map_err(std::io::Error::other)?;
        bincode::serialize_into(&mut encoder, snapshot).map_err(std::io::Error::other)?;
        encoder.finish().map_err(std::io::Error::other)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_snapshot_store_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = RecoverySnapshotStore::new(dir.path());
        let snapshot = RecoverySnapshot {
            domain: "test".to_string(),
            documents: Vec::new(),
            next_doc_id: 3,
            next_chunk_id: 7,
            chunks: Vec::new(),
            sources: Vec::new(),
        };

        store.save(&snapshot).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.domain, "test");
        assert_eq!(loaded.next_doc_id, 3);
        assert_eq!(loaded.next_chunk_id, 7);

        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
