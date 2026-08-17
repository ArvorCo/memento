# Command-Line Reference

> Exact commands for operating the Memento daemon, memory store, feeder, and
> local agent interface.

[← Documentation](README.md) · [Quick start](QUICKSTART.md) ·
[Configuration](CONFIGURATION.md) · [Examples](EXAMPLES.md)

## Installed commands

| Command | Role | Needs `mementod`? |
| --- | --- | ---: |
| `memento` | Initialize, diagnose, import, sync, learn, query, and inspect | Usually |
| `mementod` | Own the local memory store and serve local APIs | — |
| `memento-vault-sync` | Normalize heterogeneous sources into a maintained vault | No |
| `memento-mcp` | Expose bounded memory tools to a local MCP host over `stdio` | Yes |
| `memento-agent-install` | Install/repair the skill and MCP or CLI host integration | No |

Running an operational `memento` command automatically waits for or attempts to
start the daemon. `memento init` and `memento doctor` have their own lifecycle
handling.

## `memento`

```text
Usage: memento <COMMAND>

Commands:
  init    Generate initial daemon and vault sync config
  doctor  Validate config, feeder paths, and runtime health
  import  Import sessions or files into memory
  sync    Sync an existing source without duplicating content
  query   Query your memories
  status  Show memory status
  learn   Recompute learned spectral signals
```

Use `memento <command> --help` for the contract installed on your machine.

### `memento init`

Creates or reuses the vault, daemon configuration, feeder configuration, and
runtime directories. `memento onboard` is an alias.

```text
Usage: memento init [OPTIONS]

Options:
  --preset <PRESET>          auto, mac, linux, or windows [default: auto]
  --vault-root <PATH>        Vault to create or reuse [default: ~/MementoVault]
  --schedule <INTERVAL>      Default feeder interval [default: 8h]
  --force                    Overwrite changed generated config files
```

Examples:

```bash
memento init
memento init --vault-root "$HOME/Documents/MyVault"
memento init --preset linux --schedule 2h
MEMENTO_DATA_DIR=/tmp/memento-demo memento init \
  --vault-root /tmp/memento-demo-vault \
  --force
```

Without `--force`, initialization preserves a config file that differs from the
generated content. This prevents an upgrade from erasing manual source or
scheduler edits.

### `memento doctor`

Validates configuration and live runtime health.

```text
Usage: memento doctor
```

Checks include:

- daemon and feeder TOML syntax
- vault, source, connector, and runner paths
- scheduler interval and job definitions
- daemon reachability and runtime status
- document and database source configuration

Run it after installation, after configuration changes, and before collecting a
bug report.

### `memento import`

Imports a source as new input.

```text
Usage: memento import [OPTIONS] <SOURCE> [PATH]

Sources: claude, codex, file, folder, obsidian
Options: --json
```

| Source | Path | Behavior |
| --- | --- | --- |
| `claude` | Omit | Reads the local Claude session store |
| `codex` | Omit | Reads the local Codex session store |
| `file` | Required | Imports one supported document |
| `folder` | Required | Recursively imports supported files |
| `obsidian` | Required | Imports an Obsidian-compatible vault |

```bash
memento import codex
memento import file ./notes/decision.md
memento import folder ./project-notes --json
memento import obsidian "$HOME/Documents/MyVault"
```

Use `sync` for a source that will be revisited. Import is best for a one-time
load; sync keeps a manifest and reports added, updated, removed, and unchanged
files.

### `memento sync`

Synchronizes a source incrementally.

```text
Usage: memento sync [OPTIONS] <SOURCE> [PATH]

Sources: claude, codex, file, folder, obsidian
Options: --json
```

```bash
memento sync codex
memento sync file ./notes/decision.md
memento sync folder ./project-notes
memento sync obsidian "$HOME/Documents/MyVault" --json
```

The JSON response contains:

```json
{
  "chunks_synced": 42,
  "source_type": "obsidian",
  "removed_chunks": 3,
  "removed_sources": 1,
  "added_files": 2,
  "updated_files": 1,
  "removed_files": 1,
  "unchanged_files": 120,
  "coherence_after": 0.71,
  "eigenvectors_computed": 16
}
```

Numbers above are illustrative; field names are the stable interface for the
current release.

### `memento learn`

Recomputes corpus-derived spectral signals and publishes runtime state.

```text
Usage: memento learn [--json]
```

```bash
memento learn
memento learn --json
```

JSON fields are `coherence_before`, `coherence_after`, and
`eigenvectors_computed`. Learning is local and CPU-bound. It complements direct
lexical retrieval; exact matches do not depend on it.

### `memento query`

Retrieves ranked evidence and composes a grounded extractive answer.

```text
Usage: memento query [OPTIONS] <QUESTION>

Options:
  -l, --limit <N>                    Results [default: 5]
      --output <human|json|compact>  Output mode [default: human]
      --max-content-chars <N>        Per-result compact excerpt [default: 800]
```

```bash
memento query "What did we decide about authentication?"
memento query "ADR-0042" --limit 10
memento query "What changed?" --output compact --max-content-chars 300
memento query "Which source owns this policy?" --output json
```

| Output | Intended consumer | Content |
| --- | --- | --- |
| `human` | Terminal user | Styled answer, confidence, concepts, and evidence |
| `json` | Program needing full evidence | Complete answer and untruncated result content |
| `compact` | Agent or token-sensitive script | Same provenance with bounded excerpts |

