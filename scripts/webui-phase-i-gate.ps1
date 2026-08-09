$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-h-gate.ps1")
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run check:security } finally { Pop-Location }
Write-Output "Phase I gate passed"
