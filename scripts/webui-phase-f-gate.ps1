[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-f-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
foreach ($path in @("webui/src/runtime", "webui/tests/unit/event-runtime.test.ts")) {
    & pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") -Path $path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& git diff --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "webui phase F gate passed"
