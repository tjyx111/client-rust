param(
    [string]$BinaryPath,
    [string]$NfapiDll,
    [string]$DriverSys,
    [string]$ReportDir = 'logs\vm-acceptance',
    [switch]$Plan,
    [switch]$ConfirmLabVm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Steps = @(
    'Verify administrator and -ConfirmLabVm',
    'Capture pre-state: netfilter2 service, driver hash, related drivers',
    'Run client binary; press Enter when prompted',
    'Capture post-state and exit code',
    'Assert exit 0, service/file cleanup, related drivers unchanged',
    'Write report and checklist log summary'
)

if ($Plan) {
    foreach ($step in $Steps) {
        Write-Output $step
    }
    exit 0
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this script as Administrator.'
}
if (-not $ConfirmLabVm) {
    throw 'Take a recoverable VM snapshot first, then pass -ConfirmLabVm.'
}
foreach ($required in @($BinaryPath, $NfapiDll, $DriverSys)) {
    if ([string]::IsNullOrWhiteSpace($required)) {
        throw 'BinaryPath, NfapiDll and DriverSys are required.'
    }
}
$BinaryPath = (Resolve-Path -LiteralPath $BinaryPath).Path
$NfapiDll = (Resolve-Path -LiteralPath $NfapiDll).Path
$DriverSys = (Resolve-Path -LiteralPath $DriverSys).Path
$driverFile = Join-Path $env:SystemRoot 'System32\drivers\netfilter2.sys'
$sourceHash = (Get-FileHash -LiteralPath $DriverSys -Algorithm SHA256).Hash
$checklistPath = Join-Path (Get-Location) 'logs\driver-checklist.log'
$null = New-Item -ItemType Directory -Path $ReportDir -Force

function Get-RelatedDriverLines {
    Get-CimInstance Win32_SystemDriver |
        Where-Object { $_.Name -match 'netfilter|wfp|accel|vpn' } |
        Sort-Object Name |
        ForEach-Object { '{0}|{1}|{2}|{3}' -f $_.Name, $_.State, $_.StartMode, $_.PathName }
}

function Get-Netfilter2Service {
    Get-CimInstance Win32_SystemDriver -Filter "Name='netfilter2'" -ErrorAction SilentlyContinue
}

$preService = Get-Netfilter2Service
$preExists = Test-Path -LiteralPath $driverFile
$preHash = if ($preExists) {
    (Get-FileHash -LiteralPath $driverFile -Algorithm SHA256).Hash
}
else {
    'MISSING'
}
$preRelated = @(Get-RelatedDriverLines)
if ($null -ne $preService -or ($preExists -and $preHash -ne $sourceHash)) {
    throw 'Pre-state is not clean: netfilter2 service exists or driver hash differs. Use a clean snapshot or run the manual conflict scenario.'
}

$transcript = Join-Path $ReportDir 'client-output.txt'
Write-Host 'Pre-state captured. Starting client; press Enter in the client when prompted.'
& $BinaryPath $NfapiDll $DriverSys 2>&1 | Tee-Object -FilePath $transcript
$exitCode = $LASTEXITCODE

$postService = Get-Netfilter2Service
$postExists = Test-Path -LiteralPath $driverFile
$postHash = if ($postExists) {
    (Get-FileHash -LiteralPath $driverFile -Algorithm SHA256).Hash
}
else {
    'MISSING'
}
$postRelated = @(Get-RelatedDriverLines)
$relatedDiff = @(Compare-Object -ReferenceObject $preRelated -DifferenceObject $postRelated)

$failures = New-Object System.Collections.Generic.List[string]
if ($exitCode -ne 0) {
    $failures.Add("client exit code $exitCode is not 0")
}
if ($null -ne $postService) {
    $failures.Add('netfilter2 service still exists after client exit')
}
if ($preHash -eq $sourceHash) {
    if ($postHash -ne $sourceHash) {
        $failures.Add("pre-existing identical driver file changed or disappeared: $postHash")
    }
}
elseif ($postExists) {
    $failures.Add('driver file still exists after copied-file cleanup')
}
if ($relatedDiff.Count -gt 0) {
    $failures.Add('related driver set changed; inspect diff')
}

$checklistResult = 'MISSING'
$checklistErrCount = 0
if (Test-Path -LiteralPath $checklistPath) {
    $checklistText = Get-Content -LiteralPath $checklistPath -Raw
    $checklistResult = 'present'
    $checklistErrCount = ([regex]::Matches($checklistText, 'result=ERR')).Count
    if ($checklistText -notmatch 'phase=startup') {
        $failures.Add('checklist log has no startup phase')
    }
    if ($checklistText -notmatch 'phase=shutdown') {
        $failures.Add('checklist log has no shutdown phase')
    }
}
else {
    $failures.Add('checklist log is missing')
}

$reportPath = Join-Path $ReportDir ("vm-acceptance-{0}.txt" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
$reportLines = @(
    "binary=$BinaryPath",
    "nfapi=$NfapiDll",
    "driver_sys=$DriverSys",
    "source_hash=$sourceHash",
    "exit_code=$exitCode",
    "pre_service=$($null -ne $preService)",
    "post_service=$($null -ne $postService)",
    "pre_file_exists=$preExists",
    "pre_file_hash=$preHash",
    "post_file_exists=$postExists",
    "post_file_hash=$postHash",
    "checklist=$checklistResult",
    "checklist_err_count=$checklistErrCount",
    "related_driver_diff_count=$($relatedDiff.Count)"
)
if ($relatedDiff.Count -gt 0) {
    $reportLines += 'related_driver_diff:'
    $reportLines += $relatedDiff | ForEach-Object { "  $($_.SideIndicator) $($_.InputObject)" }
}
$reportLines += 'result=' + ($(if ($failures.Count -eq 0) { 'PASS' } else { 'FAIL' }))
if ($failures.Count -gt 0) {
    $reportLines += 'failures:'
    $reportLines += $failures | ForEach-Object { "  $_" }
}
$reportLines | Out-File -LiteralPath $reportPath -Encoding UTF8
Write-Host "Report: $reportPath"
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "FAIL: $_" }
    exit 1
}
exit 0
