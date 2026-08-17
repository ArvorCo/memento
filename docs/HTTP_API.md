# Local HTTP API Reference

> An opt-in authenticated transport for local integrations that cannot use the
> CLI, Unix socket client, or MCP.

[← Documentation](README.md) · [Configuration](CONFIGURATION.md#optional-http-api) ·
[Architecture](ARCHITECTURE.md#transport-and-trust-boundaries) ·
[Security](../SECURITY.md#http)

## Prefer narrower interfaces

Use the interface that grants the least authority:

| Consumer | Preferred interface |
| --- | --- |
| Human operator | `memento` CLI |
| Local AI agent | `memento-mcp` over `stdio` |
| Local program needing stable JSON | CLI JSON/compact output or Unix socket |
| Program that specifically requires TCP HTTP | Optional HTTP listener |

The HTTP router exposes every daemon mutation route. It is broader than the
bounded MCP tool surface and should remain loopback-only in normal operation.

## Start the listener

```bash
mementod --foreground --http-port 8765
```

The Unix socket remains active. HTTP defaults to `127.0.0.1`. On first start,
the daemon creates:

```text
~/.memento/config/http_auth_token
```

with mode `0600` on Unix. Supply a token from the environment instead when
process supervision manages secrets:

```bash
MEMENTO_HTTP_TOKEN='replace-with-a-strong-secret' \
  mementod --foreground --http-port 8765
```

## Authentication

`GET /health` is unauthenticated. Every other route accepts either:

```http
Authorization: Bearer <token>
```

or:

```http
x-memento-token: <token>
```

Shell setup for examples:

```bash
base_url='http://127.0.0.1:8765'
token="$(< ~/.memento/config/http_auth_token)"
```

> [!WARNING]
> Bearer authentication does not encrypt traffic. Memento does not provide TLS.
> Keep the listener on loopback; remote exposure requires an explicit override
> plus external transport and network controls.

## Route summary

| Method | Route | Auth | Mutates | Purpose |
| --- | --- | ---: | ---: | --- |
| `GET` | `/health` | no | no | Process readiness |
| `GET` | `/status` | yes | no | Corpus/runtime state |
| `POST` | `/query` | yes | no | Full answer and ranked evidence |
| `GET` | `/query` | yes | no | Compatibility result-list query |
| `POST` | `/document` | yes | no | Exact indexed document page |
| `POST` | `/import` | yes | yes | Import a source |
| `POST` | `/sync` | yes | yes | Incrementally synchronize a source |
| `POST` | `/learn` | yes | yes | Recompute spectral state |
| `POST` | `/ingest` | yes | yes | Ingest caller-supplied raw text |

## `GET /health`

```bash
curl "$base_url/health"
```

```json
{
  "ok": true,
  "status": "ok",
  "service": "mementod"
}
```

Health proves the HTTP process/router responds. It does not prove a populated or
learned corpus; use `/status` for that.

## `GET /status`

```bash
curl -H "Authorization: Bearer $token" "$base_url/status"
```

Response fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `vocabulary_size` | integer | Known token vocabulary |
| `non_zero_count` | integer | Sparse co-occurrence matrix entries |
| `coherence_score` | number | Spectral-gap-derived corpus indicator |
| `total_chunks` | integer | Indexed chunks |
| `total_sources` | integer | Tracked sources |
| `document_graph_edges` | integer | Directed graph edges including backlinks |
| `domain` | string | Current memory domain |
| `memento_file_exists` | boolean | Compatible snapshot presence |
| `runtime_manifest_generation` | integer | Published generation |
| `runtime_segment_count` | integer | Active segment descriptors |
| `runtime_segments_ready` | boolean | Lexical + metadata segments present |
| `runtime_graph_ready` | boolean | Graph segment present |
| `runtime_embedding_ready` | boolean | Local spectral-projection segment present |
| `active_operation` | object/null | Recoverable operation checkpoint |
| `scheduler_enabled` | boolean | Scheduler loop state |
| `scheduled_jobs` | array | Per-job run state |

## `POST /query`

Request:

```json
{
  "query": "What did we decide about authentication?",
  "top_k": 5
}
```

`top_k` defaults to 10 when omitted.

```bash
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{"query":"What did we decide about authentication?","top_k":5}' \
  "$base_url/query"
```

Response:

```json
{
  "answer": "Grounded extractive answer.",
  "results": [
    {
      "content": "Bounded document-aware evidence content.",
      "score": 0.91,
      "source_path": "/vault/decisions/authentication.md",
      "chunk_index": 2
    }
  ],
  "confidence": 0.84,
  "query_tokens": 5,
  "key_concepts": ["authentication"]
}
```

The HTTP response contains complete result content. Use the MCP server or CLI
compact output when a caller needs enforced excerpt bounds.

## `GET /query` compatibility route

This route accepts query parameters and returns a result array without an answer:

```bash
curl -G \
  -H "Authorization: Bearer $token" \
  --data-urlencode 'q=authentication decision' \
  --data-urlencode 'limit=5' \
  "$base_url/query"
```

```json
[
  {
    "text": "Evidence content",
    "score": 0.91,
    "source": "/vault/decisions/authentication.md"
  }
]
```

`limit` defaults to 6. `session_id` is accepted for compatibility but does not
currently partition or personalize retrieval.

## `POST /document`

Request an exact indexed source in Unicode character pages:

```bash
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{
    "source_path":"/vault/decisions/authentication.md",
    "offset_chars":0,
    "max_chars":4000
  }' \
  "$base_url/document"
```

Response:

```json
{
  "source_path": "/vault/decisions/authentication.md",
  "title": "Authentication",
  "content": "First page…",
  "offset_chars": 0,
  "returned_chars": 4000,
  "total_chars": 9214,
  "has_more": true,
  "next_offset_chars": 4000
}
```

Defaults are offset 0 and 4,000 characters. The MCP wrapper clamps page size to
20,000; direct HTTP callers should impose the same or a smaller application
limit. Copy `source_path` from query results. A path absent from the store
returns `404 Not Found` even if it exists on disk.

## `POST /import`

```bash
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{"source":"file","path":"/absolute/path/to/note.md"}' \
  "$base_url/import"
```

Supported sources: `file`, `folder`, `obsidian`, `codex`, and `claude`. Path is
required for the first three.

Response:

```json
{
  "chunks_imported": 12,
  "source_type": "file"
}
```

Folder and Obsidian import delegate to incremental sync. Repeated file/session
imports may append data; prefer `/sync` for a recurring source.

## `POST /sync`

```bash
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{"source":"obsidian","path":"/absolute/path/to/vault"}' \
  "$base_url/sync"
```

Response uses the same `SyncResponse` documented in
[CLI reference](CLI.md#memento-sync): chunks, source type, removed state, file
change counts, coherence, and computed eigenvectors.

Sync may remove stale chunks for deleted source files. Treat this as a write
operation.

## `POST /learn`

No request body is required:

```bash
curl -X POST \
  -H "Authorization: Bearer $token" \
  "$base_url/learn"
```

```json
{
  "coherence_before": 0.61,
  "coherence_after": 0.68,
  "eigenvectors_computed": 24
}
```

Learning is CPU-bound and serialized with other state mutations.

## `POST /ingest`

This compatibility route ingests caller-supplied raw text:

```bash
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{
    "text":"The synthetic meeting decided to use passkeys.",
    "session_id":"meeting-42",
    "source":"integration://meeting-42"
  }' \
  "$base_url/ingest"
```

```json
{
  "id": "generated-uuid"
}
```

`text` is required. `source` and `session_id` are optional; `session_id` is
currently accepted but not persisted as a retrieval partition. If `source` is
omitted, a legacy compatibility label is used.

This endpoint does **not** connect to Telegram or make Telegram a supported
source. It only accepts text already supplied by an authenticated caller.

## Errors

| Status | Typical cause |
| --- | --- |
| `401 Unauthorized` | Missing or incorrect token |
| `404 Not Found` | Exact `/document` source is absent from memory |
| `422 Unprocessable Entity` | JSON body/schema does not match the route |
| `500 Internal Server Error` | Parse, source, storage, query, or learning failure |

Most operation errors are plain-text bodies in 0.1.x. Do not expose those bodies
to untrusted users because they may include local paths.

## Safe client behavior

- Set connect and request timeouts.
- Bound `top_k` and document page size in the client.
- Serialize or coordinate mutations; the daemon already locks them internally,
  but callers should avoid needless contention.
- Treat `source_path`, query text, error details, and evidence as sensitive.
- Retry read-only requests selectively; do not blindly retry import/sync.
- Check `/status` after a mutation instead of assuming corpus readiness.
- Keep the token out of URLs, logs, and command history.

For agent integrations, use [MCP.md](MCP.md); it exposes smaller schemas and
explicit safety annotations.
