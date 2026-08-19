use super::*;
use tempfile::tempdir;

#[test]
fn test_tokenize_text_normalizes_punctuation() {
    let tokens = tokenize_text("Auth, JWT/refresh-token rotation!");
    assert_eq!(tokens, vec!["auth", "jwt", "refresh", "token", "rotation"]);
}

#[test]
fn test_chunker_profile_scales_with_file_size() {
    assert_eq!(
        chunker_profile_for_file_size(8 * 1024),
        (DEFAULT_CHUNK_MAX_TOKENS, DEFAULT_CHUNK_OVERLAP_TOKENS)
    );
    assert_eq!(
        chunker_profile_for_file_size(MEDIUM_FILE_THRESHOLD_BYTES),
        (
            MEDIUM_FILE_CHUNK_MAX_TOKENS,
            MEDIUM_FILE_CHUNK_OVERLAP_TOKENS
        )
    );
    assert_eq!(
        chunker_profile_for_file_size(LARGE_FILE_THRESHOLD_BYTES),
        (LARGE_FILE_CHUNK_MAX_TOKENS, LARGE_FILE_CHUNK_OVERLAP_TOKENS)
    );
    assert_eq!(
        chunker_profile_for_file_size(EXTRA_LARGE_FILE_THRESHOLD_BYTES),
        (
            EXTRA_LARGE_FILE_CHUNK_MAX_TOKENS,
            EXTRA_LARGE_FILE_CHUNK_OVERLAP_TOKENS
        )
    );
}

#[test]
fn test_collect_folder_files_respects_mementoignore() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let keep = dir.path().join("keep.md");
    let nested_dir = dir.path().join("journal");
    let nested_ignored = nested_dir.join("skip.log");
    let private_dir = dir.path().join("private");
    let private_note = private_dir.join("secret.md");

    fs::create_dir_all(&nested_dir).unwrap();
    fs::create_dir_all(&private_dir).unwrap();
    fs::write(dir.path().join(".mementoignore"), "*.log\n/private/\n").unwrap();
    fs::write(&keep, "# Keep\nimportant memory").unwrap();
    fs::write(&nested_ignored, "debug noise").unwrap();
    fs::write(&private_note, "# Secret\nshould not be indexed").unwrap();

    let files = mgr
        .collect_folder_files(dir.path().to_string_lossy().as_ref())
        .unwrap();
    let indexed: Vec<_> = files.into_iter().map(|file| file.path).collect();

    assert!(indexed.iter().any(|path| path.ends_with("keep.md")));
    assert!(!indexed.iter().any(|path| path.ends_with("skip.log")));
    assert!(!indexed.iter().any(|path| path.ends_with("secret.md")));
}

#[test]
fn test_prepare_documents_inlines_chunk_content_for_extra_large_docs() {
    let huge = "a".repeat(EXTRA_LARGE_FILE_THRESHOLD_BYTES as usize);
    let docs = prepare_documents_from_chunks(vec![Chunk {
        content: huge.clone(),
        chunk_index: 0,
        token_count: huge.len().div_ceil(4),
        metadata: libmemento::chunker::ChunkMetadata {
            source_path: "/tmp/huge.md".to_string(),
            section_title: Some("Huge".to_string()),
            chunk_type: libmemento::chunker::ChunkType::DocumentSection,
        },
    }]);

    assert_eq!(docs.len(), 1);
    assert!(docs[0].canonical_text.is_empty());
    assert!(docs[0].chunks[0].span.is_none());
    assert_eq!(docs[0].chunks[0].content.len(), huge.len());
}

#[tokio::test]
async fn test_query_matches_normalized_tokens() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let file_path = dir.path().join("notes.md");

    fs::write(
        &file_path,
        "Authentication, JWT, and refresh-token rotation are required for the login flow.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "refresh token?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response
        .results
        .iter()
        .all(|result| (0.0..=1.0).contains(&result.score)));
    assert!(response.results[0]
        .content
        .to_lowercase()
        .contains("refresh-token"));
    assert_eq!(response.query_tokens, 2);
    assert!(!response.answer.is_empty());
}

#[tokio::test]
async fn test_query_matches_source_path_metadata() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let file_path = dir.path().join("obsidian-sync-blueprint.md");

    fs::write(
        &file_path,
        "We discussed local retrieval kernels and canonical memory storage.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(file_path.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "obsidian sync blueprint".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("obsidian-sync-blueprint"));
}

