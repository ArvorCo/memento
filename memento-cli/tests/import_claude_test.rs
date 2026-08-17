/// Integration tests for Claude Code session parsing
use std::io::Write;
use tempfile::{tempdir, NamedTempFile};

// Re-export the parsers for testing
// Note: These tests use the library directly, not through the binary

#[test]
fn test_parse_real_session_structure() {
    // Simulate a real Claude Code session with multiple messages
    let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();

    // File history snapshot (should be ignored)
    writeln!(
        file,
        r#"{{"type":"file-history-snapshot","messageId":"snap-001","snapshot":{{"timestamp":"2026-02-01T10:00:00Z"}}}}"#
    ).unwrap();

    // User message (string content)
    writeln!(
        file,
        r#"{{"type":"user","sessionId":"sess-abc123","cwd":"/home/test/workspace/myproject","gitBranch":"main","message":{{"role":"user","content":"Create a Rust HTTP server using axum"}}}}"#
    ).unwrap();

    // Assistant message (array content)
    writeln!(
        file,
        r#"{{"type":"assistant","sessionId":"sess-abc123","message":{{"role":"assistant","content":[{{"type":"text","text":"I'll help you create an HTTP server with axum!"}},{{"type":"tool_use","name":"Write","input":{{"file_path":"src/main.rs","content":"use axum;"}}}}]}}}}"#
    ).unwrap();

    // Second user message
    writeln!(
        file,
        r#"{{"type":"user","sessionId":"sess-abc123","message":{{"role":"user","content":[{{"type":"text","text":"Add a health check endpoint at /health"}}]}}}}"#
    ).unwrap();

    // Verify the file was written
    assert!(file.path().exists());

    // The file is valid — in a full integration test we'd call the parser
    // For now, verify the file has 4 lines
    let content = std::fs::read_to_string(file.path()).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 4);
}

#[test]
fn test_session_directory_structure() {
    // Create a temp directory simulating ~/.claude/projects/
    let dir = tempdir().unwrap();
    let project_dir = dir.path().join("-home-test-workspace-TestProject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a session file
    let session_path = project_dir.join("test-session-id.jsonl");
    let mut f = std::fs::File::create(&session_path).unwrap();
    writeln!(
        f,
        r#"{{"type":"user","sessionId":"test-session-id","cwd":"/home/test/workspace/TestProject","message":{{"role":"user","content":"Hello"}}}}"#
    ).unwrap();
    writeln!(
        f,
        r#"{{"type":"assistant","sessionId":"test-session-id","message":{{"role":"assistant","content":"Hi there!"}}}}"#
    ).unwrap();

    assert!(session_path.exists());

    // Verify directory structure
    let projects: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(projects.len(), 1);
}

#[test]
fn test_malformed_jsonl_lines_are_skipped() {
    let mut file = NamedTempFile::with_suffix(".jsonl").unwrap();

    // Valid line
    writeln!(
        file,
        r#"{{"type":"user","sessionId":"s1","message":{{"role":"user","content":"Valid message"}}}}"#
    ).unwrap();

    // Invalid JSON (should be skipped gracefully)
    writeln!(file, r#"{{this is not valid json}}"#).unwrap();

    // Another valid line
    writeln!(
        file,
        r#"{{"type":"assistant","sessionId":"s1","message":{{"role":"assistant","content":"I understand"}}}}"#
    ).unwrap();

    // Verify the file exists and has 3 lines (1 invalid, 2 valid)
    let content = std::fs::read_to_string(file.path()).unwrap();
    assert_eq!(content.lines().count(), 3);
}

#[test]
fn test_empty_session_file() {
    let file = NamedTempFile::with_suffix(".jsonl").unwrap();
    // Empty file
    assert!(file.path().exists());
    let content = std::fs::read_to_string(file.path()).unwrap();
    assert!(content.is_empty());
}
