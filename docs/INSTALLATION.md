# Installation

> Install the complete local stack: operator CLI, daemon, vault feeder, and MCP
> server.

[← Documentation](README.md) · [Quick start](QUICKSTART.md) ·
[Configuration](CONFIGURATION.md) · [Releasing](RELEASING.md)

## Platform support

Release automation builds these targets:

| Operating system | Architecture | Release target |
| --- | --- | --- |
| macOS | Apple Silicon | `aarch64-apple-darwin` |
| macOS | Intel | `x86_64-apple-darwin` |
| Linux | arm64 | `aarch64-unknown-linux-gnu` |
| Linux | x86_64 | `x86_64-unknown-linux-gnu` |

Windows feeder presets exist, but the Unix-socket daemon is not yet a supported
release target. A native Windows transport needs to be productized before the
full stack can be advertised there.

## What gets installed

| Command | Purpose |
| --- | --- |
| `memento` | Setup, health, ingest, sync, learn, query, status |
| `mementod` | Local background memory runtime |
| `memento-vault-sync` | Python source-to-vault feeder |
| `memento-mcp` | Local stdio MCP server for agents |
| `memento-agent-install` | Install or repair the portable skill and host integration |

Release packages also carry the canonical
[`memento-runtime` skill](../.agents/skills/memento-runtime/SKILL.md). The helper
copies it into the discovery path for the selected agent rather than requiring
each integration to maintain a divergent prompt.

Memento writes no user store during package installation. `memento init` creates
the runtime under `~/.memento` and a vault only at the path you select.

## Install from an AI agent

Paste the repository's [agent installation prompt](../AGENT_INSTALL.md) into
Codex, Claude Code, OpenClaw, or another shell-capable agent. The agent inspects
and runs `scripts/install.sh`, which can:

- preserve an existing installation or install via Homebrew, release, or source
- install the same canonical skill for one or more agent hosts
- choose `mcp`, `cli`, `both`, or host-aware `auto` integration
- initialize only the vault path explicitly selected by the user
- verify program versions, runtime health, host registration, and retrieval

After a Homebrew installation, repair or add another host without reinstalling
the program:

```bash
memento-agent-install \
  --agent claude-code \
  --integration mcp \
  --program skip
```

Use `--dry-run` to preview mutations. The exact host paths and prompt contract
are documented in [AGENT_INSTALL.md](../AGENT_INSTALL.md).

## Homebrew

Homebrew is the recommended macOS and Linux installation path:

```bash
brew install ArvorCo/tap/memento
```

The fully qualified name lets Homebrew 6+ trust only this formula instead of
every current and future item in the third-party tap. Review the formula before
granting that trust. See Homebrew's [Tap Trust](https://docs.brew.sh/Tap-Trust)
documentation.

Verify the package:

```bash
memento --version
mementod --version
memento-mcp --version
memento-vault-sync --help
memento-agent-install --help
```

Let Homebrew own the daemon, generate configuration, then restart once so the
scheduler loads that configuration:

```bash
brew services start memento
memento init --vault-root "$HOME/Documents/MyVault"
brew services restart memento
memento doctor
memento status
```

Starting the service first also prevents onboarding from launching a separate
background daemon. The restart is required because scheduler configuration is
read during daemon startup.

### Conversion dependencies

The formula declares:

| Dependency | Formula policy | Used for |
| --- | --- | --- |
| Python 3.12 | required | Vault feeder runtime |
| Pandoc | recommended | Office, e-book, notebook, and markup conversion |
| Poppler | recommended | High-quality PDF layout extraction |
| Tesseract | optional, manual | Image-only PDF OCR |
| LibreOffice | optional, manual | Legacy `.doc` conversion fallback |

Inspect what the current machine can convert:

```bash
memento-vault-sync --config ~/.memento/config/vault_sync.toml capabilities
```

Install OCR only when needed:

```bash
brew install tesseract
```

Additional language packs are platform-specific.

## Prebuilt release archive

Each GitHub release contains one `.tar.gz` per supported target, a
`SHA256SUMS` file, a generated `memento.rb`, and GitHub artifact attestations.

