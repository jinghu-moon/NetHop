$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-n-gate.ps1")
$source = Get-Content -Raw (Join-Path $PSScriptRoot "..\webui\src\views\OperationsView.vue")
foreach ($required in @('connection.close','connections.close-all','logs.clear','diagnostics.bundle','topology.get','ruleset.update','core.version-check','backup.export','backup-restore')) { if (-not $source.Contains($required)) { throw "operations coverage missing $required" } }
Push-Location (Join-Path $PSScriptRoot "..\webui")
try { npm run build; npm run check:bundle; npm run check:security } finally { Pop-Location }
Write-Output "Phase O gate passed"
