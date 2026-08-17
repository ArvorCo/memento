use super::*;

fn empty_engine_state(classification_rules: ClassificationRulesConfig) -> EngineState {
    EngineState {
        matrix: SemanticMatrix::new(10000),
        vocabulary: HashMap::new(),
        next_token_id: 0,
        documents: Vec::new(),
        next_doc_id: 0,
        next_chunk_id: 0,
        chunks: Vec::new(),
        lexical_index: LexicalIndex::default(),
        document_graph: DocumentGraph::default(),
        chunk_embeddings: HashMap::new(),
        document_embeddings: HashMap::new(),
        sources: Vec::new(),
        domain: "default".to_string(),
        coherence_score: 0.0,
        classification_rules,
    }
}

impl MementoManager {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let manifest_store = ManifestStore::init(data_dir)?;
        let classification_rules = load_or_bootstrap_classification_rules(data_dir)?;
        let operation_tracker = OperationTracker::new(data_dir);
        let recovery_store = RecoverySnapshotStore::new(data_dir);
        let has_active_operation = operation_tracker.load()?.is_some();
        let memento_path = data_dir.join("default.memento");
        let state = if has_active_operation {
            if let Some(snapshot) = recovery_store.load()? {
                engine_state_from_recovery_snapshot(snapshot, classification_rules.clone())
            } else if memento_path.exists() {
                let mut file = fs::File::open(&memento_path)?;
                let mf = MementoFile::load(&mut file)?;
                let mut state = engine_state_from_memento(mf);
                state.classification_rules = classification_rules.clone();
                state
            } else {
                empty_engine_state(classification_rules.clone())
            }
        } else if let Some(manifest) = manifest_store.load_current()? {
            let lexical = manifest
                .active_segments
                .iter()
                .find(|segment| segment.kind == SegmentKind::Lexical)
                .cloned();
            let metadata = manifest
                .active_segments
                .iter()
                .find(|segment| segment.kind == SegmentKind::Metadata)
                .cloned();
            let eigen = manifest
                .active_segments
                .iter()
                .find(|segment| segment.kind == SegmentKind::Eigen)
                .cloned();
            let embedding = manifest
                .active_segments
                .iter()
                .find(|segment| segment.kind == SegmentKind::Embedding)
                .cloned();

            if let (Some(lexical), Some(metadata)) = (lexical, metadata) {
                let lexical_payload: LexicalSegmentFile = manifest_store.read_segment(&lexical)?;
                let metadata_payload: MetadataSegmentFile =
                    manifest_store.read_segment(&metadata)?;
                let eigen_payload = eigen
                    .map(|descriptor| manifest_store.read_segment(&descriptor))
                    .transpose()?;
                let embedding_payload: Option<EmbeddingSegmentFile> = embedding
                    .map(|descriptor| manifest_store.read_segment(&descriptor))
                    .transpose()?;
                let mut state = engine_state_from_memento(memento_from_runtime_segments(
                    lexical_payload,
                    metadata_payload,
                    eigen_payload,
                ));
                let (chunk_embeddings, document_embeddings) =
                    embedding_state_from_segment(&state.chunks, embedding_payload.as_ref());
                state.chunk_embeddings = chunk_embeddings;
                state.document_embeddings = document_embeddings;
                state.classification_rules = classification_rules.clone();
                state
            } else if memento_path.exists() {
                let mut file = fs::File::open(&memento_path)?;
                let mf = MementoFile::load(&mut file)?;
                let mut state = engine_state_from_memento(mf);
                state.classification_rules = classification_rules.clone();
                state
            } else {
                empty_engine_state(classification_rules.clone())
            }
        } else if memento_path.exists() {
            let mut file = fs::File::open(&memento_path)?;
            let mf = MementoFile::load(&mut file)?;
            let mut state = engine_state_from_memento(mf);
            state.classification_rules = classification_rules.clone();
            state
        } else {
            empty_engine_state(classification_rules)
        };

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            state: Arc::new(RwLock::new(state)),
            scheduler: Arc::new(RwLock::new(SchedulerSnapshot::default())),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    pub(super) fn manifests_dir(&self) -> PathBuf {
        self.data_dir.join("manifests")
    }

    fn operation_tracker(&self) -> OperationTracker {
        OperationTracker::new(&self.data_dir)
    }

    fn recovery_snapshot_store(&self) -> RecoverySnapshotStore {
        RecoverySnapshotStore::new(&self.data_dir)
    }

    pub(super) fn load_active_operation(&self) -> Option<OperationCheckpoint> {
        self.operation_tracker().load().ok().flatten()
    }

