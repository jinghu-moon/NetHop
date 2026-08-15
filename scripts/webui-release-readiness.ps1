[CmdletBinding()]
param(
    [switch]$RequireReady,
    [string]$ArtifactRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) "artifacts/webui"),
    [string]$OutputPath = (Join-Path $ArtifactRoot "release-readiness.json")
)

$ErrorActionPreference = "Stop"
$requiredAutomatic = @("bundle-metafile.json", "production-bundle.json", "webui-sbom.cdx.json", "webui-licenses.json", "checksums.sha256")
$missing = @($requiredAutomatic | Where-Object { -not (Test-Path -LiteralPath (Join-Path $ArtifactRoot $_) -PathType Leaf) })
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Read-Evidence([string]$Name) {
    $path = Join-Path $ArtifactRoot $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
    return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
}

function Test-DeviceEvidence($Evidence, [string]$Manager) {
    return $null -ne $Evidence -and
        $Evidence.schema -eq "nethop.webui.device-verification.v1" -and
        $Evidence.manager -eq $Manager -and
        $Evidence.module_installed -eq $true -and
        $Evidence.status_query -eq "passed" -and
        $Evidence.closed_loop -eq "passed"
}

$kernelSuEvidence = Read-Evidence "kernelsu-device-evidence.json"
$magiskEvidence = Read-Evidence "magisk-device-evidence.json"
$performanceEvidence = Read-Evidence "webview-performance-evidence.json"
$kernelSu = Test-DeviceEvidence $kernelSuEvidence "KernelSU"
$magisk = Test-DeviceEvidence $magiskEvidence "Magisk"
$performance = $null -ne $performanceEvidence -and
    $performanceEvidence.schema -eq "nethop.webui.performance-evidence.v1" -and
    $performanceEvidence.status -eq "passed" -and
    $performanceEvidence.p006 -eq "passed" -and
    $performanceEvidence.p007 -eq "passed" -and
    $performanceEvidence.p008 -eq "passed" -and
    $performanceEvidence.p009 -eq "passed"
$ready = $missing.Count -eq 0 -and $kernelSu -and $magisk -and $performance
$blockers = @()
if ($missing.Count -gt 0) { $blockers += "missing automatic artifacts: $($missing -join ', ')" }
if (-not $kernelSu) { $blockers += "KernelSU Android arm64 closed-loop evidence is missing" }
if (-not $magisk) { $blockers += "Magisk Action/CLI regression evidence is not available on the connected device" }
if (-not $performance) { $blockers += "Android WebView performance evidence for P006-P009 is missing" }

$report = [ordered]@{
    schema = "nethop.webui.release-readiness.v3"
    ready = $ready
    automatic_artifacts_complete = $missing.Count -eq 0
    kernelsu_verified = $kernelSu
    magisk_verified = $magisk
    performance_verified = $performance
    apatch_status = "declared_unverified"
    blockers = @($blockers)
}
New-Item -ItemType Directory -Path (Split-Path -Parent $OutputPath) -Force | Out-Null
[System.IO.File]::WriteAllText($OutputPath, ($report | ConvertTo-Json -Depth 4), $utf8NoBom)
$report | ConvertTo-Json -Depth 4
if ($RequireReady -and -not $ready) { throw "WebUI release readiness is blocked" }
