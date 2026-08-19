# Security Policy

Memento indexes personal notes, documents, conversations, and database rows.
Treat the memory store as sensitive as the sources from which it was built.

## Supported versions

Security fixes are prioritized for the current `main` branch and latest minor
release line.

| Version | Supported |
| --- | --- |
| 0.2.x | yes |
| 0.1.x | no |
| older | no |

Pre-1.0 interfaces may evolve, but security fixes will not be intentionally held
for a breaking release.

## Report a vulnerability

Do **not** open a public issue for an undisclosed vulnerability.

Email [hello@arvor.co](mailto:hello@arvor.co) with:

- affected version or commit SHA
- operating system and installation method
- attack scenario and required access
- minimal reproduction steps using synthetic data
- impact on confidentiality, integrity, or availability
- suggested mitigation, if known
- whether you intend to request a CVE or publish details

Do not send a real vault, `.memento` store, credentials, tokens, or private
conversation exports. Encrypt sensitive proof-of-concept material and arrange a
safe transfer method first.

Maintainers will acknowledge the report as soon as practical, investigate,
coordinate a fix and disclosure, and credit the reporter when requested and
appropriate. Response time depends on severity and maintainer availability; no
fixed remediation SLA is promised at this stage.

## Security posture

Safe defaults:

- local Unix socket or Windows named-pipe transport
- no HTTP listener unless `--http-port` is explicit
- loopback-only HTTP host by default
- explicit `--allow-remote-http` for non-loopback binding
- bearer-token authentication on HTTP routes outside `/health`
- mode-`0600` generated HTTP token on Unix
- local `stdio` MCP transport
- bounded MCP query, excerpt, result, and document-page sizes
- exact indexed source requirement for MCP document reads
- read-only/query-only database imports
- external database credentials selected through environment variables
- no hosted dependency for core ingest, learn, or query

## Threat model

### Protected assets

- original local sources
- normalized vault content and provenance
- `.memento` snapshots, segments, manifests, and recovery state
- source paths and metadata
- database credentials and HTTP bearer tokens
- query text and retrieved evidence

### Primary threats

- accidental network exposure of private memory
- an agent reading more local content than the memory interface intends
- source/deletion bugs removing or corrupting indexed state
- untrusted document/converter input affecting local execution
- credentials leaking through config, logs, reports, or generated Markdown
- malicious or compromised local users reading a permissively stored data dir
- supply-chain tampering with release archives or package formulas

### Assumptions

- the operating-system account running Memento is trusted
- local filesystem permissions protect the user's home and data directory
- the MCP host is trusted to receive the evidence it requests
- external converter binaries and Python dependencies are installed from trusted
  sources
- database roles enforce read-only grants in addition to client safeguards

### Current non-goals

- hostile multi-user isolation on one operating-system account
- multi-tenant hosted authorization
- anonymous or internet-exposed serving
- end-to-end encryption of a running local store
- sandboxing every third-party document converter
- deciding whether a cloud-backed MCP host retains tool output

## Operator guidance

### Local store

- Keep `MEMENTO_DATA_DIR` in a user-private directory.
- Do not synchronize `.memento` through a consumer cloud drive without
  understanding its content and conflict behavior.
- Back up the store and maintained vault before upgrades or deletion-rule
  changes.
- Never attach a raw store to a public issue.

### Sources and ingestion

- Review `.mementoignore` before a large folder or Obsidian sync.
- Exclude `.env`, private keys, credential files, and secret-bearing config;
  direct discovery supports technical files and does not infer which are safe.
- Test new feeder sources with an isolated vault and run them twice.
- Treat PDFs, Office files, archives, and exports as untrusted input.
- Run converters with user-level privileges, not root.
- Inspect generated Markdown before indexing sensitive automated sources.

### Databases

- Use a dedicated account with server-enforced read-only grants.
- Put the DSN in the environment variable named by `dsn_env`, never TOML.
- Restrict queries to the minimum columns/rows required for memory.
- Remember that converted rows may reproduce sensitive database content in the
  vault and runtime store.

### MCP

- Keep search/status read-only and require host approval for sync.
- Use exact source pagination after search; avoid large evidence budgets by
  default.
- Evaluate the MCP host's own privacy/data-retention policy.
- Use a separate `MEMENTO_DATA_DIR` for agents with a narrower memory scope.

### HTTP

- Prefer the Unix socket or MCP whenever possible.
- Keep HTTP on `127.0.0.1`/`::1`.
- Treat `--allow-remote-http` as an expert escape hatch, not a normal deployment.
- Store `MEMENTO_HTTP_TOKEN` as a secret and rotate it after suspected exposure.
- Do not place the token in shell history, source files, issue logs, or URLs.
- Add external transport encryption and network controls before any remote use;
  bearer auth alone does not provide TLS.

### Releases

- Verify `SHA256SUMS` for downloaded archives.
- Verify GitHub artifact attestations when possible.
- Install the checksum-pinned Homebrew formula from the official tap/release.
- Do not install unreviewed formula files or pipe remote scripts into a shell.

## Data deletion

Uninstalling binaries does not delete `~/.memento`, a maintained vault, or
original sources. This protects against accidental loss but means sensitive
derived data persists until the operator removes it deliberately.

Source sync and feeder `delete_removed` behavior remove only state/output owned
by their manifests. Test retention changes on synthetic data and back up first.

## Security-related changes

Contributions affecting transport, authentication, parsing, source deletion,
format compatibility, converter execution, secrets, or memory exposure must:

- include adversarial tests where practical
- document threat-model impact
- preserve safe defaults
- update this policy and relevant operator docs
- avoid logging source content or secrets

See [CONTRIBUTING.md](CONTRIBUTING.md) and the full
[architecture](docs/ARCHITECTURE.md#transport-and-trust-boundaries).
