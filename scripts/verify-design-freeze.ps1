param(
    [switch]$CheckExternalSources
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$FixtureRoot = Join-Path $RepoRoot 'fixtures\design-freeze'
$Utf8 = [System.Text.UTF8Encoding]::new($false, $true)
$Checks = 0

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )
    if (-not $Condition) {
        throw "design-draft verification failed: $Message"
    }
    $script:Checks++
}

function Read-Json {
    param([Parameter(Mandatory = $true)][string]$Path)
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "missing fixture: $Path"
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $text = $Utf8.GetString($bytes)
    return $text | ConvertFrom-Json
}

function Convert-HexBytes {
    param([Parameter(Mandatory = $true)][string]$Hex)
    $value = $Hex.Trim().ToLowerInvariant()
    if ($value.StartsWith('0x')) {
        $value = $value.Substring(2)
    }
    Assert-True (($value.Length % 2) -eq 0) "hex value has odd length: $Hex"
    Assert-True ($value -match '^[0-9a-f]*$') "hex value contains invalid characters: $Hex"
    $bytes = [byte[]]::new($value.Length / 2)
    for ($index = 0; $index -lt $bytes.Length; $index++) {
        $bytes[$index] = [Convert]::ToByte($value.Substring($index * 2, 2), 16)
    }
    return $bytes
}

function Convert-BytesHex {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)
    return -join ($Bytes | ForEach-Object { $_.ToString('x2') })
}

function Read-U16Be {
    param([byte[]]$Bytes, [int]$Offset)
    return ([uint32]$Bytes[$Offset] -shl 8) -bor [uint32]$Bytes[$Offset + 1]
}

function Read-U32Be {
    param([byte[]]$Bytes, [int]$Offset)
    return [uint32]((([uint64]$Bytes[$Offset] -shl 24) -bor
        ([uint64]$Bytes[$Offset + 1] -shl 16) -bor
        ([uint64]$Bytes[$Offset + 2] -shl 8) -bor
        [uint64]$Bytes[$Offset + 3]))
}

function Read-HexU32 {
    param([Parameter(Mandatory = $true)][string]$Value)
    $hex = $Value.ToLowerInvariant()
    if ($hex.StartsWith('0x')) {
        $hex = $hex.Substring(2)
    }
    return [Convert]::ToUInt32($hex, 16)
}

function Test-Ipv4Checksum {
    param([byte[]]$Packet, [int]$HeaderLength)
    $sum = [uint64]0
    for ($index = 0; $index -lt $HeaderLength; $index += 2) {
        $sum += ([uint64]$Packet[$index] -shl 8) -bor [uint64]$Packet[$index + 1]
    }
    while (($sum -shr 16) -ne 0) {
        $sum = ($sum -band 0xffff) + ($sum -shr 16)
    }
    return $sum -eq 0xffff
}

function Decode-LegacyVpn {
    param([byte[]]$Datagram)
    if ($Datagram.Length -lt 28) {
        return [pscustomobject]@{ Ok = $false; Error = 'packet_too_small' }
    }
    if (($Datagram[0] -band 0xf0) -ne 0x90) {
        return [pscustomobject]@{ Ok = $false; Error = 'invalid_first_byte' }
    }
    if ($Datagram[1] -ne 1) {
        return [pscustomobject]@{ Ok = $false; Error = 'unsupported_next_protocol' }
    }
    if (($Datagram[8] -shr 4) -ne 4) {
        return [pscustomobject]@{ Ok = $false; Error = 'inner_not_ipv4' }
    }
    $innerLength = $Datagram.Length - 8
    $ihl = ($Datagram[8] -band 0x0f) * 4
    if ($ihl -lt 20 -or $ihl -gt $innerLength -or ($ihl % 4) -ne 0) {
        return [pscustomobject]@{ Ok = $false; Error = 'invalid_ipv4_ihl' }
    }
    $totalLength = Read-U16Be $Datagram 10
    if ($totalLength -lt $ihl -or $totalLength -ne $innerLength) {
        return [pscustomobject]@{ Ok = $false; Error = 'ipv4_total_length_mismatch' }
    }
    return [pscustomobject]@{
        Ok = $true
        Error = $null
        HeaderHex = Convert-BytesHex ([byte[]]$Datagram[0..7])
        Flags = Read-U16Be $Datagram 2
        SessionId = Read-U32Be $Datagram 4
        InnerLength = $innerLength
        Ihl = $ihl
    }
}

