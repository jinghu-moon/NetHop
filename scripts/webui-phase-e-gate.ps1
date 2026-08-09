[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-e-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
foreach ($path in @("webui/src/model", "webui/tests/unit/dto.test.ts")) {
    & pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") -Path $path
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Write-Host "webui phase E gate passed"