Complete JSON has this shape:

```json
{
  "answer": "Grounded text assembled from retrieved evidence.",
  "results": [
    {
      "content": "Full chunk content",
      "score": 0.92,
      "source_path": "/vault/decisions/auth.md",
      "chunk_index": 3
    }
  ],
  "confidence": 0.84,
  "query_tokens": 6,
  "key_concepts": ["authentication"]
}
```

`--max-content-chars` affects compact output only and counts Unicode characters,
not bytes.

### `memento status`

Reports corpus and runtime readiness.

```text
Usage: memento status [--json]
```

Important fields:

| Field | Meaning |
| --- | --- |
| `total_sources`, `total_chunks` | Indexed corpus size |
| `vocabulary_size`, `non_zero_count` | Lexical/matrix footprint |
| `document_graph_edges` | Resolved graph relationships |
| `coherence_score` | Corpus-derived spectral coherence indicator |
| `runtime_manifest_generation` | Published runtime generation |
| `runtime_segment_count` | Durable runtime segments in that generation |
| `runtime_*_ready` | Segment, graph, and learned-signal readiness |
| `active_operation` | In-progress recoverable operation, when present |
| `scheduled_jobs` | Scheduler state and last/next run details |

## `mementod`

```text
Usage: mementod [OPTIONS]

Options:
  --http-port <PORT>       Add an HTTP listener
  --http-host <HOST>       Bind host [default: 127.0.0.1]
  --allow-remote-http      Permit a non-loopback HTTP host
  --data-dir <PATH>        Store path [default: ~/.memento]
  -f, --foreground         Keep the process attached
```

The Unix socket is always created. HTTP is additional and opt-in. A non-loopback
host is rejected unless `--allow-remote-http` is explicit; every HTTP route
except `/health` requires a bearer token.

```bash
mementod --foreground
mementod --foreground --data-dir /tmp/memento-demo
mementod --foreground --http-port 8765
```

See [Configuration → HTTP](CONFIGURATION.md#optional-http-api) before exposing a
network listener.

## `memento-vault-sync`

The feeder converts external material into provenance-rich Markdown and
maintains vault links.

```text
Usage: memento-vault-sync [--config PATH] [--json] <COMMAND>
```

> [!CAUTION]
> `--config` and `--json` are global options. They must appear **before** the
> subcommand.

| Command | Purpose |
| --- | --- |
| `init-config` | Generate a platform-aware starter TOML |
| `capabilities` | Report available local converters and enabled sources |
| `sync-markdown` | Incrementally copy configured Markdown trees |
| `import-documents [NAME\|all]` | Convert configured document sources |
| `import-databases [NAME\|all]` | Execute configured read-only queries |
| `import-sessions <CONNECTOR>` | Import `all`, `codex`, `droid`, `claude`, or `chatgpt` |
| `sync-icloud` | Sync configured iCloud folders on macOS |
| `export-apple-notes` | Export Apple Notes on macOS |
| `import-whatsapp` | Convert configured WhatsApp exports |
| `link-vault` | Build hierarchy/topic hubs and navigation blocks |
| `run-all` | Run enabled stages in deterministic pipeline order |

Generate and inspect a config:

```bash
memento-vault-sync init-config \
  --preset auto \
  --output ~/.memento/config/vault_sync.toml \
  --vault-root "$HOME/Documents/MyVault"

memento-vault-sync --config ~/.memento/config/vault_sync.toml capabilities
```

Run selected or complete stages:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-documents research

memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  import-sessions all

memento-vault-sync --config ~/.memento/config/vault_sync.toml \
  --json run-all
```

`run-all` order is Markdown → documents → databases → iCloud → Apple Notes →
WhatsApp → AI sessions → wiki linker. JSON mode returns one captured status for
each stage.

## `memento-mcp`

`memento-mcp` accepts no operational arguments. It speaks MCP over standard
input/output and finds the daemon through `MEMENTO_DATA_DIR` or `MEMENTO_SOCKET`.

```bash
memento-mcp --version
codex mcp add memento -- memento-mcp
```

Do not run it interactively and type shell commands into it; an MCP host owns
the process. See the [MCP integration guide](MCP.md).

## Running from a source checkout

| Installed command | Source-checkout equivalent |
| --- | --- |
| `memento …` | `cargo run -p memento-cli -- …` |
| `mementod …` | `cargo run -p mementod -- …` |
| `memento-mcp` | `cargo run -p memento-mcp` |
| `memento-vault-sync …` | `uv run python -m tools.vault_sync.cli …` |

Keep the separator `--` between Cargo arguments and application arguments.

## Environment variables

| Variable | Used by | Purpose |
| --- | --- | --- |
| `MEMENTO_DATA_DIR` | CLI, daemon, MCP | Override the local store root |
| `MEMENTO_SOCKET` | MCP | Override only the daemon socket path |
| `MEMENTO_HTTP_TOKEN` | Daemon | Supply HTTP bearer token instead of token file |
| `MEMENTO_VAULT_SYNC_CONFIG` | Feeder | Select feeder TOML without `--config` |
| `MEMENTO_REPO_ROOT` | Scheduler | Resolve a source-checkout feeder runner |
| `RUST_LOG` | Daemon | Configure Rust tracing verbosity |

Database DSN variable names are user-defined through `dsn_env`; see
[Configuration](CONFIGURATION.md#database-sources).
