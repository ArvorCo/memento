use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod graph;
pub mod manifest;
pub mod sources;

/// Aggregate counters and non-fatal errors for a sync run.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncResult {
    pub files_added: u64,
    pub files_updated: u64,
    pub files_removed: u64,
    pub files_unchanged: u64,
    pub bytes_processed: u64,
    pub links_extracted: u64,
    pub backlinks_injected: u64,
    pub errors: Vec<SyncError>,
    pub duration_ms: u64,
}

/// A sync error captured with file context instead of panicking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncError {
    pub path: String,
    pub error: String,
}

impl SyncError {
    pub fn new(path: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            error: error.into(),
        }
    }
}

/// Incremental manifest entry keyed by relative path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub mtime: i64,
    pub size: u64,
    pub hash: String,
    pub synced_at: i64,
}

/// Persisted sync manifest for one source.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncManifest {
    pub entries: HashMap<String, ManifestEntry>,
    pub last_sync: Option<i64>,
    pub source_id: String,
}

/// Parsed wikilink with source context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiLink {
    pub source: String,
    pub target: String,
    pub display: Option<String>,
    pub anchor: Option<String>,
}

/// Candidate file discovered during source scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCandidate {
    pub source_path: PathBuf,
    pub dest_path: PathBuf,
    pub mtime: i64,
    pub size: u64,
}

/// Processed document ready for ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncDocument {
    pub path: String,
    pub content: String,
    pub frontmatter: Option<serde_json::Value>,
    pub title: String,
    pub tags: Vec<String>,
    pub links: Vec<WikiLink>,
    pub source_type: SyncSourceType,
    pub mtime: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncSourceType {
    ObsidianVault,
    ProjectTree,
    ICloudDocument,
    WhatsAppExport,
    DocxConversion,
    CrmContact,
    ClaudeSession,
    CodexSession,
}

#[allow(async_fn_in_trait)]
pub trait SyncSource: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;
    async fn scan(&self) -> Result<Vec<SyncCandidate>>;
    async fn read(&self, candidate: &SyncCandidate) -> Result<SyncDocument>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummySource;

    impl SyncSource for DummySource {
        fn id(&self) -> &str {
            "dummy"
        }

        fn description(&self) -> &str {
            "dummy source"
        }

        async fn scan(&self) -> Result<Vec<SyncCandidate>> {
            Ok(vec![SyncCandidate {
                source_path: PathBuf::from("source.md"),
                dest_path: PathBuf::from("dest.md"),
                mtime: 1,
                size: 2,
            }])
        }

        async fn read(&self, candidate: &SyncCandidate) -> Result<SyncDocument> {
            Ok(SyncDocument {
                path: candidate.dest_path.to_string_lossy().into_owned(),
                content: "# Dummy".into(),
                frontmatter: None,
                title: "Dummy".into(),
                tags: vec!["test".into()],
                links: vec![],
                source_type: SyncSourceType::CodexSession,
                mtime: candidate.mtime,
            })
        }
    }

    #[test]
    fn sync_result_defaults_to_zero_and_no_errors() {
        let result = SyncResult::default();

        assert_eq!(result.files_added, 0);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn sync_source_trait_returns_candidates_and_documents() {
        let source = DummySource;
        let candidates = source.scan().await.unwrap();
        let document = source.read(&candidates[0]).await.unwrap();

        assert_eq!(source.id(), "dummy");
        assert_eq!(document.title, "Dummy");
        assert_eq!(document.source_type, SyncSourceType::CodexSession);
    }

    #[test]
    fn wikilink_roundtrips_through_serde() {
        let link = WikiLink {
            source: "a.md".into(),
            target: "b".into(),
            display: Some("B".into()),
            anchor: Some("section".into()),
        };

        let json = serde_json::to_string(&link).unwrap();
        let decoded: WikiLink = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, link);
    }
}
