/// Parser for Codex CLI sessions (~/.codex/sessions/)
///
/// Session format (JSONL, one JSON object per line):
/// - type: "session_meta" | "response_item"
/// - payload.role: "user" | "assistant"
/// - payload.content: Array of {type, text} or {type, input_text}
/// - timestamp: ISO 8601
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSession {
    pub session_id: String,
    pub cwd: Option<String>,
    pub timestamp: Option<String>,
    pub messages: Vec<CodexMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    line_type: String,
    timestamp: Option<String>,
    payload: Option<serde_json::Value>,
}

/// Parse all Codex sessions under `sessions_dir` (year/month/day/file.jsonl structure)
pub fn parse_all_sessions(sessions_dir: &Path) -> Result<Vec<CodexSession>> {
    let mut sessions = Vec::new();

    for entry in WalkDir::new(sessions_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().map(|x| x == "jsonl").unwrap_or(false)
        })
    {
        let path = entry.path();
        match parse_session_file(path) {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {}
            Err(e) => {
                eprintln!("  [warn] Failed to parse {}: {}", path.display(), e);
            }
        }
    }

    Ok(sessions)
}

/// Parse a single Codex JSONL session file
pub fn parse_session_file(path: &Path) -> Result<Option<CodexSession>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let mut session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut cwd = None;
    let mut session_timestamp = None;
    let mut messages = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let raw: RawLine = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "  [warn] Line {} in {} failed to parse: {}",
                    line_num + 1,
                    path.display(),
                    e
                );
                continue;
            }
        };

        match raw.line_type.as_str() {
            "session_meta" => {
                if let Some(payload) = &raw.payload {
                    if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                        session_id = id.to_string();
                    }
                    if let Some(c) = payload.get("cwd").and_then(|v| v.as_str()) {
                        cwd = Some(c.to_string());
                    }
                    if let Some(ts) = payload.get("timestamp").and_then(|v| v.as_str()) {
                        session_timestamp = Some(ts.to_string());
                    }
                }
            }
            "response_item" => {
                if let Some(payload) = &raw.payload {
                    let role = payload
                        .get("role")
                        .and_then(|r| r.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // Only user and assistant messages are meaningful
                    if !matches!(role.as_str(), "user" | "assistant") {
                        continue;
                    }

                    let content = extract_content_text(payload.get("content"));

                    if !content.trim().is_empty() {
                        messages.push(CodexMessage {
                            role,
                            content,
                            timestamp: raw.timestamp,
                        });
                    }
                }
            }
            _ => {} // Ignore other types
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    Ok(Some(CodexSession {
        session_id,
        cwd,
        timestamp: session_timestamp,
        messages,
    }))
}

/// Extract plain text from Codex content (array of content blocks)
pub fn extract_content_text(content: Option<&serde_json::Value>) -> String {
    match content {
        None => String::new(),
        Some(serde_json::Value::Array(arr)) => {
            arr.iter()
                .filter_map(|item| {
                    // Codex uses "input_text" for user, "output_text" for assistant
                    let text_keys = ["text", "input_text", "output_text"];
                    for key in &text_keys {
                        if let Some(text) = item.get(key).and_then(|t| t.as_str()) {
                            if !text.trim().is_empty() {
                                return Some(text.to_string());
                            }
                        }
                    }
                    // Tool calls
                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                        let args = item
                            .get("arguments")
                            .or_else(|| item.get("input"))
                            .map(|a| a.to_string())
                            .unwrap_or_default();
                        return Some(format!("[tool: {} {}]", name, args));
                    }
                    None
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_codex_session_basic() {
        let mut file = NamedTempFile::new().unwrap();

        writeln!(
            file,
            r#"{{"type":"session_meta","timestamp":"2026-01-01T00:00:00Z","payload":{{"id":"sess-001","cwd":"/home/test","timestamp":"2026-01-01T00:00:00Z"}}}}"#
        ).unwrap();
        writeln!(
            file,
            r#"{{"type":"response_item","timestamp":"2026-01-01T00:01:00Z","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"What is Rust?"}}]}}}}"#
        ).unwrap();
        writeln!(
            file,
            r#"{{"type":"response_item","timestamp":"2026-01-01T00:02:00Z","payload":{{"type":"message","role":"assistant","content":[{{"type":"text","text":"Rust is a systems programming language."}}]}}}}"#
        ).unwrap();

        let session = parse_session_file(file.path()).unwrap().unwrap();

        assert_eq!(session.session_id, "sess-001");
        assert_eq!(session.cwd, Some("/home/test".to_string()));
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, "user");
        assert!(session.messages[0].content.contains("What is Rust?"));
        assert_eq!(session.messages[1].role, "assistant");
    }

    #[test]
    fn test_parse_codex_session_empty() {
        let mut file = NamedTempFile::new().unwrap();

        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"id":"sess-empty"}}}}"#
        )
        .unwrap();

        let session = parse_session_file(file.path()).unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn test_extract_content_text_array() {
        let val = serde_json::json!([
            {"type": "input_text", "text": "Hello from user"}
        ]);
        assert_eq!(extract_content_text(Some(&val)), "Hello from user");
    }

    #[test]
    fn test_extract_content_text_output() {
        let val = serde_json::json!([
            {"type": "output_text", "text": "Response from assistant"}
        ]);
        assert_eq!(extract_content_text(Some(&val)), "Response from assistant");
    }
}
