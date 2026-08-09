$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$root = Join-Path $PSScriptRoot ".."
& (Join-Path $PSScriptRoot "webui-phase-g-contracts.ps1")
Push-Location (Join-Path $root "webui")
try {
  npm run check:imports
  npm run check:dependencies
} finally { Pop-Location }
Write-Output "Phase G gate passed"
