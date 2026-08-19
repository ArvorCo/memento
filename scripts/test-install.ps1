$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("memento-install-test-" + [Guid]::NewGuid())
$fakeBin = Join-Path $temporary "fake-bin"
$testHome = Join-Path $temporary "home"
$installRoot = Join-Path $temporary "install"
$project = Join-Path $temporary "project"
$commandLog = Join-Path $temporary "commands.log"
$coreBin = Join-Path $repoRoot "target\debug"

try {
    New-Item -ItemType Directory -Force -Path $fakeBin, $testHome, $project | Out-Null
    foreach ($name in @("codex", "claude", "openclaw")) {
        $content = @"
@echo off
echo $name %*>>"$commandLog"
exit /b 0
"@
        Set-Content -LiteralPath (Join-Path $fakeBin "$name.cmd") -Value $content -Encoding Ascii
    }

    $env:MEMENTO_INSTALL_HOME = $testHome
    $env:MEMENTO_INSTALL_ROOT = $installRoot
    $env:OPENCLAW_STATE_DIR = Join-Path $temporary "openclaw"
    $env:Path = "$fakeBin;$coreBin;$env:Path"

    & (Join-Path $repoRoot "scripts\install.ps1") `
        -Program skip `
        -Agent all `
        -Integration both `
        -Scope user `
        -Feeder never `
        -SkipInit `
        -NoPath

    if ($LASTEXITCODE -ne 0) { throw "user-scope installer test failed" }
    foreach ($path in @(
        (Join-Path $testHome ".agents\skills\memento-runtime\SKILL.md"),
        (Join-Path $testHome ".claude\skills\memento-runtime\SKILL.md"),
        (Join-Path $env:OPENCLAW_STATE_DIR "skills\memento-runtime\SKILL.md"),
        (Join-Path $installRoot "bin\memento-agent-install.cmd")
    )) {
        if (-not (Test-Path -LiteralPath $path)) { throw "missing installer output: $path" }
    }
    & (Join-Path $installRoot "bin\memento-agent-install.cmd") `
        -Program skip `
        -Agent generic `
        -Integration cli `
        -Feeder never `
        -SkipInit `
        -NoPath
    if ($LASTEXITCODE -ne 0) { throw "installed agent helper test failed" }
    $logged = Get-Content -LiteralPath $commandLog -Raw
    if ($logged -notmatch "codex mcp add .*memento") { throw "Codex MCP registration was not exercised" }
    if ($logged -notmatch "claude mcp add .*memento") { throw "Claude MCP registration was not exercised" }
    if ($logged -notmatch "openclaw mcp add memento") { throw "OpenClaw MCP registration was not exercised" }

    Clear-Content -LiteralPath $commandLog
    & (Join-Path $repoRoot "scripts\install.ps1") `
        -Program skip `
        -Agent all `
        -Integration cli `
        -Scope project `
        -ProjectDir $project `
        -Feeder never `
        -SkipInit `
        -NoPath

    if ($LASTEXITCODE -ne 0) { throw "project-scope installer test failed" }
    if (-not (Test-Path -LiteralPath (Join-Path $project ".agents\skills\memento-runtime\SKILL.md"))) {
        throw "generic project skill is missing"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $project ".claude\skills\memento-runtime\SKILL.md"))) {
        throw "Claude project skill is missing"
    }
    if ((Get-Content -LiteralPath $commandLog -Raw) -match "mcp add") {
        throw "CLI-only installation unexpectedly changed MCP configuration"
    }

    & (Join-Path $repoRoot "scripts\install.ps1") `
        -Program skip `
        -Agent generic `
        -Integration cli `
        -Scope user `
        -Feeder always `
        -SkipInit `
        -NoPath
    if ($LASTEXITCODE -ne 0) { throw "document feeder installation failed" }
    $feeder = Join-Path $installRoot "bin\memento-vault-sync.bat"
    if (-not (Test-Path -LiteralPath $feeder)) { throw "document feeder wrapper is missing" }
    & $feeder --help
    if ($LASTEXITCODE -ne 0) { throw "document feeder wrapper test failed" }

    Write-Host "Windows agent installer tests passed."
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
