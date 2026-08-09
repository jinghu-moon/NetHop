$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-g-gate.ps1")
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run test:browser; npm run test:e2e; npm run build; npm run check:bundle } finally { Pop-Location }
Write-Output "Phase H gate passed"
