# Configuration Reference

> The complete contract for runtime, scheduler, vault feeder, connectors,
> environment variables, and ignore rules.

[← Documentation](README.md) · [CLI reference](CLI.md) ·
[Ingestion](INGESTION.md) · [Troubleshooting](TROUBLESHOOTING.md)

## Configuration map

Memento separates runtime policy from source normalization:

```mermaid
flowchart TB
    accTitle: Memento configuration ownership
    accDescr: Environment variables select the local store; daemon.toml configures the vault and scheduler; vault_sync.toml configures source normalization; and .mementoignore excludes files during direct folder or Obsidian synchronization.

    env["Environment\nstore + transport overrides"] --> daemon["mementod"]
    dc["daemon.toml\nvault + scheduler"] --> daemon
    vc["vault_sync.toml\nsources + conversion + links"] --> feeder["memento-vault-sync"]
    ignore[".mementoignore\ndirect sync exclusions"] --> daemon
    feeder --> vault["Maintained vault"] --> daemon
```

| File or input | Default location | Owner |
| --- | --- | --- |
| Runtime directory | `~/.memento/` | CLI, daemon, MCP |
| Daemon config | `~/.memento/config/daemon.toml` | Scheduler and diagnostics |
| Feeder config | `~/.memento/config/vault_sync.toml` | `memento-vault-sync` |
| Ignore rules | `<synced-root>/.mementoignore` | Direct folder/Obsidian sync |
| HTTP token | `~/.memento/config/http_auth_token` | Optional HTTP listener |

Generate both TOML files with:

```bash
memento init --vault-root "$HOME/Documents/MyVault"
```

After every manual edit, run:

```bash
memento doctor
```

Restart `mementod` after changing scheduler configuration. Feeder configuration
is loaded on each feeder invocation.

## Environment variables

| Variable | Scope | Description |
| --- | --- | --- |
| `MEMENTO_DATA_DIR` | CLI, daemon, MCP | Replaces `~/.memento` as the runtime root |
| `MEMENTO_SOCKET` | MCP on Unix | Replaces `<data-dir>/memento.sock` for the MCP client |
| `MEMENTO_PIPE` | CLI, daemon, MCP on Windows | Overrides the derived local named-pipe name |
| `MEMENTO_HTTP_TOKEN` | Daemon HTTP | Supplies the bearer token without reading/writing the token file |
| `MEMENTO_VAULT_SYNC_CONFIG` | Feeder | Selects feeder TOML when `--config` is absent |
| `MEMENTO_REPO_ROOT` | Scheduler | Helps resolve a source-checkout feeder runner |
| `MEMENTO_VAULT_ROOT` | Scheduled feeder | Injected by the scheduler for child-process context |
| `RUST_LOG` | Daemon | Rust tracing filter, for example `mementod=debug` |

Use the same `MEMENTO_DATA_DIR` for every process that belongs to one store:

```bash
export MEMENTO_DATA_DIR=/tmp/memento-demo
mementod --foreground &
memento status
memento-mcp --version
```

Setting only `mementod --data-dir` does not redirect the CLI. The CLI derives
its Unix socket or Windows named pipe from `MEMENTO_DATA_DIR`, so prefer the
shared environment variable for an isolated stack.

## `daemon.toml`

A generated file looks like this:

```toml
[daemon]
data_dir = "/home/user/.memento"
transport = "unix"
socket_path = "/home/user/.memento/memento.sock"
http_enabled = false
http_host = "127.0.0.1"
http_port = 8765
allow_remote_http = false

[vault]
root = "/home/user/MementoVault"
vault_sync_config = "/home/user/.memento/config/vault_sync.toml"

[vault_sync_runner]
command = ["memento-vault-sync"]

[scheduler]
enabled = true
default_interval = "8h"
run_on_start = true
batch_updates = true

[[scheduler.jobs]]
name = "vault-sync"
enabled = true
type = "vault_sync"
config = "/home/user/.memento/config/vault_sync.toml"
command = "run-all"
interval = "8h"
```

### `[daemon]`

`data_dir`, `transport`, and the platform endpoint are validated by `memento
doctor` and document the generated layout. Unix uses `transport = "unix"` with
`socket_path = "<data-dir>/memento.sock"`. Windows generates:

```toml
[daemon]
data_dir = "C:\\Users\\me\\.memento"
transport = "named_pipe"
pipe_name = "\\\\.\\pipe\\memento-0123456789abcdef"
```

The actual suffix is a stable hash of the absolute data-directory path. Process
selection is controlled by `MEMENTO_DATA_DIR` or `mementod --data-dir`; use
`MEMENTO_PIPE` only for deliberate low-level transport overrides.

