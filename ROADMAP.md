# Roadmap

Memento's north star is a fast, trustworthy, local memory substrate for people
and agents. Work is prioritized by measured improvement to the
`ingest -> learn -> query` loop.

## Now: reliable local memory

- keep `.memento` format roundtrips compatible and tested
- expand multilingual lexical normalization and proximity ranking
- add adversarial retrieval suites for stale, conflicting, and similarly named notes
- make source configuration and background scheduling fully CLI-managed
- keep query latency bounded as vaults grow beyond 100,000 documents

## Next: agent integration

- evidence-only query mode for strict downstream agents
- stable JSON output and local integration examples
- MCP surface that preserves local-first permissions and provenance
- incremental graph maintenance instead of full graph rebuilds after large syncs

## Later: optional synchronization

- encrypted, user-controlled synchronization of `.memento` stores
- explicit multi-device conflict and migration semantics
- shared memory only where ownership and access controls are understandable

## Non-goals

- requiring a GPU, hosted embedding API, or proprietary LLM for core retrieval
- opening a network listener by default
- hiding benchmark regressions behind fluent answer text
- turning the core engine into a cloud-only product

See [VISION.md](VISION.md) for product principles and
[docs/BENCHMARKS.md](docs/BENCHMARKS.md) for the current measured baseline.
