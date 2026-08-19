use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FileFingerprint {
    pub(super) path: String,
    pub(super) size: u64,
    pub(super) modified_unix_ms: u128,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SourceManifest {
    source_type: String,
    source_key: String,
    files: Vec<FileFingerprint>,
}

#[derive(Debug, Default)]
struct ManifestDiff {
    added: Vec<FileFingerprint>,
    modified: Vec<FileFingerprint>,
    removed: Vec<FileFingerprint>,
    unchanged: usize,
}

fn fingerprint_file(path: &Path) -> Result<FileFingerprint> {
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
    let modified_unix_ms = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    Ok(FileFingerprint {
        path: path.to_string_lossy().to_string(),
        size: metadata.len(),
        modified_unix_ms,
    })
}

fn is_text_like(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(name, ".DS_Store" | ".vault_index.db") {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    matches!(
        ext.as_str(),
        "md" | "markdown"
            | "pdf"
            | "txt"
            | "canvas"
            | "json"
            | "jsonl"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "rs"
            | "py"
            | "sh"
            | "zsh"
            | "bash"
            | "yaml"
            | "yml"
            | "toml"
            | "csv"
            | "sql"
            | "html"
            | "css"
            | "xml"
            | "env"
            | "local"
            | "mdx"
            | "conf"
    ) || name.starts_with(".env")
}

pub(super) fn should_skip_indexing_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".obsidian"
            | ".entire"
            | "node_modules"
            | "target"
            | ".next"
            | "dist"
            | "build"
            | "coverage"
            | ".venv"
            | "venv"
            | "vendor"
    )
}

fn diff_manifests(previous: &SourceManifest, current: &SourceManifest) -> ManifestDiff {
    let previous_map: HashMap<&str, &FileFingerprint> = previous
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let current_map: HashMap<&str, &FileFingerprint> = current
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();

    let mut diff = ManifestDiff::default();

    for file in &current.files {
        match previous_map.get(file.path.as_str()) {
            None => diff.added.push(file.clone()),
            Some(previous_file) if **previous_file != *file => diff.modified.push(file.clone()),
            Some(_) => diff.unchanged += 1,
        }
    }

    for file in &previous.files {
        if !current_map.contains_key(file.path.as_str()) {
            diff.removed.push(file.clone());
        }
    }

    diff
}

pub(super) fn source_type_from_str(source: &str) -> Result<SourceType> {
    match source {
        "claude" => Ok(SourceType::Claude),
        "codex" => Ok(SourceType::Codex),
        "file" => Ok(SourceType::File),
        "folder" => Ok(SourceType::Folder),
        "obsidian" => Ok(SourceType::Obsidian),
        other => Err(anyhow::anyhow!("Unknown source type: {other}")),
    }
}

pub(super) fn source_key(source_type: &SourceType, path: Option<&str>) -> Result<String> {
    match source_type {
        SourceType::Claude => Ok("claude".to_string()),
        SourceType::Codex => Ok("codex".to_string()),
        SourceType::File | SourceType::Folder | SourceType::Obsidian => path
            .map(|value| normalize_path(value).to_string_lossy().to_string())
            .ok_or_else(|| anyhow::anyhow!("path required for {}", source_type)),
        SourceType::Url => Err(anyhow::anyhow!("URL sources are not implemented yet")),
    }
}

pub(super) fn chunk_belongs_to_source(
    chunk: &StoredChunk,
    source_type: &SourceType,
    source_key: &str,
) -> bool {
    match source_type {
        SourceType::Claude => chunk.source_path.starts_with("claude://"),
        SourceType::Codex => chunk.source_path.starts_with("codex://"),
        SourceType::File => chunk.source_path == source_key,
        SourceType::Folder | SourceType::Obsidian => {
            local_path_is_within(&chunk.source_path, source_key)
        }
        SourceType::Url => false,
    }
}

#[cfg(unix)]
fn local_path_is_within(candidate: &str, root: &str) -> bool {
    Path::new(candidate).starts_with(Path::new(root))
}

