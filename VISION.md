# Memento Vision

## The idea

Memento is a local-first memory layer for people and AI agents.

The promise is simple:

1. ingest real work from user-controlled sources
2. consolidate it into durable local memory
3. retrieve the right evidence quickly
4. answer with provenance and visible uncertainty

## The problem

Useful context already exists in notes, project history, conversations,
documents, and databases. People repeatedly search for it; agents repeatedly
ask for it or consume oversized context windows.

Many memory systems solve this by sending data to a hosted vector service or by
depending on opaque neural similarity. That can be useful, but it should not be
the only architecture available for personal memory.

Memento aims to make local language structure—exact terms, metadata, dates,
links, co-occurrence, and document relationships—fast enough and accurate
enough to become a dependable memory substrate on ordinary hardware.

## Product truth

The current center of gravity is:

- `libmemento`: shared memory engine and storage primitives
- `mementod`: local state owner and query runtime
- `memento`: operator interface
- `memento-mcp`: bounded local agent integration
- `memento-vault-sync`: configurable source normalization
- `memento-research`: measurement and retrieval diagnostics

Cloud-facing or collaborative surfaces are optional extensions. They do not
define the core.

## Principles

### Local first

The default system runs on the user's machine. A core ingest → learn → query
workflow must not require a hosted backend.

### CPU first

Exact and structured retrieval must work without a GPU. Optional accelerators
may improve future research, but they cannot become a hidden baseline
requirement.

### Provenance over plausibility

Results point back to source paths and evidence. If retrieval is weak or
ambiguous, the system should reveal that rather than compensate with confident
prose.

### Retrieval before presentation

A polished answer is only valuable when the right evidence was found. Candidate
recall, ranking, latency, source identity, and benchmark quality take priority.

### Structure is knowledge

Titles, paths, headings, frontmatter, dates, wikilinks, and backlinks encode
human intent. Memento should use that structure instead of flattening every
document into interchangeable vectors.

### Safe by default

Installation does not create a remote listener. Local transport, bounded agent
tools, explicit mutation approval, read-only database access, and user-owned
configuration are architectural requirements.

### Inspectable evolution

The memory format, normalized vault, benchmark cases, ranking signals, and
runtime status should be understandable enough to debug and improve. Learned
components earn their place through measured value.

## What Memento must do exceptionally well

### Ingest

- accept real local sources without fragile one-off scripts
- normalize heterogeneous material into inspectable, traceable documents
- preserve source identity and ownership
- repeat safely without duplicate churn
- scale beyond toy vaults

### Learn

- derive corpus-specific relationships on ordinary CPUs
- publish durable, restart-safe runtime state
- preserve exact lexical recall when learned dimensionality changes
- make computation and readiness observable

### Query

- retrieve exact identifiers, dates, entities, and concepts
- traverse deliberate document relationships
- rank canonical and episodic sources according to query intent
- return bounded, document-aware evidence
- expose confidence as a retrieval signal, not a truth guarantee

### Integrate

- give scripts stable JSON
- give agents narrow MCP tools and exact-source pagination
- keep memory ownership local even when a downstream AI is remote
- let users isolate stores for different trust scopes

## Near-term direction

The project is healthy when this loop is reliable and measurable:

```text
source → normalized document → incremental sync → learned runtime →
bounded retrieval → grounded evidence
```

Near-term work favors:

- adversarial retrieval suites
- larger-corpus performance
- stronger multilingual lexical analysis
- incremental graph/runtime maintenance
- stable format and integration contracts
- excellent installation and operator experience

## Long-term direction

Memento can become a shared open foundation for:

- personal knowledge and decision memory
- durable context for developer agents
- private research and operational recall
- local applications that need evidence-aware language search
- optional encrypted multi-device or collaborative memory

The path there is not to imitate a cloud vector database. It is to make a local,
inspectable language index so reliable that people and agents can treat it as
memory infrastructure.

## Success

Memento succeeds when:

- a new user can install it and retrieve a known fact in minutes
- repeated source maintenance is boring, incremental, and safe
- agents recover relevant context with fewer tokens and explicit citations
- exact evidence is not lost to semantic approximation
- retrieval improvements are demonstrated with reproducible quality/latency
  evidence
- contributors can understand the architecture and extend it without requiring
  private infrastructure

See the [system architecture](docs/ARCHITECTURE.md),
[retrieval design](docs/RETRIEVAL.md), and [roadmap](ROADMAP.md).
