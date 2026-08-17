/// Smart chunking engine that respects semantic boundaries.
///
/// Strategy:
/// - Claude/Codex sessions: chunk by conversation turn, then by sliding window if turns are too long
/// - Documents: chunk by section heading, then by paragraph, with token limits
/// - Code blocks are kept intact when possible
/// - Sliding window with overlap for context continuity
use super::{Chunk, ChunkMetadata, ChunkType};
use crate::parser::claude::ClaudeSession;
use crate::parser::codex::CodexSession;

/// Approximate token count (1 token ≈ 4 chars)
fn approx_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut boundary = index.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut boundary = index.min(text.len());
    while boundary < text.len() && !text.is_char_boundary(boundary) {
        boundary += 1;
    }
    boundary
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    content: String,
    chunk_index: &mut usize,
    source_path: &str,
    section_title: Option<String>,
    chunk_type: ChunkType,
) {
    if content.trim().is_empty() {
        return;
    }

    let token_count = approx_tokens(&content);
    chunks.push(Chunk {
        content,
        chunk_index: *chunk_index,
        token_count,
        metadata: ChunkMetadata {
            source_path: source_path.to_string(),
            section_title,
            chunk_type,
        },
    });
    *chunk_index += 1;
}

pub struct SmartChunker {
    max_tokens: usize,
    overlap_tokens: usize,
}

impl SmartChunker {
    pub fn new(max_tokens: usize, overlap_tokens: usize) -> Self {
        Self {
            max_tokens,
            overlap_tokens,
        }
    }

    /// Chunk a Claude Code session into semantic chunks
    pub fn chunk_claude_session(&self, session: &ClaudeSession) -> Vec<Chunk> {
        let source_path = format!("claude://{}#{}", session.project_name, session.session_id);
        let mut chunks = Vec::new();
        let mut chunk_index = 0;

        // First pass: try individual conversation turns
        for msg in &session.messages {
            let header = format!("[{} → {}]\n", msg.role, session.project_name);
            let full_content = format!("{}{}", header, msg.content);
            let tokens = approx_tokens(&full_content);

            if tokens <= self.max_tokens {
                // Fits in one chunk
                chunks.push(Chunk {
                    content: full_content,
                    chunk_index,
                    token_count: tokens,
                    metadata: ChunkMetadata {
                        source_path: source_path.clone(),
                        section_title: Some(format!("{} message", msg.role)),
                        chunk_type: ChunkType::ConversationTurn,
                    },
                });
                chunk_index += 1;
            } else {
                // Split long message by paragraphs with overlap
                let sub_chunks = self.split_long_text(
                    &full_content,
                    &source_path,
                    &mut chunk_index,
                    ChunkType::ConversationTurn,
                );
                chunks.extend(sub_chunks);
            }
        }

        // Second pass: if session produced no chunks, do sliding window on full transcript
        if chunks.is_empty() && !session.messages.is_empty() {
            let full_transcript = session
                .messages
                .iter()
                .map(|m| format!("[{}]: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n\n");

            chunks = self.sliding_window(
                &full_transcript,
                &source_path,
                &mut chunk_index,
                ChunkType::ConversationWindow,
            );
        }

        chunks
    }

    /// Chunk a Codex session into semantic chunks
    pub fn chunk_codex_session(&self, session: &CodexSession) -> Vec<Chunk> {
        let source_path = format!("codex://{}", session.session_id);
        let mut chunks = Vec::new();
        let mut chunk_index = 0;

        for msg in &session.messages {
            let cwd_hint = session
                .cwd
                .as_deref()
                .map(|c| format!(" (cwd: {})", c))
                .unwrap_or_default();
            let header = format!("[{}{}]\n", msg.role, cwd_hint);
            let full_content = format!("{}{}", header, msg.content);
            let tokens = approx_tokens(&full_content);

            if tokens <= self.max_tokens {
                chunks.push(Chunk {
                    content: full_content,
                    chunk_index,
                    token_count: tokens,
                    metadata: ChunkMetadata {
                        source_path: source_path.clone(),
                        section_title: Some(format!("{} message", msg.role)),
                        chunk_type: ChunkType::ConversationTurn,
                    },
                });
                chunk_index += 1;
            } else {
                let sub = self.split_long_text(
                    &full_content,
                    &source_path,
                    &mut chunk_index,
                    ChunkType::ConversationTurn,
                );
                chunks.extend(sub);
            }
        }

        chunks
    }

    /// Chunk a plain text document (markdown, txt, code, etc.)
    pub fn chunk_document(&self, text: &str, source_path: &str) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut chunk_index = 0;

        // Split by markdown headings first
        let sections = split_by_headings(text);

        if sections.len() > 1 {
            for (title, content) in sections {
                let section_text = if let Some(t) = &title {
                    format!("## {}\n\n{}", t, content)
                } else {
                    content.to_string()
                };
                let section_tokens = approx_tokens(&section_text);

                if section_tokens <= self.max_tokens {
                    push_chunk(
                        &mut chunks,
                        section_text,
                        &mut chunk_index,
                        source_path,
                        title,
                        ChunkType::DocumentSection,
                    );
                } else {
                    let sub = self.split_long_text(
                        &section_text,
                        source_path,
                        &mut chunk_index,
                        ChunkType::DocumentSection,
                    );
                    chunks.extend(sub);
                }
            }
        } else {
            // No headings — use sliding window
            chunks = self.sliding_window(text, source_path, &mut chunk_index, ChunkType::Paragraph);
        }

        chunks
    }

