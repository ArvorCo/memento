#Requires -Version 5.1

[CmdletBinding()]
param(
    [ValidateSet("auto", "release", "source", "skip")]
    [string]$Program = "auto",

    [ValidateSet("auto", "codex", "claude-code", "openclaw", "generic", "all")]
    [string[]]$Agent = @("auto"),

    [ValidateSet("auto", "mcp", "cli", "both")]
    [string]$Integration = "auto",

    [ValidateSet("user", "project")]
    [string]$Scope = "user",

    [string]$ProjectDir = (Get-Location).Path,
    [string]$Vault = "",
    [string]$DataDir = "",
    [string]$InstallRoot = "",
    [string]$Version = "latest",

    [ValidateSet("auto", "always", "never")]
    [string]$Feeder = "auto",

    [switch]$InstallConverters,
    [switch]$SkipInit,
    [switch]$NoPath,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$MementoRepository = "ArvorCo/memento"
$SkillName = "memento-runtime"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$UserHome = if ($env:MEMENTO_INSTALL_HOME) {
    $env:MEMENTO_INSTALL_HOME
} else {
    [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
}
if (-not $InstallRoot) {
    $InstallRoot = if ($env:MEMENTO_INSTALL_ROOT) {
        $env:MEMENTO_INSTALL_ROOT
    } else {
        Join-Path $env:LOCALAPPDATA "Programs\Memento"
    }
}
$InstallRoot = [IO.Path]::GetFullPath($InstallRoot)
$BinDir = Join-Path $InstallRoot "bin"
$LibDir = Join-Path $InstallRoot "lib\memento"
$ShareDir = Join-Path $InstallRoot "share\memento"
$CanonicalSkill = Join-Path $ShareDir "skills\$SkillName"
$ProgramSource = "existing"
$InstalledDestinations = @{}
$ResolvedAgents = @()
$TemporaryRoot = ""

function Write-MementoLog([string]$Message) {
    Write-Host "[memento] $Message"
}

function Write-MementoWarning([string]$Message) {
    Write-Warning "[memento] $Message"
}

function Format-Command([string]$Executable, [string[]]$Arguments) {
    $parts = @($Executable) + $Arguments
    return ($parts | ForEach-Object {
        if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
    }) -join " "
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [string[]]$Arguments = @(),
        [switch]$AllowFailure
    )
    Write-MementoLog ("+ " + (Format-Command $Executable $Arguments))
    if ($DryRun) {
        return 0
    }
    & $Executable @Arguments
    $status = $LASTEXITCODE
    if ($status -ne 0 -and -not $AllowFailure) {
        throw "$Executable exited with status $status"
    }
    return $status
}

function Resolve-Application([string]$Name) {
    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $command) { return $null }
    return $command.Source
}

function Test-CoreAvailable {
    return $null -ne (Resolve-Application "memento") -and
        $null -ne (Resolve-Application "mementod") -and
        $null -ne (Resolve-Application "memento-mcp")
}

function New-Directory([string]$Path) {
    if ($DryRun) {
        Write-MementoLog "+ New-Item -ItemType Directory -Force `"$Path`""
        return
    }
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
}

function Copy-DirectoryContents([string]$Source, [string]$Destination) {
    New-Directory $Destination
    if ($DryRun) {
        Write-MementoLog "+ Copy-Item -Recurse -Force `"$Source\*`" `"$Destination`""
        return
    }
    Get-ChildItem -LiteralPath $Source -Force | Copy-Item -Destination $Destination -Recurse -Force
}

function Resolve-Version {
    if ($Version -ne "latest") {
        return $Version.TrimStart("v")
    }
    Write-MementoLog "resolving latest stable release"
    $release = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$MementoRepository/releases/latest"
    return ([string]$release.tag_name).TrimStart("v")
}

function Resolve-WindowsTarget {
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    switch -Regex ($architecture) {
        "ARM64" { return "aarch64-pc-windows-msvc" }
        "AMD64|x86_64" { return "x86_64-pc-windows-msvc" }
        default { throw "Unsupported Windows architecture: $architecture" }
    }
}

