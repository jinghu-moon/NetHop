$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$webui = Join-Path $PSScriptRoot "..\webui"
Push-Location $webui
try {
  npm run typecheck
  npm run test:unit -- --run
} finally { Pop-Location }
