/// Parser for Claude Code sessions (~/.claude/projects/)
///
/// Session format (JSONL, one JSON object per line):
/// - type: "user" | "assistant" | "file-history-snapshot" | ...
/// - sessionId: UUID
/// - cwd: working directory
/// - gitBranch: current branch
/// - message.role: "user" | "assistant"
/// - message.content: String or Array of {type, text}
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub session_id: String,
    pub project_name: String,
    pub project_path: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: String,
    pub message_id: Option<String>,
    pub timestamp: Option<String>,
}

/// Raw JSONL line structure
#[derive(Debug, Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    #[serde(rename = "sessionId")]
    #[allow(dead_code)]
    session_id: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    message: Option<RawMessage>,
    #[serde(rename = "messageId")]
    message_id: Option<String>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

/// Parse all Claude sessions under `projects_dir`
pub fn parse_all_sessions(projects_dir: &Path) -> Result<Vec<ClaudeSession>> {
    let mut sessions = Vec::new();

    for entry in WalkDir::new(projects_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().map(|x| x == "jsonl").unwrap_or(false)
        })
    {
        let path = entry.path();
        match parse_session_file(path) {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {} // Empty or skipped
            Err(e) => {
                // Non-fatal: log and continue
                eprintln!("  [warn] Failed to parse {}: {}", path.display(), e);
            }
        }
    }

    Ok(sessions)
}

/// Parse a single JSONL session file
pub fn parse_session_file(path: &Path) -> Result<Option<ClaudeSession>> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    // Derive session ID from filename
    let session_id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Derive project name from parent directory name
    let project_path = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let project_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|name| derive_project_name(&name.to_string_lossy()))
        .unwrap_or_else(|| "unknown".to_string());

    let mut messages = Vec::new();
    let mut session_cwd = None;
    let mut session_branch = None;

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

        // Capture session metadata from first message
        if session_cwd.is_none() {
            session_cwd = raw.cwd.clone();
        }
        if session_branch.is_none() {
            session_branch = raw.git_branch.clone();
        }

        // Skip non-message lines
        let line_type = raw.line_type.as_deref().unwrap_or("");
        if !matches!(line_type, "user" | "assistant") {
            continue;
        }

        if let Some(msg) = raw.message {
            let role = msg.role.unwrap_or_else(|| line_type.to_string());
            let content = extract_content_text(&msg.content);

            if !content.trim().is_empty() {
                messages.push(ClaudeMessage {
                    role,
                    content,
                    message_id: raw.message_id,
                    timestamp: raw.timestamp,
                });
            }
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    Ok(Some(ClaudeSession {
        session_id,
        project_name,
        project_path,
        cwd: session_cwd,
        git_branch: session_branch,
        messages,
    }))
}

fn derive_project_name(encoded_path: &str) -> String {
    let parts = encoded_path
        .trim_start_matches('-')
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let start = usize::from(
        parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("users") || *part == "home"),
    ) * 2;
    let project = parts.into_iter().skip(start).collect::<Vec<_>>().join("/");
    if project.is_empty() {
        "unknown".to_string()
    } else {
        project
    }
}

/// Extract plain text from message content (string or array of content blocks)
pub fn extract_content_text(content: &Option<serde_json::Value>) -> String {
    match content {
        None => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => {
            arr.iter()
                .filter_map(|item| {
                    // Handle {type: "text", text: "..."}
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        if !text.trim().is_empty() {
                            return Some(text.to_string());
                        }
                    }
                    // Handle tool use — include tool name + input for context
                    if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                            let input =
                                item.get("input").map(|i| i.to_string()).unwrap_or_default();
                            return Some(format!("[tool: {} {}]", name, input));
                        }
                    }
                    None
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Some(v) => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_session_file_basic() {
        let mut file = NamedTempFile::new().unwrap();
        let session_id = "test-session-1";

        // Write a minimal valid session
        writeln!(
            file,
            r#"{{"type":"user","sessionId":"{session_id}","cwd":"/home/test","gitBranch":"main","message":{{"role":"user","content":"Hello, how do I set up a Rust project?"}}}}"#
        ).unwrap();
        writeln!(
            file,
            r#"{{"type":"assistant","sessionId":"{session_id}","message":{{"role":"assistant","content":"Use `cargo new` to create a new Rust project!"}}}}"#
        ).unwrap();

        let path = file.path();
        let session = parse_session_file(path).unwrap().unwrap();

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.cwd, Some("/home/test".to_string()));
        assert_eq!(session.git_branch, Some("main".to_string()));
    }

    #[test]
    fn test_project_name_removes_generic_home_prefix_and_username() {
        assert_eq!(
            derive_project_name("-Users-alice-work-Memento"),
            "work/Memento"
        );
        assert_eq!(derive_project_name("-home-bob-notes"), "notes");
    }

    #[test]
    fn test_parse_session_skips_snapshots() {
        let mut file = NamedTempFile::new().unwrap();

        // Only a snapshot line — should return None
        writeln!(
            file,
            r#"{{"type":"file-history-snapshot","messageId":"abc","snapshot":{{"timestamp":"2026-01-01T00:00:00Z"}}}}"#
        ).unwrap();

        let session = parse_session_file(file.path()).unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn test_extract_content_text_string() {
        let val = Some(serde_json::Value::String("Hello world".into()));
        assert_eq!(extract_content_text(&val), "Hello world");
    }

    #[test]
    fn test_extract_content_text_array() {
        let val = serde_json::json!([
            {"type": "text", "text": "First block"},
            {"type": "text", "text": "Second block"}
        ]);
        let result = extract_content_text(&Some(val));
        assert!(result.contains("First block"));
        assert!(result.contains("Second block"));
    }

    #[test]
    fn test_extract_content_text_tool_use() {
        let val = serde_json::json!([
            {"type": "tool_use", "name": "Read", "input": {"file_path": "main.rs"}}
        ]);
        let result = extract_content_text(&Some(val));
        assert!(result.contains("[tool: Read"));
    }

    #[test]
    fn test_parse_session_skips_empty_content() {
        let mut file = NamedTempFile::new().unwrap();

        writeln!(
            file,
            r#"{{"type":"user","sessionId":"s1","message":{{"role":"user","content":"   "}}}}"#
        )
        .unwrap();

        let session = parse_session_file(file.path()).unwrap();
        assert!(session.is_none()); // No meaningful messages
    }
}