function Install-Release {
    $resolvedVersion = Resolve-Version
    $target = Resolve-WindowsTarget
    $archiveName = "memento-v$resolvedVersion-$target.zip"
    $baseUrl = "https://github.com/$MementoRepository/releases/download/v$resolvedVersion"
    $script:TemporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("memento-install-" + [Guid]::NewGuid())
    $archivePath = Join-Path $TemporaryRoot $archiveName
    $checksumsPath = Join-Path $TemporaryRoot "SHA256SUMS"
    $payloadPath = Join-Path $TemporaryRoot "payload"
    New-Directory $TemporaryRoot

    if ($DryRun) {
        Write-MementoLog "would download, verify, and install release $resolvedVersion for $target"
        $script:ProgramSource = "release"
        return
    }

    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/SHA256SUMS" -OutFile $checksumsPath
    $checksumLine = Get-Content -LiteralPath $checksumsPath | Where-Object { $_ -match [Regex]::Escape($archiveName) } | Select-Object -First 1
    if (-not $checksumLine -or $checksumLine -notmatch '^([0-9A-Fa-f]{64})\s+\*?(.+)$') {
        throw "SHA256SUMS does not contain $archiveName"
    }
    $expected = $Matches[1].ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "checksum mismatch for $archiveName (expected $expected, got $actual)"
    }
    Expand-Archive -LiteralPath $archivePath -DestinationPath $payloadPath -Force
    Install-Payload $payloadPath
    $script:ProgramSource = "release"
}

function Install-Payload([string]$PayloadRoot) {
    New-Directory $BinDir
    foreach ($binary in @("memento.exe", "mementod.exe", "memento-mcp.exe")) {
        Copy-Item -LiteralPath (Join-Path $PayloadRoot $binary) -Destination (Join-Path $BinDir $binary) -Force
    }
    Copy-DirectoryContents (Join-Path $PayloadRoot "tools") (Join-Path $LibDir "tools")
    Copy-Item -LiteralPath (Join-Path $PayloadRoot "scripts\install.ps1") -Destination (Join-Path $LibDir "install.ps1") -Force
    Copy-DirectoryContents (Join-Path $PayloadRoot ".agents\skills\$SkillName") $CanonicalSkill
}

function Install-Source {
    $cargo = Resolve-Application "cargo"
    if (-not $cargo) { throw "cargo is required for --Program source" }
    Invoke-External $cargo @("build", "--release", "--locked", "-p", "memento-cli", "-p", "mementod", "-p", "memento-mcp") | Out-Null
    if (-not $DryRun) {
        Install-PayloadFromRepository
    }
    $script:ProgramSource = "source"
}

function Install-PayloadFromRepository {
    New-Directory $BinDir
    foreach ($binary in @("memento.exe", "mementod.exe", "memento-mcp.exe")) {
        Copy-Item -LiteralPath (Join-Path $RepoRoot "target\release\$binary") -Destination (Join-Path $BinDir $binary) -Force
    }
    Copy-DirectoryContents (Join-Path $RepoRoot "tools") (Join-Path $LibDir "tools")
    Copy-Item -LiteralPath (Join-Path $RepoRoot "scripts\install.ps1") -Destination (Join-Path $LibDir "install.ps1") -Force
    Copy-DirectoryContents (Join-Path $RepoRoot ".agents\skills\$SkillName") $CanonicalSkill
}

function Install-Program {
    switch ($Program) {
        "skip" {
            if (-not (Test-CoreAvailable) -and -not $DryRun) {
                throw "--Program skip requires memento.exe, mementod.exe, and memento-mcp.exe on PATH"
            }
            return
        }
        "source" { Install-Source; return }
        "release" { Install-Release; return }
        "auto" {
            if (Test-CoreAvailable) {
                Write-MementoLog "core binaries already exist on PATH; preserving them"
                return
            }
            Install-Release
        }
    }
}

