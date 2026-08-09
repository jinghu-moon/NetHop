[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& pwsh -NoProfile -File (Join-Path $workspace "scripts/test-webui.ps1") -Suite Rust
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-a-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& pwsh -NoProfile -File (Join-Path $workspace "scripts/run-webui-regression-matrix.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& pwsh -NoProfile -File (Join-Path $workspace "scripts/module-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& (Join-Path $workspace "scripts/scan-webui-secrets.ps1") -Path @(
    "tests/webui/fixtures",
    "crates/nethopctl/tests",
    "crates/nethop-protocol/tests"
)
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& git diff --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "webui phase A gate passed"
