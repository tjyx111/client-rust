param(
    [switch]$Plan
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$PowerShellExe = Join-Path $PSHOME 'powershell.exe'
if (-not (Test-Path -LiteralPath $PowerShellExe)) {
    $PowerShellExe = (Get-Process -Id $PID).Path
}

$CargoCommand = Get-Command 'cargo.exe' -ErrorAction SilentlyContinue
$CargoPath = if ($null -ne $CargoCommand) {
    $CargoCommand.Source
}
else {
    Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
}
if (-not (Test-Path -LiteralPath $CargoPath -PathType Leaf)) {
    throw 'cargo.exe was not found; install the pinned Rust toolchain first'
}

$RustcCommand = Get-Command 'rustc.exe' -ErrorAction SilentlyContinue
$RustcPath = if ($null -ne $RustcCommand) {
    $RustcCommand.Source
}
else {
    Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
}
if (-not (Test-Path -LiteralPath $RustcPath -PathType Leaf)) {
    throw 'rustc.exe was not found; install the pinned Rust toolchain first'
}

$Steps = @(
    [pscustomobject]@{
        Name = 'verify-docs'
        Command = $PowerShellExe
        Arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $RepoRoot 'scripts\verify-docs.ps1'))
    },
    [pscustomobject]@{
        Name = 'automation-self-test'
        Command = $PowerShellExe
        Arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $RepoRoot 'scripts\tests\verify-automation.ps1'))
    },
    [pscustomobject]@{
        Name = 'cargo-fmt'
        Command = $CargoPath
        Arguments = @('fmt', '--all', '--', '--check')
    },
    [pscustomobject]@{
        Name = 'cargo-clippy-release'
        Command = $CargoPath
        Arguments = @('clippy', '--locked', '--release', '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings')
    },
    [pscustomobject]@{
        Name = 'cargo-test-release'
        Command = $CargoPath
        Arguments = @('test', '--locked', '--release', '--workspace', '--all-features')
    },
    [pscustomobject]@{
        Name = 'cargo-build-release'
        Command = $CargoPath
        Arguments = @('build', '--locked', '--release', '--workspace', '--all-features')
    }
)

if ($Plan) {
    foreach ($step in $Steps) {
        Write-Output "$($step.Name): $($step.Command) $($step.Arguments -join ' ')"
    }
    exit 0
}

$Version = (& $RustcPath --version 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $Version -notmatch '^rustc 1\.97\.1\b') {
    throw "expected rustc 1.97.1, got: $Version"
}
Write-Host "Pinned toolchain: $Version"

# Source mtimes can be older than build artifacts after external edits, which
# makes Cargo skip a required rebuild. Refresh workspace Rust sources so the
# verification always builds the current code. This does not delete the cache.
Get-ChildItem -LiteralPath (Join-Path $RepoRoot 'crates') -Recurse -File -Filter '*.rs' |
    ForEach-Object { $_.LastWriteTime = Get-Date }

Push-Location $RepoRoot
try {
    foreach ($step in $Steps) {
        $started = [DateTimeOffset]::UtcNow
        Write-Host "[$($step.Name)] starting"
        $arguments = $step.Arguments
        & $step.Command @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$($step.Name) failed with exit code $LASTEXITCODE"
        }
        $elapsed = [DateTimeOffset]::UtcNow - $started
        Write-Host "[$($step.Name)] passed in $([Math]::Round($elapsed.TotalSeconds, 3))s"
    }
}
finally {
    Pop-Location
}

Write-Host 'Full S0 verification passed.'