function Install-SupportFiles {
    $repositorySkill = Join-Path $RepoRoot ".agents\skills\$SkillName"
    if (-not (Test-Path -LiteralPath $CanonicalSkill) -and (Test-Path -LiteralPath $repositorySkill)) {
        Copy-DirectoryContents $repositorySkill $CanonicalSkill
    }
    if (-not (Test-Path -LiteralPath (Join-Path $LibDir "install.ps1")) -and (Test-Path -LiteralPath $PSCommandPath)) {
        New-Directory $LibDir
        if (-not $DryRun) { Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $LibDir "install.ps1") -Force }
    }
    $repositoryTools = Join-Path $RepoRoot "tools"
    if (-not (Test-Path -LiteralPath (Join-Path $LibDir "tools\vault_sync\requirements.txt")) -and (Test-Path -LiteralPath $repositoryTools)) {
        Copy-DirectoryContents $repositoryTools (Join-Path $LibDir "tools")
    }
    if (Test-Path -LiteralPath (Join-Path $LibDir "install.ps1")) {
        New-Directory $BinDir
        $helper = @"
@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$LibDir\install.ps1" %*
"@
        if (-not $DryRun) { Set-Content -LiteralPath (Join-Path $BinDir "memento-agent-install.cmd") -Value $helper -Encoding Ascii }
    }
}

function Add-UserPath {
    $env:Path = "$BinDir;$env:Path"
    if ($NoPath -or $DryRun) { return }
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = if ($userPath) { $userPath -split ';' } else { @() }
    if ($entries | Where-Object { $_.TrimEnd('\') -ieq $BinDir.TrimEnd('\') }) { return }
    $updated = if ($userPath) { "$userPath;$BinDir" } else { $BinDir }
    [Environment]::SetEnvironmentVariable("Path", $updated, "User")
    Write-MementoLog "added $BinDir to the current-user PATH"
}

function Find-Python {
    $launcher = Resolve-Application "py"
    if ($launcher) {
        & $launcher -3.12 -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 12) else 1)" 2>$null
        if ($LASTEXITCODE -eq 0) { return [PSCustomObject]@{ Executable = $launcher; Prefix = @("-3.12") } }
    }
    foreach ($name in @("python.exe", "python3.exe")) {
        $python = Resolve-Application $name
        if ($python) {
            & $python -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 12) else 1)" 2>$null
            if ($LASTEXITCODE -eq 0) { return [PSCustomObject]@{ Executable = $python; Prefix = @() } }
        }
    }
    $known = Join-Path $env:LOCALAPPDATA "Programs\Python\Python312\python.exe"
    if (Test-Path -LiteralPath $known) { return [PSCustomObject]@{ Executable = $known; Prefix = @() } }
    return $null
}

function Install-PythonIfNeeded {
    $python = Find-Python
    if ($python) { return $python }
    if ($Feeder -ne "always") { return $null }
    $winget = Resolve-Application "winget"
    if (-not $winget) { throw "Python 3.12 is missing and WinGet is unavailable; install Python 3.12 or use -Feeder never" }
    Invoke-External $winget @(
        "install", "--id", "Python.Python.3.12", "--exact", "--scope", "user", "--silent",
        "--accept-package-agreements", "--accept-source-agreements"
    ) | Out-Null
    if ($DryRun) { return $null }
    $python = Find-Python
    if (-not $python) { throw "Python 3.12 installation completed but python.exe was not found" }
    return $python
}

function Install-DocumentConverters {
    if (-not $InstallConverters) { return }
    $winget = Resolve-Application "winget"
    if (-not $winget) {
        Write-MementoWarning "WinGet is unavailable; optional Pandoc installation was skipped"
        return
    }
    Invoke-External $winget @(
        "install", "--id", "JohnMacFarlane.Pandoc", "--exact", "--scope", "user", "--silent",
        "--accept-package-agreements", "--accept-source-agreements"
    ) | Out-Null
}

