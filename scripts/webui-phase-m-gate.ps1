$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-l-gate.ps1")
$source = Get-Content -Raw (Join-Path $PSScriptRoot "..\webui\src\views\ApplicationsView.vue")
if ($source -match 'pm list packages' -or $source -match 'setInterval') { throw "application page bypasses typed host or uses polling" }
if ($source -notmatch 'replace_application_targets' -or $source -notmatch 'uid === 0') { throw "application mutation or root UID guard missing" }
Write-Output "Phase M gate passed"
