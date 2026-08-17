use super::*;
use libmemento::storage::{EmbeddingSegmentFile, GraphSegmentFile, ManifestStore, SegmentKind};
use tempfile::tempdir;

#[tokio::test]
async fn test_status_reports_active_operation_checkpoint() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let mut checkpoint = OperationCheckpoint::new("sync", "obsidian", "/tmp/vault");
    checkpoint.phase = "ingesting".to_string();
    checkpoint.total_files = 12;
    checkpoint.processed_files = 3;
    checkpoint.current_file_path = Some("/tmp/vault/huge.md".to_string());
    checkpoint.current_file_size_bytes = Some(6 * 1024 * 1024);
    checkpoint.touch();
    mgr.save_active_operation(&checkpoint).unwrap();

    let status = mgr.status().await;
    let active = status.active_operation.expect("expected active operation");
    assert_eq!(active.phase, "ingesting");
    assert_eq!(active.processed_files, 3);
    assert_eq!(
        active.current_file_path.as_deref(),
        Some("/tmp/vault/huge.md")
    );
    assert_eq!(active.current_file_size_bytes, Some(6 * 1024 * 1024));
}

#[tokio::test]
async fn test_active_operation_prefers_checkpoint_snapshot_on_restart() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let file_path = dir.path().join("checkpoint.md");
    fs::write(
        &file_path,
        "Checkpoint snapshots should survive interrupted sync batches.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let mut checkpoint = OperationCheckpoint::new("sync", "folder", "/tmp/vault");
    checkpoint.phase = "checkpointing".to_string();
    mgr.save_active_operation(&checkpoint).unwrap();

    {
        let mut state = mgr.state.write().await;
        state.domain = "checkpoint-recovery".to_string();
    }

    mgr.save_checkpoint().await.unwrap();

    let reloaded = MementoManager::new(dir.path()).unwrap();
    let status = reloaded.status().await;
    assert_eq!(status.total_sources, 1);
    assert_eq!(status.domain, "checkpoint-recovery");
}

#[tokio::test]
async fn test_save_publishes_runtime_manifest() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let file_path = dir.path().join("notes.md");

    fs::write(
        &file_path,
        "Eigenvector memory keeps one substrate and many views.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let manifest_store = ManifestStore::init(dir.path()).unwrap();
    let manifest = manifest_store.load_current().unwrap().unwrap();

    assert_eq!(manifest.generation, 1);
    assert_eq!(manifest.active_segments.len(), 4);
    assert!(manifest
        .active_segments
        .iter()
        .any(|segment| segment.kind == SegmentKind::LegacySnapshot));
    assert!(manifest
        .active_segments
        .iter()
        .any(|segment| segment.kind == SegmentKind::Lexical));
    assert!(manifest
        .active_segments
        .iter()
        .any(|segment| segment.kind == SegmentKind::Metadata));
    assert!(manifest
        .active_segments
        .iter()
        .any(|segment| segment.kind == SegmentKind::Graph));
    assert!(manifest
        .active_segments
        .iter()
        .any(|segment| segment.relative_path == "default.memento"));

    let graph_descriptor = manifest
        .active_segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Graph)
        .unwrap();
    let graph: GraphSegmentFile = manifest_store.read_segment(graph_descriptor).unwrap();
    assert!(!graph.doc_chunk_edges.is_empty());
    assert!(!graph.chunk_token_adjacency.is_empty());
    assert!(!graph.token_graph_edges.is_empty());

    let status = mgr.status().await;
    assert!(status.runtime_segments_ready);
    assert!(status.runtime_graph_ready);
    assert_eq!(status.runtime_manifest_generation, 1);
    assert_eq!(status.runtime_segment_count, 4);
}

#[tokio::test]
async fn test_restart_loads_runtime_segments_without_legacy_fallback() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("restart.md");
    fs::write(
        &file_path,
        "A memory substrate should survive restarts through runtime segments.",
    )
    .unwrap();

    let mgr = MementoManager::new(dir.path()).unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    fs::remove_file(dir.path().join("default.memento")).unwrap();

    let restarted = MementoManager::new(dir.path()).unwrap();
    let response = restarted
        .query(&QueryRequest {
            query: "runtime segments".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .content
        .to_lowercase()
        .contains("runtime segments"));
}

#[tokio::test]
async fn test_learn_publishes_embedding_segment() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let file_path = dir.path().join("embed.md");

    fs::write(
        &file_path,
        "Embeddings should capture semantic memory through eigenvector projections.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let learn = mgr.learn().await.unwrap();
    assert!(learn.eigenvectors_computed > 0);

    let manifest_store = ManifestStore::init(dir.path()).unwrap();
    let manifest = manifest_store.load_current().unwrap().unwrap();
    let embedding_descriptor = manifest
        .active_segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::Embedding)
        .unwrap();
    let embeddings: EmbeddingSegmentFile =
        manifest_store.read_segment(embedding_descriptor).unwrap();

    assert!(embeddings.dimensions > 0);
    assert!(!embeddings.embeddings.is_empty());
    assert!(mgr.status().await.runtime_embedding_ready);
}
