# Vault Sync

Config-driven feeder toolkit that turns local sources into a maintained
Markdown/Obsidian vault for Memento.

It provides:

- content-hash incremental Markdown tree sync
- document discovery and conversion with provenance
- layered PDF extraction and local OCR fallback
- read-only database query imports
- Codex, Claude, Droid, and ChatGPT session importers
- iCloud, Apple Notes, and WhatsApp connectors
- hierarchy/topic hubs and idempotent wiki navigation
- compact JSON output for schedulers and agents

No product code contains a personal vault path. All sources, destinations,
manifests, exclusions, and connector behavior come from TOML.

## Development

```bash
uv sync --group dev
uv run ruff check tools
uv run ruff format tools
uv run python -m unittest discover -s tools/vault_sync/tests -v
```

## Usage from source

```bash
uv run python -m tools.vault_sync.cli init-config --preset auto --output ~/memento-vault-sync.toml
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml capabilities
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml import-documents all
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml import-databases all
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml import-sessions all
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml link-vault
uv run python -m tools.vault_sync.cli --config ~/memento-vault-sync.toml --json run-all
```

Homebrew installs the same CLI as `memento-vault-sync`.

Global options (`--config`, `--json`) precede the subcommand. A config can also
be selected with `MEMENTO_VAULT_SYNC_CONFIG`.

Read [the ingestion guide](../../docs/INGESTION.md) for source schemas, PDF
quality behavior, database security, linker semantics, and isolated validation.
The complete example is [config.example.toml](config.example.toml).
