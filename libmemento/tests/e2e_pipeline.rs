//! End-to-end test: mock claude session → parse → chunk → tokenize → matrix → eigen → .memento → query

use libmemento::chunker::smart::SmartChunker;
use libmemento::format::{
    MementoFile, SourceRecord, SourceType, StoredChunk, StoredDocument, TextSpan,
};
use libmemento::matrix::SemanticMatrix;
use libmemento::parser::claude::{ClaudeMessage, ClaudeSession};
use std::collections::HashMap;
use std::time::SystemTime;
use tempfile::NamedTempFile;

fn mock_claude_session() -> ClaudeSession {
    ClaudeSession {
        session_id: "test-session-001".to_string(),
        project_name: "test-project".to_string(),
        project_path: "/tmp/test-project".to_string(),
        cwd: Some("/tmp/test-project".to_string()),
        git_branch: Some("main".to_string()),
        messages: vec![
            ClaudeMessage {
                role: "user".to_string(),
                content: "How do we implement authentication in the Amigo app? We need JWT tokens and refresh token rotation.".to_string(),
                message_id: Some("msg-1".to_string()),
                timestamp: Some("2026-02-01T10:00:00Z".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: "For authentication in the Amigo app, I recommend using Supabase Auth with JWT tokens. Here's the implementation plan:\n\n1. Configure Supabase Auth with JWT\n2. Implement refresh token rotation in the middleware\n3. Add Edge Functions for token validation\n4. Store session tokens securely in the keychain\n\nThe authentication flow works as follows: user signs in, receives a JWT access token and a refresh token. The access token expires in 15 minutes, and the refresh token is rotated on each use.".to_string(),
                message_id: Some("msg-2".to_string()),
                timestamp: Some("2026-02-01T10:01:00Z".to_string()),
            },
            ClaudeMessage {
                role: "user".to_string(),
                content: "What about rate limiting for the authentication endpoints?".to_string(),
                message_id: Some("msg-3".to_string()),
                timestamp: Some("2026-02-01T10:02:00Z".to_string()),
            },
            ClaudeMessage {
                role: "assistant".to_string(),
                content: "For rate limiting the authentication endpoints, we should implement a sliding window rate limiter. I suggest 5 login attempts per minute per IP address. We can use Redis for distributed rate limiting, or a simple in-memory solution for the MVP. The rate limiter middleware should be placed before the authentication handler.".to_string(),
                message_id: Some("msg-4".to_string()),
                timestamp: Some("2026-02-01T10:03:00Z".to_string()),
            },
        ],
    }
}

fn tokenize(text: &str, vocab: &mut HashMap<String, usize>, next_id: &mut usize) -> Vec<usize> {
    text.split_whitespace()
        .map(|w| {
            let w = w.to_lowercase();
            *vocab.entry(w).or_insert_with(|| {
                let id = *next_id;
                *next_id += 1;
                id
            })
        })
        .collect()
}

#[test]
fn test_full_pipeline_parse_chunk_matrix_eigen_memento_query() {
    // 1. Parse mock session
    let session = mock_claude_session();
    assert_eq!(session.messages.len(), 4);

    // 2. Chunk the session
    let chunker = SmartChunker::new(512, 64);
    let chunks = chunker.chunk_claude_session(&session);
    assert!(
        !chunks.is_empty(),
        "Chunking should produce at least one chunk"
    );

    // 3. Tokenize + feed into SemanticMatrix
    let mut vocab: HashMap<String, usize> = HashMap::new();
    let mut next_id: usize = 0;
    let vocab_size = 5000;
    let mut matrix = SemanticMatrix::new(vocab_size);

    let canonical_text = chunks
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let stored_document = StoredDocument {
        doc_id: 0,
        source_path: "test-session".to_string(),
        canonical_text: canonical_text.clone(),
        title: Some("test-session".to_string()),
    };

    let mut stored_chunks = Vec::new();
    let mut cursor = 0usize;
    for chunk in &chunks {
        let token_ids = tokenize(&chunk.content, &mut vocab, &mut next_id);
        if token_ids.len() >= 2 {
            matrix
                .ingest_document(&token_ids)
                .expect("ingest_document should succeed");
        }
        let start = canonical_text[cursor..]
            .find(&chunk.content)
            .map(|offset| cursor + offset)
            .expect("chunk content should exist in canonical text");
        let end = start + chunk.content.len();
        cursor = end;
        stored_chunks.push(StoredChunk {
            chunk_id: chunk.chunk_index as u64,
            doc_id: stored_document.doc_id,
            span: Some(TextSpan { start, end }),
            content: String::new(),
            chunk_index: chunk.chunk_index,
            token_count: chunk.token_count,
            source_path: chunk.metadata.source_path.clone(),
            section_title: chunk.metadata.section_title.clone(),
            chunk_type: chunk.metadata.chunk_type.as_str().to_string(),
            token_ids,
        });
    }

    assert!(vocab.len() > 10, "Vocabulary should have tokens");
    assert!(
        matrix.non_zero_count() > 0,
        "Matrix should have co-occurrences"
    );

    // 4. Consolidate + compute eigendecomposition
    matrix.consolidate().expect("consolidate should succeed");
    let k = 10.min(vocab.len().saturating_sub(1));
    if k >= 2 {
        let eigen = matrix
            .compute_eigendecomposition(k)
            .expect("eigendecomposition should succeed");
        assert!(
            !eigen.eigenvalues.is_empty(),
            "Should compute at least one eigenvalue"
        );
    }

    // 5. Create .memento file
    let mut mf = MementoFile::from_matrix(&matrix, vocab.clone(), next_id, "test");
    mf.add_document(stored_document);
    mf.chunks = stored_chunks;
    mf.add_source(SourceRecord {
        path: "test-session".to_string(),
        source_type: SourceType::Claude,
        ingested_at: SystemTime::now(),
        chunk_count: chunks.len(),
        token_count: next_id,
    });

    // 6. Save to file
    let tmp = NamedTempFile::with_suffix(".memento").unwrap();
    let mut writer = std::fs::File::create(tmp.path()).unwrap();
    mf.save(&mut writer).unwrap();

    // 7. Load from file
    let mut reader = std::fs::File::open(tmp.path()).unwrap();
    let loaded = MementoFile::load(&mut reader).unwrap();

    assert_eq!(loaded.domain, "test");
    assert_eq!(loaded.sources.len(), 1);
    assert_eq!(loaded.sources[0].source_type, SourceType::Claude);
    assert_eq!(loaded.vocabulary.len(), vocab.len());
    assert!(!loaded.chunks.is_empty());

    // 8. Query — find chunks containing "authentication"
    let auth_token = loaded.vocabulary.get("authentication").copied();
    assert!(
        auth_token.is_some(),
        "Vocabulary should contain 'authentication'"
    );

    let query_tokens: Vec<usize> = ["authentication", "jwt", "token"]
        .iter()
        .filter_map(|w| loaded.vocabulary.get(*w).copied())
        .collect();
    assert!(!query_tokens.is_empty(), "Query tokens should resolve");

    // Score chunks by token overlap
    let mut scored: Vec<(usize, f64)> = loaded
        .chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let overlap: usize = query_tokens
                .iter()
                .filter(|t| chunk.token_ids.contains(t))
                .count();
            let score = overlap as f64 / query_tokens.len().max(1) as f64;
            (i, score)
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    assert!(
        !scored.is_empty(),
        "Query should return at least one matching chunk"
    );

    let docs = loaded.document_map();
    let best = &loaded.chunks[scored[0].0];
    let best_content = best.resolve_content(&docs);
    let content_lower = best_content.to_lowercase();
    assert!(
        content_lower.contains("auth")
            || content_lower.contains("jwt")
            || content_lower.contains("token"),
        "Best result should mention auth/jwt/token. Got: {}",
        &best_content[..100.min(best_content.len())]
    );
}

#[test]
fn test_memento_file_format_v2() {
    let mf = MementoFile::new("test-domain", 1000);

    let mut buf = Vec::new();
    mf.save(&mut buf).unwrap();

    // Check magic bytes
    assert_eq!(&buf[0..4], b"MNTO");
    // Check version
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 3);

    // Roundtrip
    let loaded = MementoFile::load(&mut buf.as_slice()).unwrap();
    assert_eq!(loaded.domain, "test-domain");
    assert_eq!(loaded.vocabulary_size, 1000);
}

#[test]
fn test_matrix_roundtrip_via_memento_file() {
    let mut matrix = SemanticMatrix::new(100);
    matrix.add_cooccurrence(0, 1, 1.0).unwrap();
    matrix.add_cooccurrence(1, 2, 0.5).unwrap();
    matrix.add_cooccurrence(0, 2, 0.3).unwrap();

    let mut vocab = HashMap::new();
    vocab.insert("hello".to_string(), 0);
    vocab.insert("world".to_string(), 1);
    vocab.insert("test".to_string(), 2);

    let mf = MementoFile::from_matrix(&matrix, vocab.clone(), 3, "roundtrip");

    let mut buf = Vec::new();
    mf.save(&mut buf).unwrap();
    let loaded = MementoFile::load(&mut buf.as_slice()).unwrap();

    let restored_matrix = loaded.to_matrix();
    assert_eq!(restored_matrix.vocabulary_size(), 100);
    assert!(restored_matrix.non_zero_count() > 0);
    assert_eq!(loaded.vocabulary.len(), 3);
}
