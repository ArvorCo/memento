pub mod smart;

/// A single chunk of content ready for embedding and storage
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub chunk_index: usize,
    pub token_count: usize,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone)]
pub struct ChunkMetadata {
    pub source_path: String,
    pub section_title: Option<String>,
    pub chunk_type: ChunkType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChunkType {
    ConversationTurn,
    ConversationWindow,
    DocumentSection,
    CodeBlock,
    Paragraph,
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkType::ConversationTurn => "conversation_turn",
            ChunkType::ConversationWindow => "conversation_window",
            ChunkType::DocumentSection => "document_section",
            ChunkType::CodeBlock => "code_block",
            ChunkType::Paragraph => "paragraph",
        }
    }
}