1. Download the archive and `SHA256SUMS` from
   [GitHub Releases](https://github.com/ArvorCo/memento/releases).
2. Verify the checksum.
3. Verify GitHub artifact provenance when `gh` is available.
4. Extract and install the three Rust binaries, feeder, installer, and skill.

Linux checksum example:

```bash
sha256sum --check SHA256SUMS --ignore-missing
```

macOS checksum example:

```bash
shasum -a 256 memento-v0.1.0-aarch64-apple-darwin.tar.gz
grep 'memento-v0.1.0-aarch64-apple-darwin.tar.gz' SHA256SUMS
```

Provenance example:

```bash
gh attestation verify memento-v0.1.0-aarch64-apple-darwin.tar.gz \
  --repo ArvorCo/memento
```

After extraction, install binaries on your `PATH`:

```bash
install -m 755 memento mementod memento-mcp "$HOME/.local/bin/"
```

Install the bundled skill and selected host integration from that extracted
directory:

```bash
MEMENTO_SKILL_SOURCE="$PWD/.agents/skills/memento-runtime" \
  scripts/install.sh \
    --program skip \
    --agent codex \
    --integration mcp \
    --skip-init
```

The archive's `tools/vault_sync` source can be run with Python 3.12 from the
extracted tree. Homebrew is preferred when you want a packaged
`memento-vault-sync` wrapper and managed Python dependency.

### Install a release formula directly

When a release exists but the tap has not updated, download its generated
formula and install it locally:

```bash
brew install ./memento.rb
```

Inspect downloaded formulas and verify the attached checksums before installing.

## Build from source

Requirements:

- Rust 1.88 or newer
- Git
- Python 3.12 plus `uv` for the feeder
- Node.js 22 only when building the web app

```bash
git clone https://github.com/ArvorCo/memento.git
cd memento
cargo build --release --locked \
  -p memento-cli \
  -p mementod \
  -p memento-mcp
```

Install the Rust binaries:

```bash
mkdir -p "$HOME/.local/bin"
install -m 755 \
  target/release/memento \
  target/release/mementod \
  target/release/memento-mcp \
  "$HOME/.local/bin/"
```

Ensure `$HOME/.local/bin` is on `PATH`. For feeder development/use from the
checkout:

```bash
uv sync --group dev
uv run python -m tools.vault_sync.cli --help
```

Then initialize:

```bash
memento init --vault-root "$HOME/Documents/MyVault"
memento doctor
memento status
```

Source builds do not install a system service. Use foreground mode during
development, or create a user service that invokes the exact release binary and
sets the intended `MEMENTO_DATA_DIR`.

## Smoke test in an isolated store

This verifies the installed binaries without touching a daily memory:

```bash
runtime_dir="$(mktemp -d /tmp/memento-install.XXXXXX)"
vault_dir="$(mktemp -d /tmp/memento-vault.XXXXXX)"

export MEMENTO_DATA_DIR="$runtime_dir"
printf '# Install test\n\nThe verification phrase is amber telescope.\n' \
  > "$vault_dir/test.md"

memento init --vault-root "$vault_dir" --force
memento doctor
memento sync obsidian "$vault_dir" --json
memento query "amber telescope" --output compact
```

Success means the daemon started, the source was indexed, and the unique phrase
returned with its source path.

## Upgrade

Homebrew:

```bash
brew update
brew upgrade ArvorCo/tap/memento
brew services restart memento
memento doctor
memento status
```

Before a minor/major upgrade:

1. read [CHANGELOG.md](../CHANGELOG.md)
2. stop automated syncs during the upgrade window
3. back up the data directory and maintained vault
4. upgrade all packaged binaries together
5. restart and inspect runtime readiness
6. run a known query that checks source provenance

The current release maintains the compatible `default.memento` snapshot plus
versioned runtime segments, but pre-1.0 storage contracts may evolve.

## Uninstall

Stop the Homebrew service and remove the package:

```bash
brew services stop memento
brew uninstall memento
```

Uninstalling the formula does **not** remove:

- `~/.memento`
- your configured vault
- feeder-generated Markdown in that vault
- any original source material

This is deliberate data preservation. Review and back up those paths before
removing them manually. They may contain sensitive memory and are not safely
recoverable from the package.

## Next step

Continue with the [Quick start](QUICKSTART.md), or use
[Troubleshooting](TROUBLESHOOTING.md#installation) if command verification fails.
