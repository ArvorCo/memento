## What changed

Describe the user-visible outcome and the smallest architectural slice involved.

## Why

Link the issue or explain the concrete ingest, retrieval, runtime, or release problem.

## Verification

- [ ] `make check`
- [ ] web lint/build when `memento-web` changed
- [ ] optimized benchmark with quality and p50/p95 latency when retrieval changed
- [ ] docs updated when behavior, commands, or architecture changed
- [ ] no credentials, personal paths, vault content, or private benchmark reports added

## Security and compatibility

State any effect on the `.memento` format, network surface, authentication,
privacy, migrations, or release packaging. Write `none` when there is no effect.
