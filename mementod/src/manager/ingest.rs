use super::*;

#[derive(Debug, Clone)]
pub(super) struct PreparedDocument {
    pub(super) source_path: String,
    pub(super) canonical_text: String,
    pub(super) title: Option<String>,
    pub(super) chunks: Vec<PreparedChunk>,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedChunk {
    pub(super) content: String,
    pub(super) chunk_index: usize,
    pub(super) token_count: usize,
    pub(super) section_title: Option<String>,
    pub(super) chunk_type: String,
    pub(super) span: Option<TextSpan>,
}

pub(super) fn chunker_profile_for_file_size(size: u64) -> (usize, usize) {
    if size >= EXTRA_LARGE_FILE_THRESHOLD_BYTES {
        (
            EXTRA_LARGE_FILE_CHUNK_MAX_TOKENS,
            EXTRA_LARGE_FILE_CHUNK_OVERLAP_TOKENS,
        )
    } else if size >= LARGE_FILE_THRESHOLD_BYTES {
        (LARGE_FILE_CHUNK_MAX_TOKENS, LARGE_FILE_CHUNK_OVERLAP_TOKENS)
    } else if size >= MEDIUM_FILE_THRESHOLD_BYTES {
        (
            MEDIUM_FILE_CHUNK_MAX_TOKENS,
            MEDIUM_FILE_CHUNK_OVERLAP_TOKENS,
        )
    } else {
        (DEFAULT_CHUNK_MAX_TOKENS, DEFAULT_CHUNK_OVERLAP_TOKENS)
    }
}

pub(super) fn prepare_documents_from_chunks(chunks: Vec<Chunk>) -> Vec<PreparedDocument> {
    let mut grouped: BTreeMap<String, Vec<Chunk>> = BTreeMap::new();
    for chunk in chunks {
        grouped
            .entry(chunk.metadata.source_path.clone())
            .or_default()
            .push(chunk);
    }

    grouped
        .into_iter()
        .map(|(source_path, grouped_chunks)| {
            prepare_document_from_grouped_chunks(source_path, grouped_chunks)
        })
        .collect()
}

pub(super) fn prepare_document_from_grouped_chunks(
    source_path: String,
    mut grouped_chunks: Vec<Chunk>,
) -> PreparedDocument {
    grouped_chunks.sort_by_key(|chunk| chunk.chunk_index);

    let inline_chunk_content = grouped_chunks
        .iter()
        .map(|chunk| chunk.content.len() as u64)
        .sum::<u64>()
        >= EXTRA_LARGE_FILE_THRESHOLD_BYTES;
    let mut canonical_text = String::new();
    let mut prepared_chunks = Vec::with_capacity(grouped_chunks.len());
    let mut title = None;

    for chunk in grouped_chunks {
        if title.is_none() {
            title = chunk.metadata.section_title.clone();
        }

        let span = if inline_chunk_content {
            None
        } else {
            if !canonical_text.is_empty() {
                canonical_text.push_str("\n\n");
            }

            let start = canonical_text.len();
            canonical_text.push_str(&chunk.content);
            let end = canonical_text.len();
            Some(TextSpan { start, end })
        };

        prepared_chunks.push(PreparedChunk {
            content: chunk.content,
            chunk_index: chunk.chunk_index,
            token_count: chunk.token_count,
            section_title: chunk.metadata.section_title,
            chunk_type: chunk.metadata.chunk_type.as_str().to_string(),
            span,
        });
    }

    PreparedDocument {
        source_path,
        canonical_text,
        title,
        chunks: prepared_chunks,
    }
}

impl MementoManager {
    async fn import_claude(&self) -> Result<Vec<Chunk>> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home dir"))?;
        let claude_dir = home.join(".claude").join("projects");
        if !claude_dir.exists() {
            return Err(anyhow::anyhow!(
                "Claude sessions not found at {}",
                claude_dir.display()
            ));
        }

        let sessions = libmemento::parser::claude::parse_all_sessions(&claude_dir)?;
        let chunker = SmartChunker::new(512, 64);
        let mut all_chunks = Vec::new();
        for session in &sessions {
            all_chunks.extend(chunker.chunk_claude_session(session));
        }
        Ok(all_chunks)
    }

    async fn import_codex(&self) -> Result<Vec<Chunk>> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home dir"))?;
        let codex_dir = home.join(".codex").join("sessions");
        if !codex_dir.exists() {
            return Err(anyhow::anyhow!(
                "Codex sessions not found at {}",
                codex_dir.display()
            ));
        }

