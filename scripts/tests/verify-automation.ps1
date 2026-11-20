Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Utf8 = [System.Text.UTF8Encoding]::new($false, $true)

function Assert-True {
    param($Condition, [string]$Message)
    if (-not $Condition) {
        throw "automation self-test failed: $Message"
    }
}

$RequiredFiles = @(
    'rust-toolchain.toml',
    'Cargo.toml',
    'Cargo.lock',
    '.cargo/config.toml',
    'scripts/verify.ps1',
    'scripts/verify-docs.ps1',
    'scripts/verify-vm-acceptance.ps1',
    '.github/workflows/ci.yml'
)
foreach ($relative in $RequiredFiles) {
    Assert-True (Test-Path -LiteralPath (Join-Path $RepoRoot $relative) -PathType Leaf) "missing $relative"
}

$Toolchain = $Utf8.GetString([System.IO.File]::ReadAllBytes((Join-Path $RepoRoot 'rust-toolchain.toml')))
Assert-True ($Toolchain -match 'channel\s*=\s*"1\.97\.1"') 'toolchain must be pinned to Rust 1.97.1'
Assert-True ($Toolchain -match '"rustfmt"') 'rustfmt component must be pinned'
Assert-True ($Toolchain -match '"clippy"') 'Clippy component must be pinned'
Assert-True ($Toolchain -match '"x86_64-pc-windows-msvc"') 'Windows MSVC target must be pinned'

$PowerShellExe = Join-Path $PSHOME 'powershell.exe'
if (-not (Test-Path -LiteralPath $PowerShellExe)) {
    $PowerShellExe = (Get-Process -Id $PID).Path
}
$Plan = & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $RepoRoot 'scripts\verify.ps1') -Plan 2>&1 | Out-String
Assert-True ($LASTEXITCODE -eq 0) "verification plan failed: $Plan"
foreach ($expected in @('verify-docs', 'automation-self-test', 'cargo-fmt', 'cargo-clippy-release', 'cargo-test-release', 'cargo-build-release')) {
    Assert-True ($Plan -match [regex]::Escape($expected)) "verification plan omits $expected"
}

$Workflow = $Utf8.GetString([System.IO.File]::ReadAllBytes((Join-Path $RepoRoot '.github\workflows\ci.yml')))
Assert-True ($Workflow -match 'windows-latest') 'CI must run on Windows'
Assert-True ($Workflow -match 'scripts/verify\.ps1') 'CI must call the repository verification entrypoint'
Assert-True ($Workflow -match 'timeout-minutes') 'CI job must have a timeout'

Write-Host 'Automation self-test passed.'
