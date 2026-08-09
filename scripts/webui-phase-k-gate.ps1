$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-j-gate.ps1")
& (Join-Path $PSScriptRoot "webui-phase-k-contracts.ps1")
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run test:e2e; npm run build; npm run check:bundle; npm run check:security } finally { Pop-Location }
Write-Output "Phase K gate passed"
