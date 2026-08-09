$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-i-gate.ps1")
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run typecheck; npm run test:unit -- --run } finally { Pop-Location }
Write-Output "Phase J gate passed"