The generated `http_*` keys are reserved metadata in 0.1.x; they do not enable
HTTP. Use explicit `mementod` flags described under
[Optional HTTP API](#optional-http-api). This distinction prevents a config edit
from silently creating a network listener.

### Vault

| Key | Required | Meaning |
| --- | ---: | --- |
| `root` | yes | Vault synchronized by a scheduled job after feeder execution |
| `vault_sync_config` | yes | Feeder TOML used when a job leaves `config` empty |

The scheduled path uses a generic folder sync after the feeder has normalized
sources and generated wikilinks.

### `[vault_sync_runner]`

| Key | Required | Meaning |
| --- | ---: | --- |
| `command` | yes for scheduler | Executable plus fixed arguments as a TOML array |
| `working_dir` | no | Working directory, primarily for a source checkout |

Packaged installation:

```toml
[vault_sync_runner]
command = ["memento-vault-sync"]
```

Source checkout:

```toml
[vault_sync_runner]
command = ["uv", "run", "python", "-m", "tools.vault_sync.cli"]
working_dir = "/absolute/path/to/memento"
```

Do not put shell syntax, pipes, or credentials in this array. The scheduler
executes the program directly.

### `[scheduler]`

| Key | Default | Meaning |
| --- | --- | --- |
| `enabled` | `false` | Start configured job loops |
| `default_interval` | `8h` | Fallback for jobs without an interval |
| `run_on_start` | `false` | Run enabled jobs shortly after daemon readiness |
| `batch_updates` | `false` | Reserved batching policy; the current vault job already performs one consolidated sync |

Intervals are an unsigned integer plus `m`, `h`, or `d`: `15m`, `8h`, `1d`.
Seconds, decimals, and compound values such as `1h30m` are invalid.

### `[[scheduler.jobs]]`

| Key | Required | Current contract |
| --- | ---: | --- |
| `name` | yes | Unique operator-facing name |
| `enabled` | no | Defaults to `true` |
| `type` | yes | Only `vault_sync` is supported |
| `config` | yes | Feeder config path; empty uses `[vault].vault_sync_config` |
| `command` | yes | Feeder subcommand; normally `run-all` |
| `interval` | no | Overrides `default_interval` |

A `vault_sync` job:

1. invokes the configured feeder subcommand
2. aborts the job if the feeder fails
3. incrementally syncs the configured vault into memory
4. updates learned state as part of that sync
5. records duration, result, and next run in status

## `vault_sync.toml`

The complete annotated source is
[`tools/vault_sync/config.example.toml`](../tools/vault_sync/config.example.toml).
Paths support `~` and environment-variable expansion. Vault destinations must
be relative and cannot contain `..`.

Select this file with one of:

```bash
memento-vault-sync --config /absolute/path/to/vault_sync.toml run-all
MEMENTO_VAULT_SYNC_CONFIG=/absolute/path/to/vault_sync.toml \
  memento-vault-sync run-all
```

### `[vault]`

```toml
[vault]
root = "~/MementoVault"
state_dir = "~/.memento/sync"
```

`root` owns generated Markdown. `state_dir` owns manifests and hashes. Keep
state outside the vault unless you intentionally want those implementation
files visible to note tools.

### Markdown roots

```toml
[[markdown_sync.roots]]
name = "workspace"
source = "~/Projects"
destination = "projects"
include_extensions = [".md", ".txt"]
exclude_dirs = [".git", "node_modules", "target", "dist"]
protected_globs = ["**/_*_hub.md", "**/MOC - *.md"]
manifest = "markdown-workspace.json"
delete_removed = true
```

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | required | Stable source identity |
| `source` | required | Absolute or expanded source tree |
| `destination` | required | Vault-relative output directory |
| `include_extensions` | `[".md"]` | Case-normalized extensions |
| `exclude_dirs` | `[]` | Directory names skipped at any depth |
| `protected_globs` | `[]` | Matching destinations the feeder must not replace/delete |
| `manifest` | derived from name | Absolute or state-relative manifest |
| `delete_removed` | `true` | Remove only feeder-owned outputs missing at source |

### Wiki linking

```toml
[linking]
enabled = true
default_project_prefix = "projects"
hub_filename = "_memento_hub.md"
root_hub = "_memento.md"
tag_hubs = true
min_tag_documents = 2
inject_navigation = true
exclude_dirs = [".git", ".obsidian", ".trash"]

[linking.project_aliases]
memento = "projects/Memento/Memento"
```

The linker creates marked, tool-owned hierarchy and topic hubs. It refuses to
overwrite an unmarked human file at the same hub path. Navigation injection is
idempotent and confined to marked blocks.

### Document sources

```toml
[[document_import.sources]]
name = "research"
enabled = true
source = "~/Documents/Research"
destination = "documents/research"
manifest = "documents-research.json"
include_extensions = [".pdf", ".docx", ".pptx", ".xlsx", ".csv", ".json"]
exclude_dirs = [".git", ".obsidian", "node_modules", ".venv"]
preserve_raw = false
raw_destination = "raw/research"
delete_removed = true
tags = ["research", "imported"]
max_file_bytes = 104857600
```

`raw_destination` is used only when `preserve_raw = true`. The importer hashes
content, emits provenance frontmatter, uses stable output names, and removes
only files recorded as owned by its manifest.

### Database sources

```toml
[[database_import.sources]]
name = "decisions"
enabled = true
driver = "sqlite"
database = "~/data/knowledge.db"
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

| Driver | Connection setting | Local dependency |
| --- | --- | --- |
| `sqlite` | `database = "/path/to/file.db"` | Python standard library |
| `postgres` | `dsn_env = "BRAIN_POSTGRES_DSN"` | `psycopg` |
| `mysql` / `mariadb` | `dsn_env = "BRAIN_MYSQL_DSN"` | `pymysql` |

Remote credentials must stay in the named environment variable. Queries must
begin with `SELECT` or `WITH`. Connections use explicit read-only/query-only
semantics and transactions are rolled back. A stable, unique `id_column` is
essential for incremental updates.

```bash
export BRAIN_POSTGRES_DSN='postgresql://reader:secret@localhost/knowledge'
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-databases decisions
```

### Session sources

Supported connector keys are `codex`, `droid`, `claude`, and `chatgpt`.

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

Each importer parses its own export format, emits stable Markdown, and records a
manifest. Use `exclude_path_fragments` to drop internal subtrees such as Claude
subagent sessions.

### iCloud folders

macOS only:

```toml
[icloud_sync]
enabled = true
root = "~/Library/Mobile Documents/com~apple~CloudDocs"

[[icloud_sync.folders]]
name = "documents"
source = "Documents"
raw_destination = "raw/icloud/documents"
converted_destination = "converted/icloud/documents"
include_markdown = true
include_text = true
convert_doc = true
convert_docx = true
convert_pptx = false
convert_pdf = true
```

Folder paths under `source` are relative to the iCloud root. Destination paths
are relative to the vault.

### Apple Notes

macOS only:

```toml
[apple_notes]
enabled = true
destination = "converted/apple-notes"
include_index = true
```

Apple Notes automation may require macOS Automation permission for the invoking
terminal or service account.

### WhatsApp exports

```toml
[whatsapp_import]
enabled = true
source = "~/Downloads"
destination = "whatsapp"
manifest = "whatsapp-manifest.json"
default_category = "other"

[[whatsapp_import.category_rules]]
name = "work"
destination = "work"
matches = ["team", "project", "work"]
```

Rules match export names and route conversations into vault-relative categories.
The importer supports configured ZIP or text exports; it does not connect to
WhatsApp accounts or scrape live messages.

## `.mementoignore`

Direct `folder` and `obsidian` imports/syncs read a `.mementoignore` from the
source root. Syntax follows familiar gitignore-style lines:

```gitignore
# Build and dependency output
node_modules/
target/
dist/
*.log
.env
.env.*
*.pem

# Sensitive or noisy vault zones
/private/
/exports/
**/.trash/
```

Review exclusions before the first sync. Ignore rules protect privacy and
reduce noise; removing a rule can add a large amount of material on the next
sync.

## Optional HTTP API

HTTP is disabled unless a port is passed to `mementod`:

```bash
mementod --foreground --http-port 8765
```

Defaults and safeguards:

- binds `127.0.0.1` unless `--http-host` is supplied
- refuses non-loopback hosts without `--allow-remote-http`
- creates a mode-`0600` token at
  `~/.memento/config/http_auth_token` when needed
- accepts `Authorization: Bearer <token>` or `x-memento-token`
- leaves `/health` unauthenticated; every other HTTP route requires the token

```bash
token="$(< ~/.memento/config/http_auth_token)"
curl http://127.0.0.1:8765/health
curl -H "Authorization: Bearer $token" \
  http://127.0.0.1:8765/status
curl -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  -d '{"query":"release decision","top_k":5}' \
  http://127.0.0.1:8765/query
```

> [!WARNING]
> A memory store can contain highly sensitive material. Prefer the platform's
> private local transport: a Unix socket on macOS/Linux or named pipe on Windows.
> `--allow-remote-http` is an explicit escape hatch, not a deployment
> recommendation.

All routes and schemas are documented in [Local HTTP API](HTTP_API.md).

## Validation checklist

After a configuration change:

```bash
memento doctor
memento-vault-sync --config ~/.memento/config/vault_sync.toml capabilities
memento-vault-sync --config ~/.memento/config/vault_sync.toml --json run-all
memento status --json
```

Run the feeder twice. A stable second pass should report unchanged/skipped inputs
and no unintended removals. Use an [isolated store](EXAMPLES.md#isolated-test-store)
before testing destructive source changes.