#[tokio::test]
async fn test_query_surfaces_wikilinked_evidence_without_lexical_overlap() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let source = dir.path().join("nebula-gateway.md");
    let evidence = dir.path().join("decision-ledger.md");

    fs::write(
        &source,
        "# Nebula Gateway\n\nThe supporting evidence is in [[Decision Ledger]].",
    )
    .unwrap();
    fs::write(
        &evidence,
        "# Decision Ledger\n\nApproved cobalt after the final architecture review.",
    )
    .unwrap();

    for path in [&source, &evidence] {
        mgr.import(&ImportRequest {
            source: "file".to_string(),
            path: Some(path.to_string_lossy().to_string()),
        })
        .await
        .unwrap();
    }

    let response = mgr
        .query(&QueryRequest {
            query: "where is the supporting evidence for nebula gateway?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(response
        .results
        .iter()
        .any(|result| result.source_path.ends_with("decision-ledger.md")));
}

#[tokio::test]
async fn test_query_recall_intent_prefers_episodic_memory_over_guide() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let episodic = dir.path().join("2026-01-23-daily-review.md");
    let guide = dir.path().join("one-hour-transformation-guide.md");

    fs::write(
        &episodic,
        "# 2026-01-23 Daily Review\n\nWe recorded the one-hour building pattern and session plan.",
    )
    .unwrap();
    fs::write(
        &guide,
        "# One Hour Transformation Guide\n\nThis guide explains the one-hour building pattern in general.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(episodic.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(guide.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about hour, building, pattern?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("2026-01-23-daily-review"));
}

#[tokio::test]
async fn test_query_rare_terms_beat_generic_tracking_doc() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let specific = dir.path().join("2026-01-30-gtm-notion.md");
    let generic = dir.path().join("tracking-weekly-overview.md");

    fs::write(
        &specific,
        "# GTM Notion\n\nTracking migrado para Notion e Atlas com tags de campanha.",
    )
    .unwrap();
    fs::write(
        &generic,
        "# Tracking Weekly Overview\n\nTracking para growth, setup e cadence do time.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(specific.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(generic.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about tracking, migrado, para?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("gtm-notion"));
}

#[tokio::test]
async fn test_query_recall_intent_prefers_session_note_over_review() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let session_note = dir.path().join("2026-01-23.md");
    let review = dir.path().join("2026-01-25-evening-review.md");

    fs::write(
        &session_note,
        "# 2026-01-23\n\nWe recorded the hour building pattern and Atlas plan directly in session.",
    )
    .unwrap();
    fs::write(
        &review,
        "# 2026-01-25 Evening Review\n\nWe reviewed the hour building pattern and summarized the work from the session.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(session_note.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(review.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about hour, building, pattern?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .ends_with("2026-01-23.md"));
}

#[tokio::test]
async fn test_query_review_terms_prefer_review_memory_class() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let review = dir.path().join("2026-01-25-evening-review.md");
    let session_note = dir.path().join("2026-01-25.md");

    fs::write(
        &review,
        "# Evening Review\n\nEvening review about hour building progress and blockers.",
    )
    .unwrap();
    fs::write(
        &session_note,
        "# 2026-01-25\n\nWe built for one hour and captured the raw progress notes.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(review.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(session_note.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about evening, review, building?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("evening-review"));
}

#[tokio::test]
async fn test_query_research_terms_prefer_research_memory_class() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let research = dir.path().join("2025-06-21-dan-koe-inspirations.md");
    let daily = dir.path().join("2026-03-03.md");

    fs::write(
        &research,
        "# Dan Koe Inspirations\n\nArticle insights about the future of leverage and writing systems.",
    )
    .unwrap();
    fs::write(
        &daily,
        "# 2026-03-03\n\nWe had a generic workday and some future planning notes.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(research.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(daily.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about article, insights, future?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("dan-koe-inspirations"));
}

#[tokio::test]
async fn test_query_ascii_fold_matches_accented_title_lookup() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let profile = dir.path().join("2026-01-29-jose-roberto-itprofile.md");
    let generic = dir.path().join("2026-03-03.md");

    fs::write(
        &profile,
        "# José Roberto - IT Profile\n\nJosé Roberto leads infrastructure and IT architecture.",
    )
    .unwrap();
    fs::write(
        &generic,
        "# 2026-03-03\n\nA generic day note mentioning profile updates and architecture.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(profile.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(generic.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what does Jose Roberto - IT Profile say?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("jose-roberto-itprofile"));
}