function Test-ProtocolFixture {
    $path = Join-Path $FixtureRoot 'legacy-vpn-vectors.json'
    $fixture = Read-Json $path
    Assert-True ($fixture.schema_version -eq 1) 'legacy protocol schema version must be 1'
    Assert-True ($fixture.header_size -eq 8) 'legacy header size must be 8'
    Assert-True ((Read-HexU32 $fixture.canonical_first_byte) -eq 0x90) 'canonical first byte must be 0x90'
    Assert-True ((Read-HexU32 $fixture.ipv4_next_protocol) -eq 1) 'IPv4 next protocol must be 1'

    $inner = Convert-HexBytes $fixture.inner_ipv4_hex
    Assert-True ($inner.Length -ge 20) 'golden inner IPv4 packet is too short'
    $ihl = ($inner[0] -band 0x0f) * 4
    Assert-True (($inner[0] -shr 4) -eq 4) 'golden inner packet is not IPv4'
    Assert-True ((Read-U16Be $inner 2) -eq $inner.Length) 'golden inner total length mismatch'
    Assert-True (Test-Ipv4Checksum $inner $ihl) 'golden inner IPv4 checksum is invalid'

    $encoderNames = @{}
    foreach ($vector in $fixture.encoder_vectors) {
        Assert-True (-not $encoderNames.ContainsKey($vector.name)) "duplicate encoder vector: $($vector.name)"
        $encoderNames[$vector.name] = $true
        $session = [uint64](Read-HexU32 $vector.session_id)
        $nextNode = [uint64](Read-HexU32 $vector.next_node_id)
        $composed = if ($nextNode -eq 0) { $session } else { ($session -band 0xffff0000L) -bor $nextNode }
        Assert-True ($composed -eq (Read-HexU32 $vector.expected_composed_session_id)) "composed session mismatch: $($vector.name)"

        $header = [byte[]](0x90, 0x01, 0x00, 0x00,
            [byte](($composed -shr 24) -band 0xff),
            [byte](($composed -shr 16) -band 0xff),
            [byte](($composed -shr 8) -band 0xff),
            [byte]($composed -band 0xff))
        Assert-True ((Convert-BytesHex $header) -eq $vector.expected_header_hex) "header mismatch: $($vector.name)"
        Assert-True ((Convert-BytesHex ([byte[]]($header + $inner))) -eq $vector.expected_datagram_hex) "datagram mismatch: $($vector.name)"
    }

    foreach ($vector in $fixture.decoder_vectors) {
        $result = Decode-LegacyVpn (Convert-HexBytes $vector.datagram_hex)
        Assert-True $result.Ok "valid decoder vector rejected: $($vector.name)"
        Assert-True ($result.HeaderHex -eq $vector.expected_header_hex) "exact header was not preserved: $($vector.name)"
        Assert-True ($result.SessionId -eq (Read-HexU32 $vector.expected_session_id)) "session mismatch: $($vector.name)"
        Assert-True ($result.Flags -eq (Read-HexU32 $vector.expected_flags)) "flags mismatch: $($vector.name)"
    }

    foreach ($vector in $fixture.invalid_vectors) {
        $result = Decode-LegacyVpn (Convert-HexBytes $vector.datagram_hex)
        Assert-True (-not $result.Ok) "invalid decoder vector accepted: $($vector.name)"
        Assert-True ($result.Error -eq $vector.expected_error) "wrong error for $($vector.name): expected $($vector.expected_error), got $($result.Error)"
    }
}

