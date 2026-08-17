# Examples Cookbook

> Copyable recipes for common Memento workflows. Replace example paths and
> source names with your own.

[← Documentation](README.md) · [Quick start](QUICKSTART.md) ·
[Configuration](CONFIGURATION.md) · [CLI reference](CLI.md)

## Existing Obsidian vault

Use this when your vault already contains the material you want to search.

```bash
vault="$HOME/Documents/MyVault"

memento init --vault-root "$vault"
memento doctor
memento sync obsidian "$vault" --json
memento learn --json
memento query "What did we decide about the launch?" --limit 8
```

Create `$vault/.mementoignore` before the first large sync:

```gitignore
.git/
.obsidian/
.trash/
node_modules/
dist/
*.log
.env
.env.*
*.pem
/private/
```

Verify incremental behavior:

```bash
memento sync obsidian "$vault" --json
```

The second response should show most files under `unchanged_files` and no
duplicate corpus growth.

## New brain fed by several Markdown trees

Generate a starter configuration:

```bash
memento init --vault-root "$HOME/MementoVault"
```

Add source roots to `~/.memento/config/vault_sync.toml`:

```toml
[vault]
root = "~/MementoVault"
state_dir = "~/.memento/sync"

[[markdown_sync.roots]]
name = "projects"
source = "~/Projects"
destination = "projects"
include_extensions = [".md"]
exclude_dirs = [".git", "node_modules", "target", "dist"]
protected_globs = ["**/_memento_hub.md", "**/MOC - *.md"]
delete_removed = true

[[markdown_sync.roots]]
name = "notes"
source = "~/Documents/Notes"
destination = "notes"
include_extensions = [".md", ".txt"]
exclude_dirs = ["archive-private"]
protected_globs = []
delete_removed = true

[linking]
enabled = true
hub_filename = "_memento_hub.md"
root_hub = "_memento.md"
tag_hubs = true
min_tag_documents = 2
inject_navigation = true
exclude_dirs = [".git", ".obsidian", ".trash"]
```

Run and index:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml --json run-all
memento sync obsidian "$HOME/MementoVault" --json
memento query "Which projects mention the migration?" --output compact
```

## PDF and Office research library

Add a document source:

```toml
[[document_import.sources]]
name = "research"
enabled = true
source = "~/Documents/Research"
destination = "documents/research"
manifest = "documents-research.json"
include_extensions = [".pdf", ".docx", ".pptx", ".xlsx", ".html", ".csv", ".json"]
exclude_dirs = [".git", ".obsidian", "node_modules", ".venv"]
preserve_raw = false
delete_removed = true
tags = ["research", "imported"]
max_file_bytes = 104857600
```

Check local converters, convert, inspect, then sync:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml capabilities
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-documents research

find "$HOME/MementoVault/documents/research" -name '*.md' -maxdepth 4 | head
memento sync obsidian "$HOME/MementoVault"
memento query "What evidence supports the offline architecture?" --limit 6
```

If a scanned PDF produces no text, install Tesseract and the required language
pack, rerun conversion, and inspect the generated page markers.

## SQLite decisions as durable notes

Suppose a local application has:

```sql
CREATE TABLE decisions (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Configure a read-only query:

```toml
[[database_import.sources]]
name = "decisions"
enabled = true
driver = "sqlite"
database = "~/data/app.db"
query = "SELECT id, title, body, updated_at FROM decisions ORDER BY id"
destination = "databases/decisions"
manifest = "database-decisions.json"
id_column = "id"
title_column = "title"
content_columns = ["body"]
metadata_columns = ["updated_at"]
updated_at_column = "updated_at"
tags = ["database", "decisions"]
delete_removed = true
```

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  --json import-databases decisions
memento sync obsidian "$HOME/MementoVault" --json
memento query "What was the decision about session retention?"
```

Each row becomes a stable note whose filename contains a sanitized ID and short
hash. A changed row updates that note; a missing row removes it when
`delete_removed = true`.

## PostgreSQL or MySQL knowledge view

Keep credentials out of TOML:

```toml
[[database_import.sources]]
name = "knowledge"
enabled = true
driver = "postgres"
dsn_env = "BRAIN_POSTGRES_DSN"
query = "SELECT id, title, body, updated_at FROM public.knowledge_view ORDER BY id"
destination = "databases/knowledge"
manifest = "database-knowledge.json"
id_column = "id"
title_column = "title"
content_columns = ["body"]
metadata_columns = ["updated_at"]
updated_at_column = "updated_at"
tags = ["database", "knowledge"]
delete_removed = true
```

```bash
export BRAIN_POSTGRES_DSN='postgresql://readonly:secret@127.0.0.1/brain'
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-databases knowledge
```

Use a database role with actual read-only grants. For MySQL/MariaDB, set
`driver = "mysql"`, choose a DSN environment name, and use a
`mysql://user:password@host:3306/database` URL.

