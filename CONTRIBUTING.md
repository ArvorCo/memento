# Contributing to Memento

Thank you for helping build a fast, trustworthy, local-first memory substrate
for people and agents.

By participating, you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
Unless explicitly stated otherwise, submitted contributions are licensed under
the project's dual `MIT OR Apache-2.0` terms.

## Start with the product contract

Memento's priority is the real local loop:

```text
ingest → sync → learn → query → traceable evidence
```

Contributions should preserve these principles:

- core operation remains useful without a hosted service
- a GPU or proprietary embedding API is never required for core retrieval
- provenance and exact source evidence survive every layer
- retrieval quality is measured, not inferred from plausible prose
- network exposure is opt-in and authenticated
- user-controlled files and stores are treated as sensitive data

Read the [architecture](docs/ARCHITECTURE.md),
[retrieval design](docs/RETRIEVAL.md), and [roadmap](ROADMAP.md) before proposing a
large architectural change.

## Find or propose work

1. Search existing [issues](https://github.com/ArvorCo/memento/issues) and pull
   requests.
2. For a bug, create the smallest synthetic reproduction and include sanitized
   `memento doctor` output.
3. For a feature, explain the user problem, the smallest useful behavior, and
   how success can be measured.
4. For a broad format, retrieval, transport, or architecture change, open an
   issue before investing in a large implementation.

Small fixes with clear scope can go directly to a pull request. Never use a
public issue for an undisclosed vulnerability; follow [SECURITY.md](SECURITY.md).

## Development setup

The complete environment, workspace map, and change-specific test expectations
are in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

Baseline setup:

```bash
git clone https://github.com/ArvorCo/memento.git
cd memento
uv sync --group dev
cargo test --workspace
```

Use an isolated store while developing:

```bash
export MEMENTO_DATA_DIR="$(mktemp -d /tmp/memento-dev.XXXXXX)"
memento init --vault-root "$(mktemp -d /tmp/memento-vault.XXXXXX)" --force
```

Do not point unfinished ingest/deletion code at a personal vault.

## Make a focused change

- Prefer a small vertical slice that leaves the repository runnable.
- Put reusable Rust behavior in `libmemento`; keep process orchestration in
  `mementod` and presentation in the CLI/MCP/web interfaces.
- Put external source normalization in `tools/vault_sync` when inspectable
  Markdown is the clean boundary.
- Keep files cohesive and refactor before they exceed roughly 1,000 lines.
- Preserve user work already present in a dirty checkout.
- Add tests for first run, repeated run, update, deletion, error, and Unicode
  behavior as relevant.
- Update documentation when commands, config, architecture, security, or output
  contracts change.

Avoid speculative frameworks, cloud-only assumptions, generic product copy, and
weight tuning without a regression case.

## Validation

Run the full default gate:

```bash
make check
git diff --check
```

This covers Rust format/Clippy/tests, Python lint/tests, shell syntax, and public
documentation validation.

When the web app changes:

```bash
cd memento-web
npm ci
npm run lint
npm run build
```

When release packaging changes:

```bash
make release-check
```

## Retrieval changes

Every ranking, chunking, graph, learning, or answer-composition change needs a
case that fails before the change and succeeds after it.

Run an optimized representative benchmark:

```bash
cargo run --release -p memento-research -- benchmark run \
  --dataset /absolute/path/to/benchmark.jsonl \
  --top-k 10 \
  --report /tmp/memento-benchmark.json
```

Report:

- hit@k and MRR
- result-term and answer-term recall when relevant
- p50 and p95 query latency
- per-case regressions
- hardware/build profile and dataset size

Never commit a personal corpus or a report containing private paths/content.
See [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Storage and security changes

If a change affects `.memento` format, manifests, recovery, transport,
authentication, source deletion, or memory exposure:

- describe compatibility and migration behavior
- test restart and interrupted operations
- update the architecture/security documentation
- state the threat-model effect in the pull request
- preserve safe defaults

## Documentation changes

The root README is a concise product and first-success page. Put detailed tasks
and contracts in `docs/`:

- tutorial for a learning journey
- how-to for one task
- reference for exact interfaces
- explanation for architecture and design

Use relative repository links and add `accTitle` plus `accDescr` to every
Mermaid diagram. Run:

```bash
make docs-check
```

## Pull request expectations

A good pull request contains:

- a user-visible outcome, not only implementation details
- the reason and smallest architectural slice involved
- focused tests and exact verification results
- benchmark evidence for retrieval/performance changes
- documentation for user-visible behavior
- explicit format, security, privacy, and compatibility impact

Keep unrelated refactors out of the same pull request. Review comments should
be direct, actionable, and about the work—not the person.

## Privacy

Do not commit or attach:

- real vaults, `.memento` stores, or recovery snapshots
- conversation exports or database dumps
- API keys, bearer tokens, DSNs, or private keys
- personal absolute paths
- private benchmark cases/reports

Use synthetic fixtures with distinctive invented facts. The
[troubleshooting guide](docs/TROUBLESHOOTING.md#collect-a-privacy-safe-report)
shows what a safe report contains.

## Getting help

Read [SUPPORT.md](SUPPORT.md) for usage and issue channels. Maintainers may ask
for a smaller reproduction, benchmark evidence, or architectural discussion
before merging a high-risk change.
