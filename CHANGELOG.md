# Changelog

All notable changes to Memento are documented here. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-08-17

### Fixed

- explicit `--program release|source|brew` installs now honor the selected
  method even when older Memento binaries already exist on `PATH`
- the generated Homebrew formula passes strict online audit and installs the
  runtime skill under `pkgshare`

### Changed

- GitHub Actions use current Node 24-based releases and CI audits both Rust and
  npm dependencies on every change
- release archives and Homebrew now include the MCP server, configurable vault
  feeder, agent installer, and canonical runtime skill
- folder and Obsidian discovery now include PDFs and reject binary Office files
  with actionable conversion guidance

### Security

- upgraded vulnerable Next.js, PDF parsing, cache, randomness, and concurrency
  dependencies; npm and RustSec now report zero known vulnerabilities

## [0.1.0] - 2026-08-17

### Added

- hash-incremental imports for documents, PDFs, tabular files, and read-only database queries
- hierarchical and tag-based vault hubs with idempotent Obsidian navigation
- token-bounded local MCP server with search, document pagination, status, sync, and learn tools
- compact/JSON CLI output and exact paginated access to ingested source documents
- a complete documentation portal with quick start, CLI/configuration reference,
  architecture and retrieval diagrams, ingestion recipes, troubleshooting,
  development, benchmark, security, and release guides
- automated validation for internal links, Markdown fences, Mermaid
  accessibility metadata, file size, and private-path hygiene
- portable `memento-runtime` Agent Skill for Codex, Claude Code, OpenClaw, and
  compatible AgentSkills hosts
- idempotent agent installer with Homebrew/release/source program methods,
  user/project skill scopes, and MCP/CLI integration modes
- copy-paste installation prompt with end-to-end runtime and retrieval proof
- local daemon and CLI for import, sync, learn, query, status, and diagnostics
- rebuildable BM25-style postings index with metadata and multilingual query bridges
- Obsidian wikilink graph with query-local personalized PageRank
- bounded, citation-aware extractive answer synthesis
- incremental source sync, runtime manifests, checkpoints, and recovery snapshots
- release benchmarks, cross-platform binary packaging, and Homebrew formula generation

### Changed

- query work is bounded by an indexed candidate pool instead of scanning the vault
- exact dates and rare metadata constraints are protected from semantic expansion
- learning runs outside the daemon's state lock so status remains responsive
- confidence now reflects retrieval strength, coverage, rank margin, and matrix coherence

### Security

- Unix socket remains the default transport
- optional HTTP is loopback-only unless remote access is explicitly allowed
- HTTP requests require the generated bearer token outside the health endpoint

[Unreleased]: https://github.com/ArvorCo/memento/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ArvorCo/memento/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ArvorCo/memento/releases/tag/v0.1.0
