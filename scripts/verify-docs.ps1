Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Utf8 = [System.Text.UTF8Encoding]::new($false, $true)
$TextExtensions = @('.md', '.json', '.ps1', '.rs', '.toml', '.yml', '.yaml', '.c')
$TextFiles = @(Get-ChildItem -LiteralPath $RepoRoot -Recurse -File | Where-Object {
    $TextExtensions -contains $_.Extension -and
    $_.FullName -notmatch '[\\/]\.git[\\/]' -and
    $_.FullName -notmatch '[\\/]target[\\/]'
})

foreach ($file in $TextFiles) {
    $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
    $text = $Utf8.GetString($bytes)
    $lineNumber = 0
    foreach ($line in [System.IO.File]::ReadAllLines($file.FullName, $Utf8)) {
        $lineNumber++
        if ($line -match '[ \t]+$') {
            throw "trailing whitespace: $($file.FullName):$lineNumber"
        }
        if ($line -match '^(<<<<<<<|=======|>>>>>>>)') {
            throw "merge conflict marker: $($file.FullName):$lineNumber"
        }
    }
    if ($file.Extension -eq '.json') {
        [void]($text | ConvertFrom-Json)
    }
}

$MarkdownFiles = @($TextFiles | Where-Object { $_.Extension -eq '.md' })
$LinkPattern = '\[[^\]]*\]\((?<target>[^)]+)\)'
foreach ($file in $MarkdownFiles) {
    $text = $Utf8.GetString([System.IO.File]::ReadAllBytes($file.FullName))
    foreach ($match in [regex]::Matches($text, $LinkPattern)) {
        $target = $match.Groups['target'].Value.Trim()
        if ($target.StartsWith('<') -and $target.EndsWith('>')) {
            $target = $target.Substring(1, $target.Length - 2)
        }
        $target = ($target -split '#', 2)[0]
        if ([string]::IsNullOrWhiteSpace($target) -or
            $target -match '^[a-zA-Z][a-zA-Z0-9+.-]*:') {
            continue
        }
        $resolved = Join-Path $file.DirectoryName ([Uri]::UnescapeDataString($target))
        if (-not (Test-Path -LiteralPath $resolved)) {
            throw "missing Markdown link target: $($file.FullName) -> $target"
        }
    }
}

$LedgerFiles = @()
foreach ($file in $MarkdownFiles) {
    $text = $Utf8.GetString([System.IO.File]::ReadAllBytes($file.FullName))
    if ($text -match '(?m)^- \[[ xX]\] \[P[0-2]\]') {
        $LedgerFiles += $file.FullName
    }
}
$ExpectedLedger = (Resolve-Path (Join-Path $RepoRoot 'docs\project-progress.md')).Path
if ($LedgerFiles.Count -ne 1 -or $LedgerFiles[0] -ne $ExpectedLedger) {
    throw "expected one status ledger at docs/project-progress.md; got: $($LedgerFiles -join ', ')"
}

$Progress = $Utf8.GetString([System.IO.File]::ReadAllBytes($ExpectedLedger))
$FullWidthColon = [char]0xff1a
$TestMarker = ([char]0x6d4b).ToString() + [char]0x8bd5 + $FullWidthColon
$FeaturePattern = '(?m)^- \[ \] \[P[0-2]\] [ACILDNWOMRB]\d{2}' + $FullWidthColon + '(?<body>.*)$'
foreach ($match in [regex]::Matches($Progress, $FeaturePattern)) {
    if (-not $match.Groups['body'].Value.Contains($TestMarker)) {
        throw "pending feature lacks automated-test mapping: $($match.Value)"
    }
}

Push-Location $RepoRoot
try {
    & git diff --check
    if ($LASTEXITCODE -ne 0) {
        throw "git diff --check failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Write-Host "Documentation verification passed: $($TextFiles.Count) text files, $($MarkdownFiles.Count) Markdown files."
