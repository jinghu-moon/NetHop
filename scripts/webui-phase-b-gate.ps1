[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo test --workspace --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-b-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& pwsh -NoProfile -File (Join-Path $workspace "scripts/run-webui-regression-matrix.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& pwsh -NoProfile -File (Join-Path $workspace "scripts/module-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$secretScanPaths = @(
    "tests/webui/fixtures",
    "crates/nethop-protocol/tests",
    "crates/nethopctl/tests",
    "crates/nethopd/tests/webui_payload_contracts.rs"
)
foreach ($path in $secretScanPaths) {
    & pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") -Path $path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& git diff --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "webui phase B gate passed"