function Test-AbiFixture {
    $path = Join-Path $FixtureRoot 'netfilter-abi-x64.json'
    $fixture = Read-Json $path
    Assert-True ($fixture.schema_version -eq 1) 'ABI schema version must be 1'
    Assert-True ($fixture.variant -eq 'wfp-c-api-x64') 'ABI variant must be WFP C API x64'
    Assert-True ($fixture.rust_target -eq 'x86_64-pc-windows-msvc') 'unexpected ABI Rust target'
    Assert-True ($fixture.calling_convention -eq 'cdecl') 'NetFilter C API must use cdecl'
    Assert-True ($fixture.pointer_size -eq 8) 'x64 pointer size must be 8'
    Assert-True ($fixture.pack -eq 1) 'vendor structures must use pack(1)'
    Assert-True (-not $fixture.source_worktree_clean) 'dirty source provenance must stay explicit'

    foreach ($source in @($fixture.sources) + @($fixture.artifacts)) {
        Assert-True ($source.sha256 -match '^[0-9a-f]{64}$') "invalid SHA-256: $($source.path)"
        if ($CheckExternalSources) {
            Assert-True (Test-Path -LiteralPath $source.path -PathType Leaf) "external source missing: $($source.path)"
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $source.path).Hash.ToLowerInvariant()
            Assert-True ($actual -eq $source.sha256) "external source drift: $($source.path)"
        }
    }

    $typeNames = @{}
    foreach ($type in $fixture.types) {
        Assert-True (-not $typeNames.ContainsKey($type.name)) "duplicate ABI type: $($type.name)"
        $typeNames[$type.name] = $true
        Assert-True ($type.size -gt 0) "ABI type size must be positive: $($type.name)"
        if ($type.name -eq 'ENDPOINT_ID') {
            Assert-True ($type.align -eq 8) 'ENDPOINT_ID scalar alignment must be 8 on x64'
        }
        else {
            Assert-True ($type.align -eq 1) "ABI structure must be packed: $($type.name)"
        }
        $end = 0
        foreach ($field in $type.fields) {
            Assert-True ($field.size -gt 0) "field size must be positive: $($type.name).$($field.name)"
            Assert-True ($field.offset -ge $end) "overlapping or unordered field: $($type.name).$($field.name)"
            $end = $field.offset + $field.size
            Assert-True ($end -le $type.size) "field exceeds type size: $($type.name).$($field.name)"
        }
    }

    foreach ($table in $fixture.callback_tables) {
        Assert-True ($table.count -eq @($table.callbacks).Count) "callback count mismatch: $($table.name)"
        Assert-True ($table.size -eq ($table.count * $fixture.pointer_size)) "callback table size mismatch: $($table.name)"
        Assert-True (@($table.callbacks | Select-Object -Unique).Count -eq $table.count) "duplicate callback: $($table.name)"
    }

    $symbols = @($fixture.required_symbols)
    Assert-True (($symbols | Select-Object -Unique).Count -eq $symbols.Count) 'required DLL symbols must be unique'
    foreach ($required in @('nf_init', 'nf_free', 'nf_addRule', 'nf_udpPostSend', 'nf_udpPostReceive', 'nf_ipPostReceive', 'nf_setIPEventHandler')) {
        Assert-True ($symbols -contains $required) "missing required DLL symbol: $required"
    }

    if ($CheckExternalSources) {
        $command = Get-Command 'cl.exe' -ErrorAction SilentlyContinue
        Assert-True ($null -ne $command) 'MSVC cl.exe is required for the external ABI probe'
        $compiler = $command.Source -replace 'Hostx64\\x86\\cl\.exe$', 'Hostx64\x64\cl.exe'
        Assert-True (Test-Path -LiteralPath $compiler -PathType Leaf) "x64 MSVC compiler not found: $compiler"

        $msvcRoot = $compiler
        for ($level = 0; $level -lt 4; $level++) {
            $msvcRoot = Split-Path -Parent $msvcRoot
        }
        Assert-True (Test-Path -LiteralPath (Join-Path $msvcRoot 'include') -PathType Container) 'MSVC include directory is missing'

        $kitsRoot = 'C:\Program Files (x86)\Windows Kits\10\Include'
        Assert-True (Test-Path -LiteralPath $kitsRoot -PathType Container) 'Windows SDK include root is missing'
        $kit = Get-ChildItem -LiteralPath $kitsRoot -Directory |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'um\windows.h') -PathType Leaf } |
            Sort-Object Name -Descending |
            Select-Object -First 1
        Assert-True ($null -ne $kit) 'no usable Windows SDK include version found'

        $nfapiSource = $fixture.sources | Where-Object { $_.path -match '/wfp/include/nfapi\.h$' } | Select-Object -First 1
        $nfeventsSource = $fixture.sources | Where-Object { $_.path -match '/include/nfevents\.h$' } | Select-Object -First 1
        Assert-True ($null -ne $nfapiSource) 'ABI manifest does not identify nfapi.h'
        Assert-True ($null -ne $nfeventsSource) 'ABI manifest does not identify nfevents.h'

        $includes = @(
            (Join-Path $msvcRoot 'include'),
            (Join-Path $kit.FullName 'ucrt'),
            (Join-Path $kit.FullName 'shared'),
            (Join-Path $kit.FullName 'um'),
            (Join-Path $kit.FullName 'winrt'),
            (Split-Path -Parent $nfapiSource.path),
            (Split-Path -Parent $nfeventsSource.path)
        )
        $probe = Join-Path $FixtureRoot 'netfilter-abi-probe.c'
        $arguments = @('/nologo', '/TC', '/W4', '/WX', '/Zs') +
            @($includes | ForEach-Object { "/I$_" }) + @($probe)
        & $compiler @arguments
        Assert-True ($LASTEXITCODE -eq 0) 'MSVC NetFilter ABI probe failed'
    }
}