#[cfg(windows)]
fn local_path_is_within(candidate: &str, root: &str) -> bool {
    fn key(value: &str) -> String {
        let mut normalized = value.replace('\\', "/").to_ascii_lowercase();
        while normalized.len() > 1 && normalized.ends_with('/') {
            normalized.pop();
        }
        normalized
    }

    let candidate = key(candidate);
    let root = key(root);
    candidate == root
        || (root == "/" && candidate.starts_with('/'))
        || candidate
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

pub(super) fn source_record_matches(
    source: &SourceRecord,
    source_type: &SourceType,
    source_key: &str,
) -> bool {
    source.source_type == *source_type && source.path == source_key
}

fn count_source_chunks(state: &EngineState, source_type: &SourceType, source_key: &str) -> usize {
    state
        .chunks
        .iter()
        .filter(|chunk| chunk_belongs_to_source(chunk, source_type, source_key))
        .count()
}

pub(super) fn upsert_source_record(
    state: &mut EngineState,
    source_type: &SourceType,
    source_key: &str,
) {
    state
        .sources
        .retain(|source| !source_record_matches(source, source_type, source_key));

    let chunk_count = count_source_chunks(state, source_type, source_key);
    if chunk_count == 0 {
        return;
    }

    state.sources.push(SourceRecord {
        path: source_key.to_string(),
        source_type: source_type.clone(),
        ingested_at: SystemTime::now(),
        chunk_count,
        token_count: state.next_token_id,
    });
}

impl MementoManager {
    fn manifest_path(&self, source_type: &SourceType, source_key: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        source_type.to_string().hash(&mut hasher);
        source_key.hash(&mut hasher);
        let digest = hasher.finish();
        self.manifests_dir()
            .join(format!("{}-{digest:016x}.json", source_type))
    }

    fn load_manifest(&self, source_type: &SourceType, source_key: &str) -> Result<SourceManifest> {
        let path = self.manifest_path(source_type, source_key);
        if !path.exists() {
            return Ok(SourceManifest::default());
        }

        let content = fs::read_to_string(path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    fn save_manifest(
        &self,
        source_type: &SourceType,
        source_key: &str,
        manifest: &SourceManifest,
    ) -> Result<()> {
        fs::create_dir_all(self.manifests_dir())?;
        let path = self.manifest_path(source_type, source_key);
        let content = serde_json::to_string_pretty(manifest)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn build_manifest(&self, source_type: &SourceType, source_key: &str) -> Result<SourceManifest> {
        let files = match source_type {
            SourceType::File => vec![fingerprint_file(Path::new(source_key))?],
            SourceType::Folder => self.collect_folder_files(source_key)?,
            SourceType::Obsidian => self.collect_obsidian_files(source_key)?,
            _ => Vec::new(),
        };

        Ok(SourceManifest {
            source_type: source_type.to_string(),
            source_key: source_key.to_string(),
            files,
        })
    }

    pub(super) fn collect_folder_files(&self, source_key: &str) -> Result<Vec<FileFingerprint>> {
        let ignore_rules = IgnoreRules::load(Path::new(source_key))?;
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(source_key)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let name = entry.file_name().to_string_lossy();
                if should_skip_indexing_dir(&name) {
                    return false;
                }
                !ignore_rules.is_ignored(entry.path(), entry.file_type().is_dir())
            })
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            if !ignore_rules.is_ignored(entry.path(), false) && is_text_like(entry.path()) {
                let fingerprint = fingerprint_file(entry.path())?;
                if fingerprint.size > 0 {
                    files.push(fingerprint);
                }
            }
        }
        Ok(files)
    }

    pub(super) fn collect_obsidian_files(&self, source_key: &str) -> Result<Vec<FileFingerprint>> {
        let ignore_rules = IgnoreRules::load(Path::new(source_key))?;
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(source_key)
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let name = entry.file_name().to_string_lossy();
                if should_skip_indexing_dir(&name) {
                    return false;
                }
                !ignore_rules.is_ignored(entry.path(), entry.file_type().is_dir())
            })
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            if !ignore_rules.is_ignored(entry.path(), false) && is_text_like(entry.path()) {
                let fingerprint = fingerprint_file(entry.path())?;
                if fingerprint.size > 0 {
                    files.push(fingerprint);
                }
            }
        }
        Ok(files)
    }

    pub(super) async fn load_chunks_for_files(
        &self,
        source_type: &SourceType,
        source_key: &str,
        files: &[FileFingerprint],
    ) -> Result<Vec<PreparedDocument>> {
        match source_type {
            SourceType::File => self.import_file(source_key).await,
            SourceType::Folder | SourceType::Obsidian => {
                let documents: Vec<PreparedDocument> = files
                    .par_iter()
                    .filter_map(|file| {
                        let parser = libmemento::parser::document::DocumentParser::new();
                        let (max_tokens, overlap_tokens) = chunker_profile_for_file_size(file.size);
                        let chunker = SmartChunker::new(max_tokens, overlap_tokens);
                        let path = Path::new(&file.path);
                        match parser.parse_file(path) {
                            Ok(text) if !text.trim().is_empty() => {
                                let chunks = chunker.chunk_document(&text, &file.path);
                                (!chunks.is_empty()).then(|| {
                                    prepare_document_from_grouped_chunks(file.path.clone(), chunks)
                                })
                            }
                            _ => None,
                        }
                    })
                    .collect();
                Ok(documents)
            }
            _ => Err(anyhow::anyhow!(
                "incremental chunk loading is only supported for file, folder, and obsidian sources"
            )),
        }
    }

    pub(super) async fn sync_local_source_incremental(
        &self,
        req: &ImportRequest,
        source_type: &SourceType,
        source_path: &str,
    ) -> Result<SyncResponse> {
        let _operation_guard = self.operation_lock.lock().await;
        let previous_manifest = self.load_manifest(source_type, source_path)?;
        let current_manifest = self.build_manifest(source_type, source_path)?;
        let diff = diff_manifests(&previous_manifest, &current_manifest);

        if diff.added.is_empty()
            && diff.modified.is_empty()
            && diff.removed.is_empty()
            && !previous_manifest.files.is_empty()
        {
            let status = self.status().await;
            return Ok(SyncResponse {
                chunks_synced: 0,
                source_type: req.source.clone(),
                removed_chunks: 0,
                removed_sources: 0,
                added_files: 0,
                updated_files: 0,
                removed_files: 0,
                unchanged_files: diff.unchanged,
                coherence_after: status.coherence_score,
                eigenvectors_computed: 0,
            });
        }

        let mut changed_files: Vec<FileFingerprint> = diff
            .added
            .iter()
            .chain(diff.modified.iter())
            .cloned()
            .collect();
        changed_files.sort_by_key(|file| (file.size, file.path.clone()));
        let changed_paths: HashSet<String> = diff
            .modified
            .iter()
            .chain(diff.removed.iter())
            .map(|file| file.path.clone())
            .collect();
        let total_batches = if changed_files.is_empty() {
            0
        } else {
            changed_files.len().div_ceil(INCREMENTAL_SYNC_BATCH_SIZE)
        };

        let mut checkpoint =
            OperationCheckpoint::new("sync", source_type.to_string(), source_path.to_string());
        checkpoint.phase = "planning".to_string();
        checkpoint.total_files =
            (diff.added.len() + diff.modified.len() + diff.removed.len()) as u64;
        checkpoint.total_batches = total_batches as u64;
        checkpoint.added_files = diff.added.len() as u64;
        checkpoint.updated_files = diff.modified.len() as u64;
        checkpoint.removed_files = diff.removed.len() as u64;
        checkpoint.touch();
        self.save_active_operation(&checkpoint)?;

        let sync_result = async {
            checkpoint.phase = "rebuilding-state".to_string();
            checkpoint.touch();
            self.save_active_operation(&checkpoint)?;

            let (removed_chunks, removed_sources) = {
                let mut state = self.state.write().await;
                let before_chunks = state.chunks.len();
                let before_sources = state.sources.len();

                state.chunks.retain(|chunk| {
                    !(chunk_belongs_to_source(chunk, source_type, source_path)
                        && changed_paths.contains(&chunk.source_path))
                });
                state.documents.retain(|document| {
                    !(document.source_path == source_path
                        || changed_paths.contains(&document.source_path))
                });
                state
                    .sources
                    .retain(|source| !source_record_matches(source, source_type, source_path));

                let removed_chunks = before_chunks.saturating_sub(state.chunks.len());
                let removed_sources = before_sources.saturating_sub(state.sources.len());

                rebuild_state_from_chunks(&mut state);
                upsert_source_record(&mut state, source_type, source_path);
                (removed_chunks, removed_sources)
            };

            checkpoint.phase = "checkpointing".to_string();
            checkpoint.processed_files = diff.removed.len() as u64;
            checkpoint.touch();
            self.save_checkpoint().await?;
            self.save_active_operation(&checkpoint)?;

            let mut synced_chunks = 0usize;
            let mut last_checkpoint_save = std::time::Instant::now();
            for (batch_idx, batch) in changed_files
                .chunks(INCREMENTAL_SYNC_BATCH_SIZE)
                .enumerate()
            {
                checkpoint.phase = "ingesting".to_string();
                checkpoint.completed_batches = batch_idx as u64;
                checkpoint.touch();
                self.save_active_operation(&checkpoint)?;

                for slice in batch.chunks(INCREMENTAL_SYNC_PROGRESS_SLICE_SIZE) {
                    if let Some(current_file) = slice.iter().max_by_key(|file| file.size) {
                        checkpoint.current_file_path = Some(current_file.path.clone());
                        checkpoint.current_file_size_bytes = Some(current_file.size);
                        checkpoint.touch();
                        self.save_active_operation(&checkpoint)?;
                    }

                    let documents = self
                        .load_chunks_for_files(source_type, source_path, slice)
                        .await?;
                    let slice_chunk_count: usize =
                        documents.iter().map(|document| document.chunks.len()).sum();

                    {
                        let mut state = self.state.write().await;
                        self.ingest_prepared_documents(&documents, &mut state);
                        upsert_source_record(&mut state, source_type, source_path);
                    }

                    synced_chunks += slice_chunk_count;
                    checkpoint.processed_files += slice.len() as u64;
                    checkpoint.chunks_written = synced_chunks as u64;
                    checkpoint.phase = "ingesting".to_string();
                    checkpoint.touch();
                    self.save_active_operation(&checkpoint)?;
                }

                checkpoint.completed_batches = (batch_idx + 1) as u64;
                checkpoint.phase = "checkpointing".to_string();
                checkpoint.touch();
                let should_save_checkpoint = checkpoint.completed_batches
                    == checkpoint.total_batches
                    || checkpoint.completed_batches == 1
                    || batch_idx
                        .checked_add(1)
                        .is_some_and(|n| n % INCREMENTAL_SYNC_CHECKPOINT_BATCH_INTERVAL == 0)
                    || last_checkpoint_save.elapsed() >= INCREMENTAL_SYNC_CHECKPOINT_MAX_AGE;
                if should_save_checkpoint {
                    self.save_checkpoint().await?;
                    last_checkpoint_save = std::time::Instant::now();
                }
                self.save_active_operation(&checkpoint)?;
            }

            checkpoint.phase = "learning".to_string();
            checkpoint.current_file_path = None;
            checkpoint.current_file_size_bytes = None;
            checkpoint.touch();
            self.save_active_operation(&checkpoint)?;
            {
                let mut state = self.state.write().await;
                state.document_graph = DocumentGraph::build(&state.documents);
            }
            self.save_manifest(source_type, source_path, &current_manifest)?;
            let learn = self.learn_with_cap(Some(8)).await?;

            Ok(SyncResponse {
                chunks_synced: synced_chunks,
                source_type: req.source.clone(),
                removed_chunks,
                removed_sources,
                added_files: diff.added.len(),
                updated_files: diff.modified.len(),
                removed_files: diff.removed.len(),
                unchanged_files: diff.unchanged,
                coherence_after: learn.coherence_after,
                eigenvectors_computed: learn.eigenvectors_computed,
            })
        }
        .await;

        match sync_result {
            Ok(response) => {
                let _ = self.clear_active_operation();
                Ok(response)
            }
            Err(error) => {
                checkpoint.phase = "failed".to_string();
                checkpoint.status = "failed".to_string();
                checkpoint.touch();
                let _ = self.save_active_operation(&checkpoint);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::local_path_is_within;

    #[test]
    fn local_source_membership_respects_path_boundaries() {
        #[cfg(unix)]
        {
            assert!(local_path_is_within("/vault/nested/note.md", "/vault"));
            assert!(!local_path_is_within("/vault-copy/note.md", "/vault"));
        }

        #[cfg(windows)]
        {
            assert!(local_path_is_within(
                r"C:\Users\Example\Vault\nested\note.md",
                r"c:/users/example/vault/"
            ));
            assert!(!local_path_is_within(
                r"C:\Users\Example\Vault-copy\note.md",
                r"C:\Users\Example\Vault"
            ));
        }
    }
}
