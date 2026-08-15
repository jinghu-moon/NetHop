[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$contractRoot = Join-Path $workspace "out/webui-release-readiness-contracts"
$blockedRoot = Join-Path $contractRoot "blocked"
$readyRoot = Join-Path $contractRoot "ready"
$requiredAutomatic = @("bundle-metafile.json", "production-bundle.json", "webui-sbom.cdx.json", "webui-licenses.json", "checksums.sha256")
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Write-Utf8NoBom([string]$Path, [string]$Value) {
    [System.IO.File]::WriteAllText($Path, $Value, $utf8NoBom)
}

function Initialize-AutomaticArtifacts([string]$Root) {
    New-Item -ItemType Directory -Path $Root -Force | Out-Null
    foreach ($name in $requiredAutomatic) {
        Write-Utf8NoBom (Join-Path $Root $name) "{}"
    }
}

function Write-Json([string]$Path, [hashtable]$Value) {
    Write-Utf8NoBom $Path ($Value | ConvertTo-Json -Depth 8)
}

Initialize-AutomaticArtifacts $blockedRoot
Initialize-AutomaticArtifacts $readyRoot

Write-Json (Join-Path $readyRoot "kernelsu-device-evidence.json") @{
    schema = "nethop.webui.device-verification.v1"
    manager = "KernelSU"
    module_installed = $true
    status_query = "passed"
    closed_loop = "passed"
}
Write-Json (Join-Path $readyRoot "magisk-device-evidence.json") @{
    schema = "nethop.webui.device-verification.v1"
    manager = "Magisk"
    module_installed = $true
    status_query = "passed"
    closed_loop = "passed"
}
Write-Json (Join-Path $readyRoot "webview-performance-evidence.json") @{
    schema = "nethop.webui.performance-evidence.v1"
    status = "passed"
    p006 = "passed"
    p007 = "passed"
    p008 = "passed"
    p009 = "passed"
}
$blockedOutput = Join-Path $blockedRoot "release-readiness.json"
& (Join-Path $PSScriptRoot "webui-release-readiness.ps1") -ArtifactRoot $blockedRoot -OutputPath $blockedOutput | Out-Null
$blocked = Get-Content -LiteralPath $blockedOutput -Raw | ConvertFrom-Json
if ($blocked.ready) { throw "readiness must remain blocked when device and performance evidence is absent" }
foreach ($expected in @("KernelSU", "Magisk", "performance")) {
    if (-not ($blocked.blockers -match $expected)) { throw "missing blocker category: $expected" }
}

$readyOutput = Join-Path $readyRoot "release-readiness.json"
& (Join-Path $PSScriptRoot "webui-release-readiness.ps1") -ArtifactRoot $readyRoot -OutputPath $readyOutput -RequireReady | Out-Null
$ready = Get-Content -LiteralPath $readyOutput -Raw | ConvertFrom-Json
if (-not $ready.ready) { throw "complete evidence set must pass readiness" }
if (-not $ready.kernelsu_verified -or -not $ready.magisk_verified -or -not $ready.performance_verified) {
    throw "readiness did not preserve all verification dimensions"
}
if ($ready.blockers.Count -ne 0) { throw "ready report must not contain blockers" }

Write-Output "WebUI release readiness contracts passed"
