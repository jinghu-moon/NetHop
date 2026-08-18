[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $PSScriptRoot "validate-data-plane-evidence.ps1"
$valid = Join-Path $workspace "tests/data-plane/fixtures/valid-evidence.json"
$invalid = Join-Path $workspace "tests/data-plane/fixtures/invalid-evidence.json"

$summary = & $validator -EvidencePath $valid | ConvertFrom-Json
if ($summary.schema -ne "nethop-data-plane-benchmark-summary-v1" -or
    $summary.modes.tproxy.runs -ne 5 -or $summary.modes.tun.runs -ne 5) {
    throw "valid data-plane evidence did not produce the frozen summary"
}

$rejected = $false
try {
    & $validator -EvidencePath $invalid | Out-Null
} catch {
    $rejected = $true
}
if (-not $rejected) { throw "invalid data-plane evidence was accepted" }

Write-Host "NetHop data-plane benchmark contracts passed"