function Test-PolicyCapacityFixture {
    $path = Join-Path $FixtureRoot 'policy-capacity.json'
    $fixture = Read-Json $path
    Assert-True ($fixture.schema_version -eq 1) 'policy/capacity schema version must be 1'
    Assert-True ($fixture.freeze_version -eq 'design-draft-v1') 'unexpected design-draft version'
    Assert-True ($fixture.status -eq 'future_incremental_confirmation') 'design draft must remain unconfirmed'

    $policyIds = @($fixture.failure_policies | ForEach-Object { $_.id })
    Assert-True (($policyIds | Select-Object -Unique).Count -eq $policyIds.Count) 'failure policy IDs must be unique'
    foreach ($required in @('unmatched_or_unknown_process', 'explicit_block',
        'proxy_admission_failure_before_consume', 'must_proxy_admission_failure',
        'locked_proxy_runtime_failure', 'driver_start_failure', 'dataplane_queue_full',
        'tunnel_reconnect_exhausted', 'malformed_or_stale_downlink', 'shutdown_deadline_exhausted')) {
        Assert-True ($policyIds -contains $required) "missing failure policy: $required"
    }
    foreach ($policy in $fixture.failure_policies) {
        Assert-True (-not [string]::IsNullOrWhiteSpace($policy.action)) "empty failure action: $($policy.id)"
        Assert-True ($policy.observable) "failure policy must be observable: $($policy.id)"
    }

    $capacityIds = @($fixture.capacities | ForEach-Object { $_.id })
    Assert-True (($capacityIds | Select-Object -Unique).Count -eq $capacityIds.Count) 'capacity IDs must be unique'
    foreach ($capacity in $fixture.capacities) {
        Assert-True ($capacity.default -gt 0) "capacity default must be positive: $($capacity.id)"
        Assert-True ($capacity.hard_max -ge $capacity.default) "capacity default exceeds hard max: $($capacity.id)"
        Assert-True (-not [string]::IsNullOrWhiteSpace($capacity.exhaustion)) "missing exhaustion action: $($capacity.id)"
        Assert-True (@($capacity.metrics).Count -gt 0) "missing capacity metrics: $($capacity.id)"
    }
    foreach ($required in @('packet_pool_bytes', 'callback_queue_packets', 'callback_queue_bytes',
        'tunnel_queue_packets', 'tunnel_queue_bytes', 'tcp_flows', 'udp_sessions',
        'pending_resolution', 'actor_events_per_poll', 'actor_bytes_per_poll',
        'tunnel_reconnect_attempts', 'shutdown_deadline_seconds')) {
        Assert-True ($capacityIds -contains $required) "missing capacity: $required"
    }
}

function Test-RepositoryDocuments {
    $requiredFiles = @(
        'AGENTS.md',
        'README.md',
        'docs/project-progress.md',
        'docs/design-freeze-draft.md',
        'fixtures/design-freeze/legacy-vpn-vectors.json',
        'fixtures/design-freeze/netfilter-abi-x64.json',
        'fixtures/design-freeze/netfilter-abi-probe.c',
        'fixtures/design-freeze/policy-capacity.json'
    )
    foreach ($relative in $requiredFiles) {
        Assert-True (Test-Path -LiteralPath (Join-Path $RepoRoot $relative) -PathType Leaf) "missing repository file: $relative"
    }

    $markdown = @(Get-ChildItem -LiteralPath $RepoRoot -Recurse -File -Filter '*.md' |
        Where-Object { $_.FullName -notmatch '[\\/]\.git[\\/]' })
    $ledgerFiles = @()
    foreach ($file in $markdown) {
        $text = $Utf8.GetString([System.IO.File]::ReadAllBytes($file.FullName))
        if ($text -match '(?m)^- \[[ xX]\] \[P[0-2]\]') {
            $ledgerFiles += $file.FullName
        }
        $lineNumber = 0
        foreach ($line in [System.IO.File]::ReadAllLines($file.FullName, $Utf8)) {
            $lineNumber++
            Assert-True ($line -notmatch '[ \t]+$') "trailing whitespace: $($file.FullName):$lineNumber"
        }
    }
    $expectedLedger = (Resolve-Path (Join-Path $RepoRoot 'docs\project-progress.md')).Path
    Assert-True ($ledgerFiles.Count -eq 1) 'there must be exactly one functional status ledger'
    Assert-True ($ledgerFiles[0] -eq $expectedLedger) 'project-progress.md must be the only status ledger'

    $progress = $Utf8.GetString([System.IO.File]::ReadAllBytes($expectedLedger))
    Assert-True ($progress -match 'DESIGN-001') 'project status ledger must contain DESIGN-001'
    Assert-True ($progress -match 'scripts/verify-design-freeze\.ps1') 'design draft ledger item must link its automated verification'

    Push-Location $RepoRoot
    try {
        & git diff --check
        Assert-True ($LASTEXITCODE -eq 0) 'git diff --check failed'
    }
    finally {
        Pop-Location
    }
}

Test-ProtocolFixture
Test-AbiFixture
Test-PolicyCapacityFixture
Test-RepositoryDocuments

Write-Host "Design-draft verification passed: $Checks assertions; external_sources=$CheckExternalSources"
