# Support

Memento is early-stage open source software. Good reports help turn local-first
memory into a dependable tool without exposing anyone's private brain.

## Questions and usage help

Before opening an issue:

1. run `memento doctor`
2. read the [quick start](docs/QUICKSTART.md)
3. check the [troubleshooting guide](docs/TROUBLESHOOTING.md)
4. search existing [GitHub issues](https://github.com/ArvorCo/memento/issues)

For a reproducible bug, use the
[bug report template](https://github.com/ArvorCo/memento/issues/new?template=bug_report.yml).
For a focused capability proposal, use the
[feature request template](https://github.com/ArvorCo/memento/issues/new?template=feature_request.yml).

## Include in a bug report

- Memento version or commit SHA
- operating system and CPU architecture
- the smallest reproduction sequence
- expected and actual behavior
- sanitized `memento doctor` output
- whether the problem reproduces with an isolated `MEMENTO_DATA_DIR`

Never attach a real vault, raw memory store, credentials, access tokens, database
DSNs, or private benchmark results. Replace personal paths and content with
minimal fixtures.

## Security vulnerabilities

Do **not** open a public issue for an undisclosed vulnerability. Follow the
private reporting process in [SECURITY.md](SECURITY.md).

## Scope

Support is prioritized for the latest release and the current `main` branch on
the platforms listed in the [installation guide](docs/INSTALLATION.md). Memento
does not currently promise hosted operation, multi-tenant isolation, remote MCP,
or cloud synchronization as a primary workflow.