#[tokio::test]
async fn test_document_lookup_prefers_titled_doc_over_daily_reference() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let target = dir.path().join("2026-01-29-jose-roberto-itprofile.md");
    let daily = dir.path().join("2026-03-02.md");

    fs::write(
        &target,
        "# José Roberto - IT Profile\n\nFocused profile page for José Roberto and his IT scope.",
    )
    .unwrap();
    fs::write(
        &daily,
        "# 2026-03-02\n\nTalked with José Roberto about architecture and some profile updates.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(target.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(daily.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what does José Roberto - IT Profile say?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("jose-roberto-itprofile"));
}

#[tokio::test]
async fn test_query_title_exactness_beats_generic_dense_note() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let review = dir.path().join("2026-01-30-evening-review.md");
    let dense = dir.path().join("2026-02-02-pitch-deck.md");

    fs::write(
        &review,
        "# Evening Review\n\nFriday evening emails and review of the communication backlog.",
    )
    .unwrap();
    fs::write(
        &dense,
        "# Pitch Deck\n\nA dense note about evening communication, building, messaging, emails, and deck work.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(review.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(dense.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about evening, review, emails?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("evening-review"));
}

#[test]
fn test_new_bootstraps_classification_rules_config() {
    let dir = tempdir().unwrap();
    let _mgr = MementoManager::new(dir.path()).unwrap();
    let path = classification_rules_path(dir.path());

    assert!(path.exists());
    let payload = fs::read_to_string(path).unwrap();
    let config: ClassificationRulesConfig = serde_json::from_str(&payload).unwrap();
    assert!(!config.rules.is_empty());
}

#[test]
fn test_build_result_bundles_attach_supporting_chunk() {
    let chunks = vec![
        StoredChunk {
            chunk_id: 1,
            doc_id: 7,
            span: Some(TextSpan { start: 0, end: 10 }),
            content: "first".to_string(),
            chunk_index: 0,
            token_count: 2,
            source_path: "/tmp/example.md".to_string(),
            section_title: None,
            chunk_type: "paragraph".to_string(),
            token_ids: vec![1, 2],
        },
        StoredChunk {
            chunk_id: 2,
            doc_id: 7,
            span: Some(TextSpan { start: 12, end: 22 }),
            content: "second".to_string(),
            chunk_index: 1,
            token_count: 2,
            source_path: "/tmp/example.md".to_string(),
            section_title: None,
            chunk_type: "paragraph".to_string(),
            token_ids: vec![3, 4],
        },
    ];
    let by_document = BTreeMap::from([(
        "/tmp/example.md".to_string(),
        vec![
            ChunkRanking {
                idx: 0,
                doc_id: 7,
                source_path: "/tmp/example.md".to_string(),
                score: 0.9,
                metadata_score: 0.1,
                metadata_bonus: 0.0,
                exactness_score: 0.0,
                entity_score: 0.0,
                query_coverage_score: 0.7,
                graph_score: 0.0,
            },
            ChunkRanking {
                idx: 1,
                doc_id: 7,
                source_path: "/tmp/example.md".to_string(),
                score: 0.7,
                metadata_score: 0.1,
                metadata_bonus: 0.0,
                exactness_score: 0.0,
                entity_score: 0.0,
                query_coverage_score: 0.5,
                graph_score: 0.0,
            },
        ],
    )]);
    let document_rankings = vec![("/tmp/example.md".to_string(), 7, 0.95, 0)];

    let documents = vec![StoredDocument {
        doc_id: 7,
        source_path: "/tmp/example.md".to_string(),
        canonical_text: "first\nsecond".to_string(),
        title: Some("Example".to_string()),
    }];
    let lexical_index = LexicalIndex::build(&chunks, &documents);
    let bundles =
        build_result_bundles(&document_rankings, &by_document, &chunks, &lexical_index, 5);

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].chunk_indices, vec![0, 1]);
}

