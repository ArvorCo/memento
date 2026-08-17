# Memento Documentation

> Everything needed to install, understand, operate, integrate, and contribute
> to Memento.

[← Project README](../README.md) · [Quick start](QUICKSTART.md) ·
[Troubleshooting](TROUBLESHOOTING.md) · [Contributing](../CONTRIBUTING.md)

## Choose your path

| I want to… | Begin here | Then read |
| --- | --- | --- |
| Install from inside an AI agent | [Agent installation](../AGENT_INSTALL.md) | [MCP integration](MCP.md) |
| Try Memento with an existing vault | [Quick start](QUICKSTART.md) | [CLI reference](CLI.md) |
| Build a personal brain from many sources | [Ingestion](INGESTION.md) | [Configuration](CONFIGURATION.md) |
| Connect an AI agent | [MCP integration](MCP.md) | [Examples](EXAMPLES.md#agent-memory-with-mcp) |
| Understand result quality | [Retrieval design](RETRIEVAL.md) | [Benchmarks](BENCHMARKS.md) |
| Diagnose a broken setup | [Troubleshooting](TROUBLESHOOTING.md) | [Configuration](CONFIGURATION.md) |
| Contribute code | [Development](DEVELOPMENT.md) | [Contributing](../CONTRIBUTING.md) |
| Prepare a release | [Releasing](RELEASING.md) | [Installation](INSTALLATION.md) |

## Tutorials

Tutorials take you from zero to a working outcome.

- **[Quick start](QUICKSTART.md)** — install, initialize, sync a vault, learn,
  query, and inspect evidence in about five minutes.
- **[Agent installation](../AGENT_INSTALL.md)** — paste one prompt into Codex,
  Claude Code, OpenClaw, or another compatible agent to install the program,
  skill, and local interface.
- **[Examples cookbook](EXAMPLES.md)** — complete recipes for documents,
  databases, AI sessions, isolated stores, automation, and agent memory.

## How-to guides

Task-focused guides for a working installation.

- **[Installation](INSTALLATION.md)** — agent-guided setup, Homebrew, archives,
  source builds, upgrades, and uninstall behavior.
- **[Ingestion and vault maintenance](INGESTION.md)** — direct import versus
  feeder pipelines, document conversion, read-only databases, connectors, and
  wiki hubs.
- **[MCP integration](MCP.md)** — connect local agents, choose approvals, bound
  context, and troubleshoot the protocol boundary.
- **[Troubleshooting](TROUBLESHOOTING.md)** — symptom-driven diagnostics for
  daemon, configuration, conversion, synchronization, and retrieval failures.

## Reference

Reference pages describe exact interfaces and configuration contracts.

- **[CLI reference](CLI.md)** — every `memento`, `mementod`,
  `memento-vault-sync`, and `memento-mcp` command.
- **[Configuration reference](CONFIGURATION.md)** — daemon, scheduler, feeder,
  connector, environment, and ignore-rule settings.
- **[Local HTTP API](HTTP_API.md)** — authenticated routes, request/response
  schemas, errors, and safe client behavior.
- **[Benchmark reference](BENCHMARKS.md)** — dataset schema, metrics,
  reproduction, and interpretation.

## Concepts and architecture

Explanations build a mental model of why Memento works the way it does.

- **[System architecture](ARCHITECTURE.md)** — components, trust boundaries,
  data flow, runtime storage, lifecycle, and extension points.
- **[Retrieval design](RETRIEVAL.md)** — candidate generation, ranking signals,
  graph propagation, learning, grounded answers, and confidence.
- **[Project vision](../VISION.md)** — product principles and long-term direction.
- **[Roadmap](../ROADMAP.md)** — current priorities and explicit non-goals.

## Project and contributor guides

- **[Development](DEVELOPMENT.md)** — workspace map, setup, validation,
  architecture rules, and change-specific test expectations.
- **[Contributing](../CONTRIBUTING.md)** — contribution workflow and pull
  request expectations.
- **[Security](../SECURITY.md)** — supported versions, disclosure, threat model,
  and operator guidance.
- **[Releasing](RELEASING.md)** — versioning, artifacts, attestations, and the
  Homebrew tap flow.
- **[Support](../SUPPORT.md)** — where to ask questions and report problems.
- **[Changelog](../CHANGELOG.md)** — user-visible release history.

## Documentation conventions

Examples use placeholders with deliberate meanings:

| Placeholder | Meaning |
| --- | --- |
| `$HOME/Documents/MyVault` | A user-controlled vault path |
| `~/.memento` | Default local runtime directory |
| `/absolute/path/to/store` | A path that must be absolute for an integration |
| `brain` | A friendly example name, never a hardcoded product directory |

Shell examples assume a POSIX-compatible shell. Paths should be quoted whenever
they may contain spaces. Commands prefixed with `cargo run` or `uv run` are for a
source checkout; installed commands omit those wrappers.

> [!TIP]
> Start with the [quick start](QUICKSTART.md). Read configuration reference only
> when you need to customize a source, schedule, or integration.