function Install-Feeder {
    if ($Feeder -eq "never") {
        Write-MementoLog "optional document feeder skipped"
        return
    }
    $requirements = Join-Path $LibDir "tools\vault_sync\requirements.txt"
    if (-not (Test-Path -LiteralPath $requirements) -and -not $DryRun) {
        Write-MementoWarning "packaged feeder files are unavailable; direct memento import remains usable"
        return
    }
    $python = Install-PythonIfNeeded
    if (-not $python) {
        if ($Feeder -eq "always" -and -not $DryRun) { throw "Python 3.12 is required for the document feeder" }
        Write-MementoWarning "Python 3.12 is missing; document feeder skipped (core Markdown/PDF import still works)"
        return
    }
    $venv = Join-Path $InstallRoot "python"
    $venvPython = Join-Path $venv "Scripts\python.exe"
    if (-not (Test-Path -LiteralPath $venvPython)) {
        Invoke-External $python.Executable (@($python.Prefix) + @("-m", "venv", $venv)) | Out-Null
    }
    Invoke-External $venvPython @("-m", "pip", "install", "--disable-pip-version-check", "-r", $requirements) | Out-Null
    New-Directory $BinDir
    $wrapper = @"
@echo off
set "PYTHONPATH=$LibDir;%PYTHONPATH%"
"$venvPython" -m tools.vault_sync.cli %*
"@
    if (-not $DryRun) { Set-Content -LiteralPath (Join-Path $BinDir "memento-vault-sync.bat") -Value $wrapper -Encoding Ascii }
    Install-DocumentConverters
    Write-MementoLog "document feeder installed with an isolated Python environment"
}

function Resolve-AgentTargets {
    $requested = @()
    foreach ($value in $Agent) { $requested += $value -split ',' }
    if ($requested -contains "all") { return @("codex", "claude-code", "openclaw", "generic") }
    if ($requested -notcontains "auto") { return @($requested | Select-Object -Unique) }
    $detected = @()
    if (Resolve-Application "codex") { $detected += "codex" }
    if (Resolve-Application "claude") { $detected += "claude-code" }
    if (Resolve-Application "openclaw") { $detected += "openclaw" }
    if ($detected.Count -eq 0) { $detected += "generic" }
    return $detected
}

function Get-SkillDestination([string]$Target) {
    if ($Scope -eq "project") {
        if ($Target -eq "claude-code") { return Join-Path $ProjectDir ".claude\skills\$SkillName" }
        return Join-Path $ProjectDir ".agents\skills\$SkillName"
    }
    switch ($Target) {
        "claude-code" { return Join-Path $UserHome ".claude\skills\$SkillName" }
        "openclaw" {
            $state = if ($env:OPENCLAW_STATE_DIR) { $env:OPENCLAW_STATE_DIR } else { Join-Path $UserHome ".openclaw" }
            return Join-Path $state "skills\$SkillName"
        }
        default { return Join-Path $UserHome ".agents\skills\$SkillName" }
    }
}

function Install-Skills {
    if (-not (Test-Path -LiteralPath $CanonicalSkill) -and -not $DryRun) {
        throw "canonical skill not found at $CanonicalSkill"
    }
    foreach ($target in $ResolvedAgents) {
        $destination = Get-SkillDestination $target
        if ($InstalledDestinations.ContainsKey($destination)) { continue }
        $InstalledDestinations[$destination] = $true
        Copy-DirectoryContents $CanonicalSkill $destination
        Write-MementoLog "installed skill for agent discovery: $destination"
    }
}

function Test-AgentWantsMcp([string]$Target) {
    if ($Integration -eq "mcp" -or $Integration -eq "both") { return $true }
    if ($Integration -eq "cli") { return $false }
    return $Target -ne "generic"
}