#[tokio::test]
async fn test_frontmatter_tag_rule_can_promote_project_note() {
    let dir = tempdir().unwrap();
    let config_path = classification_rules_path(dir.path());
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&ClassificationRulesConfig {
            rules: vec![ClassificationRule {
                name: "tagged projects".to_string(),
                class: "project_note".to_string(),
                path_contains: Vec::new(),
                title_contains: Vec::new(),
                tag_contains: vec!["project/active".to_string()],
                content_contains: Vec::new(),
                priority: 100,
            }],
        })
        .unwrap(),
    )
    .unwrap();

    let mgr = MementoManager::new(dir.path()).unwrap();
    let tagged = dir.path().join("work-note.md");
    let generic = dir.path().join("generic-memory.md");

    fs::write(
        &tagged,
        "---\ntags: [project/active]\n---\n# Work Note\n\nTracking migrado para Notion com plano de campanha.",
    )
    .unwrap();
    fs::write(
        &generic,
        "# Generic Memory\n\nTracking and migration notes from a generic memory.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(tagged.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(generic.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about tracking, migrado, para?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("work-note"));
}

#[tokio::test]
async fn test_contextual_freshness_prefers_more_recent_matching_review() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let older = dir.path().join("2026-01-23.md");
    let fresher = dir.path().join("2026-01-30-evening-review.md");

    fs::write(
        &older,
        "# 2026-01-23 Daily Review & Planning\n\nFriday planning and generic daily review notes.",
    )
    .unwrap();
    fs::write(
        &fresher,
        "# 2026-01-30 - Friday Evening Review\n\nFriday evening emails review with explicit follow-up actions.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(older.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(fresher.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about friday, evening, emails?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("2026-01-30-evening-review"));
}

#[tokio::test]
async fn test_specific_project_note_beats_dense_generic_recent_note() {
    let dir = tempdir().unwrap();
    let mgr = MementoManager::new(dir.path()).unwrap();
    let project = dir.path().join("2026-01-30-gtm-notion.md");
    let generic = dir.path().join("2026-02-25.md");

    fs::write(
        &project,
        "# 2026-01-30 - GTM Tracking migrado para Notion\n\nTracking migrado para Notion com baseline GTM e views futuras.",
    )
    .unwrap();
    fs::write(
        &generic,
        "# 2026-02-25\n\nDense recent note about multiple topics, product ideas, and unrelated ops work.",
    )
    .unwrap();

    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(project.to_string_lossy().to_string()),
    })
    .await
    .unwrap();
    mgr.import(&ImportRequest {
        source: "file".to_string(),
        path: Some(generic.to_string_lossy().to_string()),
    })
    .await
    .unwrap();

    let response = mgr
        .query(&QueryRequest {
            query: "what did we record about tracking, migrado, para?".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .source_path
        .to_lowercase()
        .contains("gtm-notion"));
}

#[tokio::test]
async fn test_sync_obsidian_reindexes_vault_without_duplicates() {
    let dir = tempdir().unwrap();
    let vault_path = dir.path().join("vault");
    fs::create_dir_all(vault_path.join(".obsidian")).unwrap();
    fs::write(
        vault_path.join(".obsidian").join("workspace.json"),
        "{\"ui\":\"ignore\"}",
    )
    .unwrap();
    fs::write(
        vault_path.join("daily.md"),
        "# Daily\n\nWe discussed eigenvector memory and local retrieval.",
    )
    .unwrap();

    let mgr = MementoManager::new(dir.path()).unwrap();
    let request = ImportRequest {
        source: "obsidian".to_string(),
        path: Some(vault_path.to_string_lossy().to_string()),
    };

    let first = mgr.sync(&request).await.unwrap();
    assert!(first.chunks_synced > 0);

    fs::write(
        vault_path.join("daily.md"),
        "# Daily\n\nWe discussed eigenvector memory, local retrieval, and obsidian sync.",
    )
    .unwrap();

    let second = mgr.sync(&request).await.unwrap();
    assert!(second.removed_chunks > 0);

    let status = mgr.status().await;
    assert_eq!(status.total_sources, 1);

    let response = mgr
        .query(&QueryRequest {
            query: "obsidian sync".to_string(),
            top_k: 5,
        })
        .await
        .unwrap();

    assert!(!response.results.is_empty());
    assert!(response.results[0]
        .content
        .to_lowercase()
        .contains("obsidian sync"));

    let third = mgr.sync(&request).await.unwrap();
    assert_eq!(third.chunks_synced, 0);
    assert_eq!(third.updated_files, 0);
    assert_eq!(third.removed_files, 0);
    assert!(third.unchanged_files >= 1);
    assert!(mgr.load_active_operation().is_none());
}
