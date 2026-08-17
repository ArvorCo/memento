---
name: memento-runtime
description: Install, configure, operate, and troubleshoot the local-first Memento memory system. Use when a user asks to install Memento, connect Codex, Claude Code, OpenClaw, or another agent through MCP or CLI, initialize an Obsidian vault or document collection, ingest or synchronize sources, learn, search memory, inspect evidence, or diagnose retrieval quality.
---

# Memento Runtime

Operate Memento as a local, evidence-first memory system. Keep source material
and the memory store on the user's machine unless the user explicitly chooses a
remote workflow.

## Choose the access surface

Use this priority order:

1. Use Memento MCP tools when the host exposes them.
2. Otherwise use the installed `memento` CLI.
3. Use `memento-vault-sync` only for heterogeneous source conversion or feeder
   workflows.
4. Use repository commands such as `cargo run` only when developing Memento
   itself from a source checkout.

Never assume MCP is configured merely because `memento-mcp` is installed.
Never assume the CLI can see the same store as MCP when `MEMENTO_DATA_DIR`
differs between processes.

## Bootstrap or repair installation

Check the installation first:

```bash
command -v memento
command -v mementod
command -v memento-mcp
memento --version
```

If any core binary is missing, follow the repository's agent installation
contract:

```text
https://github.com/ArvorCo/memento/blob/main/AGENT_INSTALL.md
```

When a Homebrew package already provides `memento-agent-install`, use it to add
or repair the skill and host integration:

```bash
memento-agent-install --agent auto --integration auto --program skip
```

Do not pipe an unreviewed remote script directly into a shell. Clone or download
the repository, inspect `scripts/install.sh`, and then execute it.

## First-run sequence

Ask for or discover the intended vault path. Do not invent a personal path.
Then run:

```bash
memento init --vault-root "/absolute/path/to/vault"
memento doctor
memento status
```

Initialization creates editable configuration under `~/.memento/config` by
default. Use `MEMENTO_DATA_DIR` for an isolated store or non-default location,
and apply the same value to the daemon, CLI, and MCP server.

Continue with a narrow vertical test:

```bash
memento sync obsidian "/absolute/path/to/vault"
memento learn
memento query "a distinctive phrase from the vault" --limit 5 --output compact
```

Only automate background synchronization after this loop succeeds.

## Use MCP as an agent

Prefer this evidence-bounded workflow:

1. Call `memento_get_status` to confirm readiness when state is uncertain.
2. Call `memento_search_memory` with the user's actual question.
3. Reason from returned evidence and retain each `source_path`.
4. Call `memento_get_document` only for an exact source returned by search and
   only when the excerpt is insufficient.
5. Call `memento_sync_source` or `memento_learn` only when mutation is requested
   or necessary and permitted.

Available tools:

| Tool | Mutation | Use |
| --- | --- | --- |
| `memento_search_memory` | No | Search with bounded excerpts and optional grounded answer |
| `memento_get_document` | No | Page through an exact indexed source |
| `memento_get_status` | No | Inspect corpus and runtime readiness |
| `memento_sync_source` | Yes | Synchronize one tracked source |
| `memento_learn` | Yes | Recompute local learned retrieval state |

Do not use `memento_get_document` as an arbitrary filesystem reader. Copy the
exact path from search evidence. Keep result limits and excerpt sizes small
unless the task demonstrably needs more context.

## Use the CLI as an agent

Check status and retrieve compact evidence:

```bash
memento status --json
memento query "What did we decide about authentication?" \
  --limit 5 \
  --output compact \
  --max-content-chars 600
```

Preserve provenance in the response. Distinguish retrieved facts from your own
inference. If the query returns weak or empty evidence, report that honestly
instead of fabricating memory.

Common operator commands:

```bash
memento doctor
memento status
memento import file "/absolute/path/to/note.md"
memento import folder "/absolute/path/to/documents"
memento sync folder "/absolute/path/to/documents"
memento sync obsidian "/absolute/path/to/vault"
memento learn
memento query "question"
```

Repeated folders should normally use `sync`, which removes stale indexed chunks
for files removed from that tracked source. Treat synchronization as a mutation.

## Use the vault feeder

Use `memento-vault-sync` for PDFs, Office documents, databases, AI session
exports, iCloud folders, Apple Notes, WhatsApp exports, or multiple Markdown
trees.

Inspect capabilities and generated configuration before running everything:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml capabilities
memento-vault-sync --config ~/.memento/config/vault_sync.toml --json run-all
```

Keep global feeder options before the subcommand. Use a narrow connector command
when debugging instead of repeatedly running the entire feeder.

Never put database credentials, tokens, `.env` files, private keys, or unrelated
personal folders into the vault. Use read-only database credentials and
explicit queries.

## Diagnose failures

### Daemon unreachable

Run, in order:

```bash
memento doctor
mementod --foreground
memento status
```

Use an isolated `MEMENTO_DATA_DIR` to distinguish configuration or store damage
from a binary/runtime failure. Do not delete the user's primary store as a
diagnostic shortcut.

### Sync imported nothing

Verify:

1. the exact source path exists and is readable
2. ignore rules do not exclude it
3. the intended connector is enabled
4. the feeder config points at the intended vault
5. an incremental manifest is not correctly suppressing unchanged content

### Retrieval quality is poor

Verify ingest before blaming ranking:

1. confirm the expected source appears in status or a narrow exact query
2. run `memento learn`
3. retry with distinctive names, identifiers, or dates
4. inspect returned paths and excerpts
5. use `memento-research` only from a source checkout for systematic benchmark
   work

### MCP is installed but unavailable

Confirm the host registration and executable path:

```bash
codex mcp list
claude mcp list
openclaw mcp status --verbose
```

For OpenClaw, probe the configured server:

```bash
openclaw mcp doctor memento --probe
```

Restart or reload the host after changing skill or MCP configuration when its
runtime does not detect changes automatically.

## Safety invariants

- Keep Memento local-first and CPU-first.
- Never hardcode or disclose personal source paths.
- Never upload vault content as part of installation or diagnostics.
- Preserve user-authored files and protected vault hubs.
- Require clear authorization before changing ignore rules, deleting indexed
  state, or broadening source scope.
- Prefer generated, editable configuration over shell wrappers with embedded
  machine-specific paths.
- Validate a representative query and its source provenance after every
  meaningful ingest, configuration, or retrieval change.

## Canonical documentation

- Installation contract: <https://github.com/ArvorCo/memento/blob/main/AGENT_INSTALL.md>
- CLI and onboarding: <https://github.com/ArvorCo/memento/blob/main/docs/QUICKSTART.md>
- MCP tools and security: <https://github.com/ArvorCo/memento/blob/main/docs/MCP.md>
- Configuration: <https://github.com/ArvorCo/memento/blob/main/docs/CONFIGURATION.md>
- Troubleshooting: <https://github.com/ArvorCo/memento/blob/main/docs/TROUBLESHOOTING.md>
