[CmdletBinding()]
param(
    [switch]$SkipWorkspace
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$statusPath = Join-Path $workspace "artifacts/node-territory/d16-implementation-status.json"

function Invoke-Gate {
    param([Parameter(Mandatory = $true)][string]$Path)
    & (Join-Path $workspace $Path)
    if ($LASTEXITCODE -ne 0) { throw "gate failed: $Path" }
}

$status = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json
if ($status.schema -ne "nethop-node-territory-d16-status-v1") {
    throw "D16 implementation status schema is invalid"
}
if (@($status.stages).Count -ne 14 -or (@($status.stages.id | Sort-Object -Unique) -join "") -ne "ABCDEFGHIJKLMN") {
    throw "D16 implementation status must contain stages A-N exactly once"
}
$allowedStatuses = @("passed", "passed_with_visual_review", "passed_with_external_evidence_blocked", "pending_device", "in_progress")
foreach ($stage in $status.stages) {
    if ($allowedStatuses -notcontains $stage.status) { throw "D16 stage has an invalid status: $($stage.id)" }
    foreach ($evidence in @($stage.evidence)) {
        if ([string]::IsNullOrWhiteSpace($evidence) -or -not (Test-Path -LiteralPath (Join-Path $workspace $evidence))) {
            throw "D16 stage evidence is missing: $($stage.id)/$evidence"
        }
    }
}
if ($status.contracts.control_protocol -ne 4 -or
    $status.contracts.generation_node_registry -ne 3 -or
    $status.contracts.selection_snapshot -ne 2 -or
    $status.contracts.territory_count -ne 249 -or
    $status.contracts.flag_count -ne 249) {
    throw "D16 implementation status contract values drifted"
}
$rawStatus = Get-Content -LiteralPath $statusPath -Raw
if ($rawStatus -match '(?i)subscription_url|token|password|uuid|private_key|server_address') {
    throw "D16 implementation status contains a forbidden sensitive key"
}

Invoke-Gate "scripts/territory-data-gate.ps1"
Invoke-Gate "scripts/territory-engine-gate.ps1"
Invoke-Gate "scripts/territory-generation-gate.ps1"
Invoke-Gate "scripts/territory-protocol-gate.ps1"
Invoke-Gate "scripts/node-territory-webui-gate.ps1"
Invoke-Gate "scripts/module-contracts.ps1"
Invoke-Gate "scripts/webui-release-readiness-contracts.ps1"

Push-Location $workspace
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt check failed" }
    if (-not $SkipWorkspace) {
        cargo test --workspace --all-features --locked
        if ($LASTEXITCODE -ne 0) { throw "workspace regression failed" }
    }
    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw "git diff check failed" }
}
finally { Pop-Location }

Write-Output "D16 host release gate passed; device validation remains recorded in $statusPath"
