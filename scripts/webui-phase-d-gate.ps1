[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
& pwsh -NoProfile -File (Join-Path $workspace "scripts/webui-phase-d-contracts.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Push-Location (Join-Path $workspace "webui")
try {
    & npm run test:browser
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }
& pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") -Path "webui/src"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "webui phase D gate passed"