    pub(super) fn save_active_operation(&self, checkpoint: &OperationCheckpoint) -> Result<()> {
        self.operation_tracker().save(checkpoint)
    }

    pub(super) fn clear_active_operation(&self) -> Result<()> {
        self.operation_tracker().clear()
    }

    fn save_recovery_snapshot(&self, snapshot: &RecoverySnapshot) -> Result<()> {
        self.recovery_snapshot_store().save(snapshot)
    }

    fn clear_recovery_snapshot(&self) -> Result<()> {
        self.recovery_snapshot_store().clear()
    }

    pub async fn status(&self) -> StatusResponse {
        let state = self.state.read().await;
        let scheduler = self.scheduler.read().await.clone();
        let memento_path = self.data_dir.join("default.memento");
        let runtime_manifest = ManifestStore::init(&self.data_dir)
            .ok()
            .and_then(|store| store.load_current().ok().flatten());
        let active_operation = self.load_active_operation();
        StatusResponse {
            vocabulary_size: state.vocabulary.len(),
            non_zero_count: state.matrix.non_zero_count(),
            coherence_score: state.coherence_score,
            total_chunks: state.chunks.len(),
            total_sources: state.sources.len(),
            document_graph_edges: state.document_graph.edge_count(),
            domain: state.domain.clone(),
            memento_file_exists: memento_path.exists(),
            runtime_manifest_generation: runtime_manifest
                .as_ref()
                .map(|manifest| manifest.generation)
                .unwrap_or(0),
            runtime_segment_count: runtime_manifest
                .as_ref()
                .map(|manifest| manifest.active_segments.len())
                .unwrap_or(0),
            runtime_segments_ready: runtime_manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .active_segments
                        .iter()
                        .any(|segment| segment.kind == SegmentKind::Lexical)
                        && manifest
                            .active_segments
                            .iter()
                            .any(|segment| segment.kind == SegmentKind::Metadata)
                })
                .unwrap_or(false),
            runtime_graph_ready: runtime_manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .active_segments
                        .iter()
                        .any(|segment| segment.kind == SegmentKind::Graph)
                })
                .unwrap_or(false),
            runtime_embedding_ready: runtime_manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .active_segments
                        .iter()
                        .any(|segment| segment.kind == SegmentKind::Embedding)
                })
                .unwrap_or(false),
            active_operation,
            scheduler_enabled: scheduler.enabled,
            scheduled_jobs: scheduler.jobs,
        }
    }

    pub async fn set_scheduler_snapshot(&self, snapshot: SchedulerSnapshot) {
        *self.scheduler.write().await = snapshot;
    }

    pub async fn update_scheduled_job<F>(&self, name: &str, mutate: F)
    where
        F: FnOnce(&mut ScheduledJobState),
    {
        let mut scheduler = self.scheduler.write().await;
        if let Some(job) = scheduler.jobs.iter_mut().find(|job| job.name == name) {
            mutate(job);
        }
    }

    pub(super) async fn save_checkpoint(&self) -> Result<()> {
        let snapshot = {
            let state = self.state.read().await;
            RecoverySnapshot {
                domain: state.domain.clone(),
                documents: state.documents.clone(),
                next_doc_id: state.next_doc_id,
                next_chunk_id: state.next_chunk_id,
                chunks: state.chunks.clone(),
                sources: state.sources.clone(),
            }
        };
        self.save_recovery_snapshot(&snapshot)
    }

    pub(super) async fn save(&self) -> Result<()> {
        let (
            mf,
            lexical_segment,
            metadata_segment,
            graph_segment,
            embedding_segment,
            eigen_segment,
            domain,
            source_count,
            vocabulary_size,
            chunk_count,
            token_count,
        ) = {
            let state = self.state.read().await;
            let mut mf = MementoFile::from_matrix(
                &state.matrix,
                state.vocabulary.clone(),
                state.next_token_id,
                &state.domain,
            );
            mf.documents = state.documents.clone();
            mf.next_doc_id = state.next_doc_id;
            mf.next_chunk_id = state.next_chunk_id;
            mf.chunks = state.chunks.clone();
            mf.sources = state.sources.clone();
            mf.coherence_score = state.coherence_score;
            let eigen_segment = state.matrix.cached_eigen().map(|eigen| {
                mf.set_eigen(&eigen.eigenvectors, &eigen.eigenvalues);
                EigenSegmentFile {
                    eigenvectors: eigen
                        .eigenvectors
                        .column_iter()
                        .map(|column| column.iter().copied().collect())
                        .collect(),
                    eigenvalues: eigen.eigenvalues.iter().copied().collect(),
                }
            });

            let lexical_segment = LexicalSegmentFile {
                domain: state.domain.clone(),
                vocabulary: state.vocabulary.clone(),
                next_token_id: state.next_token_id,
                vocabulary_size: state.matrix.vocabulary_size(),
                triplets: mf.triplets.clone(),
                coherence_score: state.coherence_score,
                confidence_history: state.matrix.confidence_history().to_vec(),
            };
            let metadata_segment = MetadataSegmentFile {
                domain: state.domain.clone(),
                documents: state.documents.clone(),
                next_doc_id: state.next_doc_id,
                next_chunk_id: state.next_chunk_id,
                chunks: state.chunks.clone(),
                sources: state.sources.clone(),
            };
            let graph_segment = build_graph_segment(&state, &mf.triplets);
            let embedding_segment = build_embedding_segment(&state);
            let token_count: u64 = state
                .chunks
                .iter()
                .map(|chunk| chunk.token_count as u64)
                .sum();

            (
                mf,
                lexical_segment,
                metadata_segment,
                graph_segment,
                embedding_segment,
                eigen_segment,
                state.domain.clone(),
                state.sources.len() as u64,
                state.vocabulary.len() as u64,
                state.chunks.len() as u64,
                token_count,
            )
        };

        let memento_path = self.data_dir.join("default.memento");
        let tmp_path = self.data_dir.join("default.memento.tmp");
        let mut file = fs::File::create(&tmp_path)?;
        mf.save(&mut file)?;
        fs::rename(&tmp_path, &memento_path)?;

        let manifest_store = ManifestStore::init(&self.data_dir)?;
        let previous_manifest = manifest_store.load_current()?;
        let generation = manifest_store.next_generation()?;

        let superseded_by_kind = |kind: SegmentKind| -> Vec<String> {
            previous_manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .active_segments
                        .iter()
                        .filter(|segment| segment.kind == kind)
                        .map(|segment| segment.segment_id.clone())
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut active_segments = Vec::new();
        active_segments.push(SegmentDescriptor {
            segment_id: format!("legacy-default-{generation:020}"),
            generation,
            kind: SegmentKind::LegacySnapshot,
            relative_path: "default.memento".to_string(),
            format_version: 3,
            created_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            doc_count: source_count,
            chunk_count,
            token_count,
            supersedes: superseded_by_kind(SegmentKind::LegacySnapshot),
        });
        active_segments.push(manifest_store.write_segment(
            generation,
            SegmentKind::Lexical,
            &lexical_segment,
            SegmentStats {
                token_count,
                ..SegmentStats::default()
            },
            superseded_by_kind(SegmentKind::Lexical),
        )?);
        active_segments.push(manifest_store.write_segment(
            generation,
            SegmentKind::Metadata,
            &metadata_segment,
            SegmentStats {
                doc_count: metadata_segment.documents.len() as u64,
                chunk_count,
                token_count,
            },
            superseded_by_kind(SegmentKind::Metadata),
        )?);
        active_segments.push(manifest_store.write_segment(
            generation,
            SegmentKind::Graph,
            &graph_segment,
            SegmentStats {
                doc_count: metadata_segment.documents.len() as u64,
                chunk_count,
                token_count,
            },
            superseded_by_kind(SegmentKind::Graph),
        )?);
        if let Some(embedding_segment) = embedding_segment {
            active_segments.push(manifest_store.write_segment(
                generation,
                SegmentKind::Embedding,
                &embedding_segment,
                SegmentStats {
                    chunk_count: embedding_segment.embeddings.len() as u64,
                    token_count,
                    ..SegmentStats::default()
                },
                superseded_by_kind(SegmentKind::Embedding),
            )?);
        }
        if let Some(eigen_segment) = eigen_segment {
            active_segments.push(manifest_store.write_segment(
                generation,
                SegmentKind::Eigen,
                &eigen_segment,
                SegmentStats {
                    token_count,
                    ..SegmentStats::default()
                },
                superseded_by_kind(SegmentKind::Eigen),
            )?);
        }

        let tombstones = previous_manifest
            .map(|manifest| {
                manifest
                    .active_segments
                    .into_iter()
                    .map(|segment| segment.segment_id)
                    .collect()
            })
            .unwrap_or_default();
        let _ = manifest_store.publish_runtime_segments(
            ManifestMetadata {
                domain,
                source_count,
                vocabulary_size,
            },
            active_segments,
            tombstones,
        )?;
        let _ = self.clear_recovery_snapshot();

        Ok(())
    }
}
