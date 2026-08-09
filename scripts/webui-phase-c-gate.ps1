[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"

Push-Location $webui
try {
    & npm ci --ignore-scripts
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run gate
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-c-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& git diff --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "webui phase C gate passed"
