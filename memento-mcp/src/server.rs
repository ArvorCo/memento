use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::{tool, tool_handler, tool_router, Json, ServerHandler};

use crate::client::DaemonClient;
use crate::models::*;

#[derive(Debug, Clone)]
pub struct MementoMcp {
    client: DaemonClient,
    tool_router: ToolRouter<Self>,
}

impl MementoMcp {
    pub fn new() -> Self {
        Self {
            client: DaemonClient::from_environment(),
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for MementoMcp {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl MementoMcp {
    /// Search local memory and return a bounded answer plus ranked, traceable evidence.
    #[tool(
        name = "memento_search_memory",
        annotations(
            title = "Search Memento Memory",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn search_memory(
        &self,
        Parameters(request): Parameters<SearchRequest>,
    ) -> Result<Json<SearchResponse>, String> {
        let query = request.query.trim();
        if query.is_empty() || query.chars().count() > 2_000 {
            return Err("query must contain 1-2000 characters".to_string());
        }
        let limit = request.limit.unwrap_or(5).clamp(1, 20);
        let max_chars = request.max_chars_per_result.unwrap_or(800).clamp(80, 4_000);
        let response: QueryResponse = self
            .client
            .post(
                "/query",
                &QueryRequest {
                    query: query.to_string(),
                    top_k: limit,
                },
            )
            .await
            .map_err(actionable_error)?;
        Ok(Json(compact_search_response(
            response,
            max_chars,
            request.include_answer.unwrap_or(true),
        )))
    }

    /// Read one bounded page from an already ingested document using an exact search result path.
    #[tool(
        name = "memento_get_document",
        annotations(
            title = "Read Memento Document",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn get_document(
        &self,
        Parameters(request): Parameters<GetDocumentRequest>,
    ) -> Result<Json<DocumentResponse>, String> {
        if request.source_path.trim().is_empty() {
            return Err("source_path must be copied exactly from a search result".to_string());
        }
        self.client
            .post(
                "/document",
                &DocumentRequest {
                    source_path: request.source_path,
                    offset_chars: request.offset_chars.unwrap_or(0),
                    max_chars: request.max_chars.unwrap_or(4_000).clamp(1, 20_000),
                },
            )
            .await
            .map(Json)
            .map_err(actionable_error)
    }

    /// Inspect local memory readiness, corpus size, graph size, and scheduler state.
    #[tool(
        name = "memento_get_status",
        annotations(
            title = "Get Memento Status",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn get_status(&self) -> Result<Json<StatusResponse>, String> {
        self.client
            .get("/status")
            .await
            .map(Json)
            .map_err(actionable_error)
    }

    /// Incrementally synchronize one configured local source into memory; removed source files remove stale chunks.
    #[tool(
        name = "memento_sync_source",
        annotations(
            title = "Sync Memento Source",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn sync_source(
        &self,
        Parameters(request): Parameters<SyncSourceRequest>,
    ) -> Result<Json<SyncResponse>, String> {
        if !matches!(
            request.source.as_str(),
            "file" | "folder" | "obsidian" | "claude" | "codex"
        ) {
            return Err("source must be file, folder, obsidian, claude, or codex".to_string());
        }
        if matches!(request.source.as_str(), "file" | "folder" | "obsidian")
            && request.path.as_deref().is_none_or(str::is_empty)
        {
            return Err(format!("path is required for source {}", request.source));
        }
        self.client
            .post(
                "/sync",
                &ImportRequest {
                    source: request.source,
                    path: request.path,
                },
            )
            .await
            .map(Json)
            .map_err(actionable_error)
    }

    /// Recompute local spectral memory signals after ingest or synchronization.
    #[tool(
        name = "memento_learn",
        annotations(
            title = "Learn Memento Memory",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub async fn learn(&self) -> Result<Json<LearnResponse>, String> {
        self.client
            .post("/learn", &serde_json::json!({}))
            .await
            .map(Json)
            .map_err(actionable_error)
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "memento-mcp",
    instructions = "Search before reading. Use exact source_path values with memento_get_document and paginate only when more context is necessary. Keep evidence bounded."
)]
impl ServerHandler for MementoMcp {}

fn compact_search_response(
    response: QueryResponse,
    max_chars: usize,
    include_answer: bool,
) -> SearchResponse {
    SearchResponse {
        answer: include_answer.then_some(response.answer),
        confidence: response.confidence,
        query_tokens: response.query_tokens,
        concepts: response.key_concepts,
        evidence: response
            .results
            .into_iter()
            .map(|result| {
                let (excerpt, excerpt_truncated) = truncate_chars(&result.content, max_chars);
                EvidenceResult {
                    source_path: result.source_path,
                    chunk_index: result.chunk_index,
                    score: result.score,
                    excerpt,
                    excerpt_truncated,
                }
            })
            .collect(),
    }
}

fn truncate_chars(value: &str, limit: usize) -> (String, bool) {
    if value.chars().count() <= limit {
        return (value.to_string(), false);
    }
    let mut output = value.chars().take(limit).collect::<String>();
    output.push('…');
    (output, true)
}

fn actionable_error(error: anyhow::Error) -> String {
    format!("Memento operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use rmcp::{ServerHandler, ServiceExt};

    use super::*;

    #[test]
    fn search_response_is_unicode_bounded_and_traceable() {
        let response = QueryResponse {
            answer: "answer".to_string(),
            confidence: 0.9,
            query_tokens: 3,
            key_concepts: vec!["wiki".to_string()],
            results: vec![QueryResult {
                content: "áβcdef".to_string(),
                score: 0.8,
                source_path: "notes/ação.md".to_string(),
                chunk_index: 2,
            }],
        };

        let compact = compact_search_response(response, 3, true);

        assert_eq!(compact.evidence[0].excerpt, "áβc…");
        assert!(compact.evidence[0].excerpt_truncated);
        assert_eq!(compact.evidence[0].source_path, "notes/ação.md");
    }

    #[test]
    fn tool_catalog_has_schemas_and_safety_annotations() {
        let server = MementoMcp::new();
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), 5);
        let search = tools
            .iter()
            .find(|tool| tool.name == "memento_search_memory")
            .unwrap();
        assert!(search.output_schema.is_some());
        assert_eq!(
            search
                .annotations
                .as_ref()
                .and_then(|value| value.read_only_hint),
            Some(true)
        );
        let sync = tools
            .iter()
            .find(|tool| tool.name == "memento_sync_source")
            .unwrap();
        assert_eq!(
            sync.annotations
                .as_ref()
                .and_then(|value| value.destructive_hint),
            Some(true)
        );
    }

    #[tokio::test]
    async fn protocol_handshake_lists_all_tools() -> anyhow::Result<()> {
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            MementoMcp::new()
                .serve(server_transport)
                .await?
                .waiting()
                .await?;
            anyhow::Ok(())
        });
        let client = ().serve(client_transport).await?;

        let catalog = client.list_tools(None).await?;

        assert_eq!(catalog.tools.len(), 5);
        assert!(catalog
            .tools
            .iter()
            .all(|tool| !tool.input_schema.is_empty()));
        client.cancel().await?;
        server.await??;
        Ok(())
    }
}