    /// Split long text using paragraphs, then sliding window as fallback
    fn split_long_text(
        &self,
        text: &str,
        source_path: &str,
        chunk_index: &mut usize,
        chunk_type: ChunkType,
    ) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_len = 0usize;

        for para in text.split("\n\n").filter(|p| !p.trim().is_empty()) {
            let para_tokens = approx_tokens(para);

            // If a single paragraph is too big, use sliding window on it
            if para_tokens > self.max_tokens {
                if !current.is_empty() {
                    push_chunk(
                        &mut chunks,
                        std::mem::take(&mut current),
                        chunk_index,
                        source_path,
                        None,
                        chunk_type.clone(),
                    );
                    current_len = 0;
                }
                let sub = self.sliding_window(para, source_path, chunk_index, chunk_type.clone());
                chunks.extend(sub);
                continue;
            }

            let projected_len = if current.is_empty() {
                para.len()
            } else {
                current_len + 2 + para.len()
            };
            let projected_tokens = projected_len.div_ceil(4);

            if projected_tokens <= self.max_tokens {
                if !current.is_empty() {
                    current.push_str("\n\n");
                }
                current.push_str(para);
                current_len = projected_len;
            } else {
                if !current.is_empty() {
                    push_chunk(
                        &mut chunks,
                        std::mem::take(&mut current),
                        chunk_index,
                        source_path,
                        None,
                        chunk_type.clone(),
                    );
                }
                current.push_str(para);
                current_len = para.len();
            }
        }

        if !current.is_empty() {
            push_chunk(
                &mut chunks,
                current,
                chunk_index,
                source_path,
                None,
                chunk_type,
            );
        }

        chunks
    }

    /// Sliding window chunker: chunk_size tokens with overlap
    fn sliding_window(
        &self,
        text: &str,
        source_path: &str,
        chunk_index: &mut usize,
        chunk_type: ChunkType,
    ) -> Vec<Chunk> {
        if text.split_whitespace().next().is_none() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let step = self.max_tokens.saturating_sub(self.overlap_tokens).max(1);
        let chars_per_token = 4usize;
        let window_chars = self.max_tokens * chars_per_token;
        let step_chars = step * chars_per_token;

        let mut start = 0usize;
        while start < text.len() {
            start = ceil_char_boundary(text, start);
            let end = floor_char_boundary(text, (start + window_chars).min(text.len()));

            // Find a good split point (end of sentence/word)
            let split_at = if end >= text.len() {
                text.len()
            } else {
                text[start..end]
                    .rfind(['.', '\n', '!', '?'])
                    .map(|pos| start + pos + 1)
                    .unwrap_or(end)
            };

            let chunk_text = text[start..split_at].trim().to_string();
            if !chunk_text.is_empty() {
                let tokens = approx_tokens(&chunk_text);
                chunks.push(Chunk {
                    content: chunk_text,
                    chunk_index: *chunk_index,
                    token_count: tokens,
                    metadata: ChunkMetadata {
                        source_path: source_path.to_string(),
                        section_title: None,
                        chunk_type: chunk_type.clone(),
                    },
                });
                *chunk_index += 1;
            }

            if split_at >= text.len() {
                break;
            }

            // Move forward by step (minus overlap)
            let next_start = ceil_char_boundary(text, (start + step_chars).min(text.len()));
            if next_start >= split_at {
                start = split_at;
            } else {
                start = next_start;
            }
        }

        chunks
    }
}

