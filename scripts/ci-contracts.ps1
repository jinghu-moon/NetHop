[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$workflowPath = Join-Path $workspace ".github/workflows/ci.yml"

function Assert-Contains([string]$Source, [string]$Needle, [string]$Message) {
    if (-not $Source.Contains($Needle)) {
        throw $Message
    }
}

if (-not (Test-Path -LiteralPath $workflowPath -PathType Leaf)) {
    throw "unified CI workflow is missing"
}

$workflow = Get-Content -LiteralPath $workflowPath -Raw
Assert-Contains $workflow "name: ci" "unified CI must expose the stable workflow name"
Assert-Contains $workflow "pull_request:" "unified CI must run for every pull request"
Assert-Contains $workflow "workflow_dispatch:" "unified CI must remain manually runnable"
foreach ($job in @("rust:", "webui:", "companion:", "module-contracts:", "required:")) {
    Assert-Contains $workflow $job "unified CI is missing job $job"
}
Assert-Contains $workflow "needs: [rust, webui, companion, module-contracts]" "required gate must aggregate every platform job"
Assert-Contains $workflow 'if: ${{ always() }}' "required gate must report failures instead of being skipped"
Assert-Contains $workflow "cargo test --workspace --locked --all-features" "Rust gate must execute the complete workspace"
Assert-Contains $workflow "cargo clippy --workspace --locked --all-targets --all-features -- -D warnings" "Rust gate must reject warnings"
Assert-Contains $workflow "npm run gate" "WebUI gate must execute the production quality contract"
Assert-Contains $workflow "testDebugUnitTest lintRelease assembleDebugAndroidTest" "Companion gate must cover JVM, lint and instrumentation compilation"
Assert-Contains $workflow "./scripts/module-contracts.ps1" "module packaging contracts must be part of the aggregate gate"
Assert-Contains $workflow "./scripts/android-build-contracts.ps1" "Android build provenance contracts must be part of the aggregate gate"
Assert-Contains $workflow "./scripts/data-plane-benchmark-contracts.ps1" "data-plane benchmark evidence contracts must be part of the aggregate gate"

Write-Host "NetHop unified CI contracts passed"
