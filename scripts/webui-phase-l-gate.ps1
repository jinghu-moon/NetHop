$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-k-gate.ps1")
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run test:unit -- --run; npm run test:browser; npm run test:e2e } finally { Pop-Location }
Write-Output "Phase L gate passed"