        let sessions = libmemento::parser::codex::parse_all_sessions(&codex_dir)?;
        let chunker = SmartChunker::new(512, 64);
        let mut all_chunks = Vec::new();
        for session in &sessions {
            all_chunks.extend(chunker.chunk_codex_session(session));
        }
        Ok(all_chunks)
    }

    pub(super) async fn import_file(&self, path: &str) -> Result<Vec<PreparedDocument>> {
        let normalized_path = normalize_path(path);
        let parser = libmemento::parser::document::DocumentParser::new();
        let text = parser.parse_file(&normalized_path)?;
        let size = fs::metadata(&normalized_path)
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        let (max_tokens, overlap_tokens) = chunker_profile_for_file_size(size);
        let chunker = SmartChunker::new(max_tokens, overlap_tokens);
        Ok(prepare_documents_from_chunks(
            chunker.chunk_document(&text, &normalized_path.to_string_lossy()),
        ))
    }

    async fn import_folder(&self, path: &str) -> Result<Vec<PreparedDocument>> {
        let normalized_root = normalize_path(path).to_string_lossy().to_string();
        let files = self.collect_folder_files(&normalized_root)?;
        self.load_chunks_for_files(&SourceType::Folder, &normalized_root, &files)
            .await
    }

    async fn import_obsidian(&self, path: &str) -> Result<Vec<PreparedDocument>> {
        let normalized_root = normalize_path(path).to_string_lossy().to_string();
        let files = self.collect_obsidian_files(&normalized_root)?;
        self.load_chunks_for_files(&SourceType::Obsidian, &normalized_root, &files)
            .await
    }

    pub(super) async fn load_documents(
        &self,
        source_type: &SourceType,
        path: Option<&str>,
    ) -> Result<Vec<PreparedDocument>> {
        match source_type {
            SourceType::Claude => Ok(prepare_documents_from_chunks(self.import_claude().await?)),
            SourceType::Codex => Ok(prepare_documents_from_chunks(self.import_codex().await?)),
            SourceType::File => {
                let path = path.ok_or_else(|| anyhow::anyhow!("path required for file import"))?;
                self.import_file(path).await
            }
            SourceType::Folder => {
                let path =
                    path.ok_or_else(|| anyhow::anyhow!("path required for folder import"))?;
                self.import_folder(path).await
            }
            SourceType::Obsidian => {
                let path =
                    path.ok_or_else(|| anyhow::anyhow!("path required for obsidian sync"))?;
                self.import_obsidian(path).await
            }
            SourceType::Url => Err(anyhow::anyhow!("URL import is not implemented yet")),
        }
    }

    pub(super) fn ingest_prepared_documents(
        &self,
        documents: &[PreparedDocument],
        state: &mut EngineState,
    ) {
        let original_threshold = state.matrix.consolidation_threshold();
        state.matrix.set_consolidation_threshold(usize::MAX / 4);
        state.chunk_embeddings.clear();
        state.document_embeddings.clear();
        for document in documents {
            let doc_id = state.next_doc_id;
            state.next_doc_id += 1;
            state.documents.push(StoredDocument {
                doc_id,
                source_path: document.source_path.clone(),
                canonical_text: document.canonical_text.clone(),
                title: document.title.clone(),
            });

            let mut document_metadata_terms = HashSet::new();
            let mut document_token_ids = HashSet::new();
            let mut document_source_text = document.source_path.clone();
            if let Some(title) = &document.title {
                document_source_text.push(' ');
                document_source_text.push_str(title);
            }

            for chunk in &document.chunks {
                let token_ids = self.tokenize_chunk(&chunk.content, state);
                if token_ids.len() >= 2 {
                    let _ = state.matrix.ingest_document(&token_ids);
                }

                let stored_chunk = StoredChunk {
                    chunk_id: state.next_chunk_id,
                    doc_id,
                    span: chunk.span,
                    content: if chunk.span.is_some() {
                        String::new()
                    } else {
                        chunk.content.clone()
                    },
                    chunk_index: chunk.chunk_index,
                    token_count: chunk.token_count,
                    source_path: document.source_path.clone(),
                    section_title: chunk.section_title.clone(),
                    chunk_type: chunk.chunk_type.clone(),
                    token_ids,
                };
                let metadata_terms = lexical_metadata_terms(&stored_chunk, state.documents.last());
                document_metadata_terms.extend(metadata_terms.iter().cloned());
                document_token_ids.extend(stored_chunk.token_ids.iter().copied());
                let chunk_idx = state.chunks.len();
                state.lexical_index.append_chunk(
                    chunk_idx,
                    doc_id,
                    &stored_chunk.token_ids,
                    &metadata_terms,
                    &stored_chunk.source_path,
                );
                state.chunks.push(stored_chunk);
                state.next_chunk_id += 1;
            }
            state.lexical_index.finish_document(
                doc_id,
                &document_token_ids,
                &document_metadata_terms,
                &document_source_text,
            );
        }
        state.matrix.set_consolidation_threshold(original_threshold);
    }

    fn tokenize_chunk(&self, content: &str, state: &mut EngineState) -> Vec<usize> {
        tokenize_text(content)
            .into_iter()
            .map(|token| {
                *state.vocabulary.entry(token).or_insert_with(|| {
                    let token_id = state.next_token_id;
                    state.next_token_id += 1;
                    token_id
                })
            })
            .collect()
    }
}