function Configure-Mcp {
    if ($Integration -eq "cli") {
        Write-MementoLog "CLI integration selected; no MCP configuration changed"
        return
    }
    $mcpBinary = if (Test-Path -LiteralPath (Join-Path $BinDir "memento-mcp.exe")) {
        Join-Path $BinDir "memento-mcp.exe"
    } else {
        Resolve-Application "memento-mcp"
    }
    if (-not $mcpBinary -and -not $DryRun) { throw "memento-mcp.exe is unavailable" }
    if (-not $mcpBinary) { $mcpBinary = Join-Path $BinDir "memento-mcp.exe" }

    foreach ($target in $ResolvedAgents) {
        if (-not (Test-AgentWantsMcp $target)) { continue }
        switch ($target) {
            "codex" {
                $hostBinary = Resolve-Application "codex"
                if (-not $hostBinary) { Write-MementoWarning "Codex CLI not found; MCP registration skipped"; continue }
                Invoke-External $hostBinary @("mcp", "remove", "memento") -AllowFailure | Out-Null
                $commandArgs = @("mcp", "add")
                if ($DataDir) { $commandArgs += @("--env", "MEMENTO_DATA_DIR=$DataDir") }
                $commandArgs += @("memento", "--", $mcpBinary)
                Invoke-External $hostBinary $commandArgs | Out-Null
            }
            "claude-code" {
                $hostBinary = Resolve-Application "claude"
                if (-not $hostBinary) { Write-MementoWarning "Claude Code CLI not found; MCP registration skipped"; continue }
                $commandArgs = @("mcp", "remove", "--scope", $Scope, "memento")
                Invoke-External $hostBinary $commandArgs -AllowFailure | Out-Null
                $commandArgs = @("mcp", "add", "--transport", "stdio", "--scope", $Scope)
                if ($DataDir) { $commandArgs += @("--env", "MEMENTO_DATA_DIR=$DataDir") }
                $commandArgs += @("memento", "--", $mcpBinary)
                Push-Location $ProjectDir
                try { Invoke-External $hostBinary $commandArgs | Out-Null } finally { Pop-Location }
            }
            "openclaw" {
                $hostBinary = Resolve-Application "openclaw"
                if (-not $hostBinary) { Write-MementoWarning "OpenClaw CLI not found; MCP registration skipped"; continue }
                Invoke-External $hostBinary @("mcp", "unset", "memento") -AllowFailure | Out-Null
                $commandArgs = @("mcp", "add", "memento", "--command", $mcpBinary)
                if ($DataDir) { $commandArgs += @("--env", "MEMENTO_DATA_DIR=$DataDir") }
                if ($SkipInit -or -not $Vault) { $commandArgs += "--no-probe" }
                Invoke-External $hostBinary $commandArgs | Out-Null
            }
            "generic" { Write-MementoWarning "generic host: configure stdio command $mcpBinary" }
        }
    }
}

function Resolve-MementoBinary([string]$Name) {
    $installed = Join-Path $BinDir "$Name.exe"
    if (Test-Path -LiteralPath $installed) { return $installed }
    return Resolve-Application $Name
}

function Invoke-Memento([string]$Name, [string[]]$Arguments) {
    $binary = Resolve-MementoBinary $Name
    if (-not $binary -and -not $DryRun) { throw "$Name.exe is unavailable" }
    if (-not $binary) { $binary = Join-Path $BinDir "$Name.exe" }
    $previous = $env:MEMENTO_DATA_DIR
    if ($DataDir) { $env:MEMENTO_DATA_DIR = $DataDir }
    try { Invoke-External $binary $Arguments | Out-Null } finally {
        if ($null -eq $previous) { Remove-Item Env:MEMENTO_DATA_DIR -ErrorAction SilentlyContinue } else { $env:MEMENTO_DATA_DIR = $previous }
    }
}

function Verify-Installation {
    foreach ($name in @("memento", "mementod", "memento-mcp")) {
        Invoke-Memento $name @("--version")
    }
}

function Initialize-Runtime {
    if ($SkipInit) { Write-MementoLog "runtime initialization skipped"; return }
    if (-not $Vault) {
        Write-MementoLog "no -Vault supplied; onboarding remains pending"
        return
    }
    Invoke-Memento "memento" @("init", "--preset", "windows", "--vault-root", ([IO.Path]::GetFullPath($Vault)))
    Invoke-Memento "memento" @("doctor")
    Invoke-Memento "memento" @("status")
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "scripts/install.ps1 is the native Windows installer; use scripts/install.sh on Unix"
    }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $ResolvedAgents = @(Resolve-AgentTargets)
    Write-MementoLog ("agent targets: " + ($ResolvedAgents -join ", "))
    Install-Program
    Install-SupportFiles
    Add-UserPath
    Install-Feeder
    Verify-Installation
    Install-Skills
    Initialize-Runtime
    Configure-Mcp
    Write-MementoLog "installation complete"
    Write-MementoLog "integration: $Integration; scope: $Scope; program source: $ProgramSource"
} finally {
    if ($TemporaryRoot -and (Test-Path -LiteralPath $TemporaryRoot)) {
        Remove-Item -LiteralPath $TemporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
