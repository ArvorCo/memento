# Windows

> Run the complete local Memento stack on Windows 10/11 using native binaries,
> a private named pipe, and an optional isolated Python document feeder.

[← Documentation](README.md) · [Installation](INSTALLATION.md) ·
[Troubleshooting](TROUBLESHOOTING.md)

## Supported environments

Memento publishes native `x86_64-pc-windows-msvc` and
`aarch64-pc-windows-msvc` ZIP archives. PowerShell 5.1 or newer is the supported
installation shell. After installation, `memento.exe` and the generated
`memento-vault-sync.bat` wrapper also work from Command Prompt and Git Bash.

The runtime does not open a TCP port. CLI and MCP clients connect to `mementod`
through a local Windows named pipe. The pipe name is deterministic for the
absolute `MEMENTO_DATA_DIR`, so isolated stores cannot accidentally share a
daemon. Remote pipe clients are rejected.

## Install

Clone and inspect the installer before executing it:

```powershell
$checkout = Join-Path $env:TEMP ("memento-" + [Guid]::NewGuid())
git clone --depth 1 https://github.com/ArvorCo/memento.git $checkout
Set-Location $checkout
Get-Content .\scripts\install.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\install.ps1 `
  -Agent auto `
  -Integration auto
```

The default installation root is
`%LOCALAPPDATA%\Programs\Memento`. The installer downloads the matching release
ZIP, verifies it against the release `SHA256SUMS`, installs the runtime skill,
adds the binary directory to the current-user `PATH`, and preserves existing
runtime data.

The execution-policy override applies only to this PowerShell process. The
installer never changes the user's or machine's persistent execution policy.

Open a new terminal after installation, then verify:

```powershell
memento --version
mementod --version
memento-mcp --version
memento doctor
```

## Initialize and query

PowerShell:

```powershell
$vault = Join-Path $HOME "Documents\MyVault"
memento init --preset windows --vault-root $vault
memento sync obsidian $vault
memento learn
memento query "What did we decide?" --limit 5 --output compact
```

Git Bash can run the installed commands. Pass paths in a form Windows programs
understand when a tool does not translate them automatically:

```bash
vault="$(cygpath -w "$HOME/Documents/MyVault")"
memento.exe sync obsidian "$vault"
memento.exe query "What did we decide?" --limit 5 --output compact
```

Run the installation itself from PowerShell. `scripts/install.sh` intentionally
stops on MSYS, Cygwin, and Git Bash and points to `install.ps1`, avoiding mixed
Unix/Windows package and service semantics.

## Document feeder

Core file/folder/Obsidian sync is implemented by the Rust binaries. The optional
`memento-vault-sync` feeder handles heterogeneous sources and requires Python
3.12. Its native converters read PDF, DOCX, PPTX, XLSX, CSV, TSV, JSON, and
Jupyter notebooks without Microsoft Office or a GPU.

Install the feeder and its isolated environment automatically:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\install.ps1 `
  -Program skip `
  -Feeder always
```

When Python 3.12 is absent, `-Feeder always` asks WinGet to install it for the
current user. Add `-InstallConverters` to install optional Pandoc support. Use
`-Feeder never` for a Rust-only installation.

Inspect conversion support:

```powershell
memento-vault-sync --config "$HOME\.memento\config\vault_sync.toml" capabilities
```

Windows sources should use normal absolute paths in generated TOML. Memento's
`init-config --preset windows` command generates valid examples and stores
manifests under the selected runtime directory.

## Agent integrations

The installer can copy the portable runtime skill and register the local stdio
MCP server for Codex, Claude Code, and OpenClaw:

```powershell
memento-agent-install -Agent codex -Integration mcp -Program skip
codex mcp list
```

For project-scoped skill installation:

```powershell
memento-agent-install `
  -Agent claude-code `
  -Integration both `
  -Scope project `
  -ProjectDir (Get-Location).Path `
  -Program skip
```

All integrations must inherit the same `MEMENTO_DATA_DIR`. The MCP server uses
stdio with the agent host and the private named pipe with the daemon.

## Runtime files and diagnostics

The default store is `%USERPROFILE%\.memento`:

```text
.memento\
├── config\
│   ├── daemon.toml
│   └── vault_sync.toml
├── mementod.log
├── mementod.pid
└── ... indexed memory data
```

The named pipe exists in the Windows object namespace, not as a file inside the
store. Diagnose a failed daemon without deleting the store:

```powershell
memento doctor
Get-Content "$HOME\.memento\mementod.log" -Tail 100
mementod --foreground
```

Use a temporary isolated store for safe testing:

```powershell
$env:MEMENTO_DATA_DIR = Join-Path $env:TEMP "memento-test"
memento init --preset windows --vault-root (Join-Path $env:TEMP "memento-vault")
memento status
Remove-Item Env:MEMENTO_DATA_DIR
```

See [Troubleshooting](TROUBLESHOOTING.md#windows) for PATH, execution-policy,
named-pipe, and feeder failures.
