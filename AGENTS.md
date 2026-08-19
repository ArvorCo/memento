# Memento Repository Guide

## Mission

Build Memento as a local-first intelligent memory system for people and agents.
The core promise is simple: ingest real work, consolidate it into durable memory,
and retrieve it with useful semantic context.

## Product Direction

The current source of truth is the local-first direction described in `VISION.md`.
Prefer this architecture when making decisions:

- `libmemento`: core memory engine, file format, chunking, learning, retrieval
- `mementod`: local daemon managing `.memento` data and serving local APIs
- `memento-cli`: primary user interface for import, query, learn, and status
- `memento-research`: evaluation and backend diagnostics for retrieval quality
- `memento-web`: product surface, landing page, dashboard, and future memory UX
- `memento-api`: experimental/cloud-facing surface; do not let it redefine the core

When older docs conflict with this model, favor the engine/daemon/CLI path.

## Working Priorities

1. Make the local ingest → learn → query loop reliable.
2. Keep the `.memento` format stable and well tested.
3. Improve retrieval quality before adding broad feature surface.
4. Use the web app to communicate the product clearly, not as a generic template.
5. Treat sync/cloud as optional extensions, not the default architecture.

## Engineering Rules

- Prefer small vertical slices that leave the repo in a runnable state.
- Keep Rust logic in `libmemento` when it can be shared by CLI, daemon, and API.
- Avoid introducing cloud-only assumptions into core types or workflows.
- Update docs when code changes architecture, commands, or primary workflows.
- Preserve user changes already present in the worktree unless explicitly asked.
- Do not hardcode personal machine paths into product code, docs, or defaults.
- Prefer generated config, env vars, and editable user settings over shell wrappers.

## Operational Notes

- The runtime is local-first and centered on `mementod` + `memento-cli`.
- `mementod` manages local state under `~/.memento/` by default, or `MEMENTO_DATA_DIR` when set.
- `memento-cli` supports `import`, `sync`, `learn`, `query`, and `status`.
- `tools/vault_sync` is the feeder layer for user folders and external exports.
- `memento-research` is the place for benchmark, doctor, probe, and retrieval evaluation work.

## Sync And Import Model

- Prefer `tools/vault_sync` over personal cron, shell, or launchd wrappers.
- Sync/import behavior should be driven by TOML config, not user-specific code branches.
- The current generic feeder layer supports:
  - Markdown tree sync
  - AI session importers: `codex`, `droid`, `claude`, `chatgpt`
  - Connectors: `icloud`, `apple-notes`, `whatsapp`
- The reference config is `tools/vault_sync/config.example.toml`.
- Cross-platform setup should start from `init-config --preset auto|mac|linux|windows`.

## Python Tooling

- The repo baseline is Python `3.12`, managed through `uv`.
- Use `uv`, not ad hoc virtualenv activation, as the primary Python workflow.
- Use `ruff` for linting/formatting Python in `tools/`.
- Use the root `Makefile` shortcuts for common Python validation tasks.

Preferred commands:

- `uv sync --group dev`
- `uv run ruff check tools`
- `uv run ruff format tools`
- `uv run python -m unittest discover -s tools/vault_sync/tests -v`
- `make uv-sync`
- `make py-lint`
- `make py-test`
- `make fmt`

## Agent Skill

- The main operational skill for this repo is `.agents/skills/memento-runtime/SKILL.md`.
- Use it when the task is to run Memento on a machine, configure sync, debug ingest, or troubleshoot retrieval.
- UI metadata for the skill lives in `.agents/skills/memento-runtime/agents/openai.yaml`.
- `AGENT_INSTALL.md` is the public prompt and execution contract for installing
  the program, skill, and MCP/CLI integration from an AI agent.
- `scripts/install.sh` is the canonical multi-host installer; keep it
  idempotent and validate it with `scripts/test-install.sh`.

## Common Commands

Workspace:

- `cargo test`
- `cargo test -p libmemento`
- `cargo test -p memento-research`
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets`

Daemon and CLI:

- `cargo run -p mementod -- --foreground`
- `cargo run -p memento-cli -- status`
- `cargo run -p memento-cli -- import claude <path>`
- `cargo run -p memento-cli -- sync obsidian <path>`
- `cargo run -p memento-cli -- query "what did we decide?"`
- `cargo run -p memento-cli -- learn`

Research:

- `cargo run -p memento-research -- doctor`
- `cargo run -p memento-research -- probe --backend mlx`
- `cargo run -p memento-research -- benchmark run --dataset <path> --corpus <path> --report <path>`

Web:

- `cd memento-web && npm install`
- `cd memento-web && npm run dev`
- `cd memento-web && npm run lint`
- `cd memento-web && npm run build`

## Current Baseline

- The engine already supports a local `import -> sync -> learn -> query` loop.
- The runtime persists local state and exposes kernel readiness through `memento-cli status`.
- Retrieval quality work is benchmarked in `memento-research`; prefer measured improvements over intuition-only ranking changes.
- The web app should stay product-specific and avoid generic scaffold language.
- Root planning docs still contain older assumptions; reconcile them incrementally instead of rewriting everything at once.

## First Milestone Definition

Memento is in a good local-first baseline when all of the following are true:

- a session or document can be imported locally
- a folder or vault can be synced locally without duplicating content
- the daemon can manage the resulting memory store
- query results return relevant chunks with traceable provenance
- the system can run `learn` and return grounded answers from retrieved evidence
- the file format roundtrips safely
- the web app explains the product without placeholder content