## Import local AI sessions

Generated onboarding config detects known directories. A manual Codex section:

```toml
[session_import.codex]
enabled = true
source = "~/.codex/sessions"
destination = "converted/codex"
manifest = "codex-manifest.json"
label = "Codex"
source_tag = "codex"
file_glob = "*.jsonl"
exclude_path_fragments = []
```

Import one or all:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-sessions codex
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-sessions all
memento sync obsidian "$HOME/MementoVault"
memento query "What did the agent conclude about the index design?"
```

For the native daemon path without normalized Markdown:

```bash
memento sync codex
memento sync claude
```

Choose one primary path per session source to avoid indexing the same material
twice.

## Full scheduled personal brain

`memento init` creates the baseline. A packaged runner section should be:

```toml
[vault]
root = "/home/user/MementoVault"
vault_sync_config = "/home/user/.memento/config/vault_sync.toml"

[vault_sync_runner]
command = ["memento-vault-sync"]

[scheduler]
enabled = true
default_interval = "4h"
run_on_start = true
batch_updates = true

[[scheduler.jobs]]
name = "vault-sync"
enabled = true
type = "vault_sync"
config = "/home/user/.memento/config/vault_sync.toml"
command = "run-all"
interval = "4h"
```

Validate manually before enabling the loop:

```bash
memento doctor
memento-vault-sync --config ~/.memento/config/vault_sync.toml --json run-all
memento sync folder "$HOME/MementoVault" --json
brew services restart memento
memento status
```

The scheduled job normalizes sources, synchronizes the maintained vault, learns
the changed corpus, and publishes last/next-run state.

## Machine-readable CLI automation

Use complete JSON when downstream code needs full content:

```bash
memento query "release decision" --output json > /tmp/memento-query.json
jq '.results[] | {source_path, score}' /tmp/memento-query.json
```

Use compact JSON for a context budget:

```bash
memento query "release decision" \
  --output compact \
  --limit 5 \
  --max-content-chars 320 \
  | jq .
```

Health gate for a script:

```bash
status="$(memento status --json)"
jq -e '.total_chunks > 0 and .runtime_segments_ready == true' <<<"$status" >/dev/null
```

Treat field values as data. Do not parse styled human output.

## Agent memory with MCP

Register the local server:

```bash
codex mcp add memento -- memento-mcp
```

A good agent workflow is:

```text
1. memento_get_status
2. memento_search_memory
   query: "What did we decide about authentication?"
   limit: 5
   max_chars_per_result: 400
   include_answer: false
3. Inspect evidence paths and scores.
4. memento_get_document only for the exact strongest path if needed.
5. Cite the source path in the final answer.
```

Configure write approvals so `memento_sync_source` requires confirmation while
search/status remain read-only. Full tool schemas are in [MCP.md](MCP.md).

## Isolated test store

Use this for experiments, bug reports, and integration tests:

```bash
runtime_dir="$(mktemp -d /tmp/memento-runtime.XXXXXX)"
vault_dir="$(mktemp -d /tmp/memento-vault.XXXXXX)"

export MEMENTO_DATA_DIR="$runtime_dir"
memento init --vault-root "$vault_dir" --force

printf '%s\n' \
  '# Authentication decision' \
  '' \
  'The launch code is cobalt hummingbird.' \
  > "$vault_dir/authentication.md"

memento sync obsidian "$vault_dir" --json
memento learn --json
memento query "What is the launch code?" --output compact
```

The temporary paths are independent of `~/.memento` and any real vault. Keep
the environment exported in every terminal participating in the test.

## Local authenticated HTTP

Use only when a local integration cannot use the Unix socket or MCP:

```bash
mementod --foreground --http-port 8765
```

In another terminal:

```bash
token="$(< ~/.memento/config/http_auth_token)"

curl http://127.0.0.1:8765/health
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{"query":"authentication decision","top_k":5}' \
  http://127.0.0.1:8765/query
```

Keep the listener on loopback. The [configuration reference](CONFIGURATION.md#optional-http-api)
lists routes and authentication behavior.

## Retrieval regression fixture

Create JSONL with one evaluation case per line:

```json
{"id":"auth-001","query":"What is the launch code?","expected_path":"authentication.md","expected_title":"Authentication decision","expected_terms":["cobalt","hummingbird"],"excerpt":"The launch code is cobalt hummingbird."}
```

Run an optimized benchmark against an isolated store:

```bash
cargo run --release -p memento-research -- benchmark run \
  --dataset /absolute/path/to/benchmark.jsonl \
  --top-k 10 \
  --report /tmp/memento-benchmark.json
```

Add a case whenever you fix a ranking failure. See [Benchmarks](BENCHMARKS.md)
for metric definitions and privacy guidance.