/// Split text by markdown headings
/// Returns Vec of (Option<section_title>, section_content)
fn split_by_headings(text: &str) -> Vec<(Option<String>, &str)> {
    let mut sections: Vec<(Option<String>, &str)> = Vec::new();
    let mut section_start = 0;
    let mut last_title: Option<String> = None;
    let mut offset = 0usize;

    for line in text.split_inclusive('\n') {
        let heading_line = line.trim_end_matches('\n');
        if heading_line.starts_with('#') {
            let section = &text[section_start..offset];
            if !section.trim().is_empty() {
                sections.push((last_title.clone(), section));
            }

            last_title = Some(heading_line.trim_start_matches('#').trim().to_string());
            section_start = offset + line.len();
        }

        offset += line.len();
    }

    // Add last section
    let remaining = &text[section_start..];
    if !remaining.trim().is_empty() {
        sections.push((last_title, remaining));
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::claude::{ClaudeMessage, ClaudeSession};
    use crate::parser::codex::{CodexMessage, CodexSession};

    fn make_chunker() -> SmartChunker {
        SmartChunker::new(512, 64)
    }

    #[test]
    fn test_chunk_claude_session_basic() {
        let session = ClaudeSession {
            session_id: "test-123".to_string(),
            project_name: "TestProject".to_string(),
            project_path: "/tmp/test".to_string(),
            cwd: Some("/tmp/test".to_string()),
            git_branch: Some("main".to_string()),
            messages: vec![
                ClaudeMessage {
                    role: "user".to_string(),
                    content: "How do I create a Rust project?".to_string(),
                    message_id: None,
                    timestamp: None,
                },
                ClaudeMessage {
                    role: "assistant".to_string(),
                    content: "Use `cargo new my-project` to create a new Rust project.".to_string(),
                    message_id: None,
                    timestamp: None,
                },
            ],
        };

        let chunker = make_chunker();
        let chunks = chunker.chunk_claude_session(&session);

        assert!(!chunks.is_empty());
        assert!(chunks[0].content.contains("user"));
        assert!(chunks.iter().any(|c| c.content.contains("cargo new")));
    }

    #[test]
    fn test_chunk_session_long_message() {
        let long_content = "word ".repeat(3000); // ~15000 chars, way over 512 tokens
        let session = ClaudeSession {
            session_id: "long-session".to_string(),
            project_name: "LongProject".to_string(),
            project_path: "/tmp".to_string(),
            cwd: None,
            git_branch: None,
            messages: vec![ClaudeMessage {
                role: "assistant".to_string(),
                content: long_content,
                message_id: None,
                timestamp: None,
            }],
        };

        let chunker = make_chunker();
        let chunks = chunker.chunk_claude_session(&session);

        // Should produce multiple chunks
        assert!(chunks.len() > 1);
        // Each chunk should be within token limit (with some tolerance)
        for chunk in &chunks {
            assert!(
                chunk.token_count <= 512 + 20,
                "chunk too large: {}",
                chunk.token_count
            );
        }
    }

    #[test]
    fn test_chunk_codex_session() {
        let session = CodexSession {
            session_id: "codex-001".to_string(),
            cwd: Some("/home/user/project".to_string()),
            timestamp: None,
            messages: vec![
                CodexMessage {
                    role: "user".to_string(),
                    content: "Implement a linked list in Rust".to_string(),
                    timestamp: None,
                },
                CodexMessage {
                    role: "assistant".to_string(),
                    content: "Here's a simple linked list implementation in Rust:\n```rust\nenum List {\n    Cons(i32, Box<List>),\n    Nil,\n}\n```".to_string(),
                    timestamp: None,
                },
            ],
        };

        let chunker = make_chunker();
        let chunks = chunker.chunk_codex_session(&session);

        assert!(!chunks.is_empty());
        assert!(chunks.iter().any(|c| c.content.contains("linked list")));
    }

    #[test]
    fn test_chunk_document_with_headings() {
        let doc = "# Introduction\n\nThis is the intro section.\n\n## Section Two\n\nThis is section two content.\n\n## Section Three\n\nAnd this is section three.";
        let chunker = make_chunker();
        let chunks = chunker.chunk_document(doc, "/tmp/doc.md");

        assert!(chunks.len() >= 2);
        assert!(chunks.iter().any(|c| c.content.contains("intro")));
        assert!(chunks.iter().any(|c| c.content.contains("Section Two")));
    }

    #[test]
    fn test_chunk_document_no_headings() {
        let doc = "First paragraph about Rust programming.\n\nSecond paragraph about memory safety.\n\nThird paragraph about performance.";
        let chunker = make_chunker();
        let chunks = chunker.chunk_document(doc, "/tmp/plain.txt");

        assert!(!chunks.is_empty());
        assert!(chunks.iter().any(|c| c.content.contains("Rust")));
    }

    #[test]
    fn test_approx_tokens() {
        assert_eq!(approx_tokens("hello"), 2); // 5 chars / 4 = 1.25 → 2
        assert_eq!(approx_tokens(""), 0);
        assert_eq!(approx_tokens("a".repeat(100).as_str()), 25);
    }

    #[test]
    fn test_chunk_indices_are_sequential() {
        let doc = "# Sec A\n\nContent A.\n\n# Sec B\n\nContent B.\n\n# Sec C\n\nContent C.";
        let chunker = make_chunker();
        let chunks = chunker.chunk_document(doc, "/tmp/test.md");

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i, "chunk index should be sequential");
        }
    }

    #[test]
    fn test_sliding_window_handles_unicode_boundaries() {
        let doc = "Fernanda T\u{327}orres Bossa Invest\n\n".repeat(200);
        let chunker = SmartChunker::new(16, 4);
        let chunks = chunker.chunk_document(&doc, "/tmp/unicode.md");

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|chunk| !chunk.content.is_empty()));
    }
}
