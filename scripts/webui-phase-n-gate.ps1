$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
& (Join-Path $PSScriptRoot "webui-phase-m-gate.ps1")
$source = Get-Content -Raw (Join-Path $PSScriptRoot "..\webui\src\views\SettingsView.vue")
foreach ($required in @('config-validate','config-apply','expected_config_digest','config.reload')) { if (-not $source.Contains($required)) { throw "settings transaction missing $required" } }
if ($source -match 'nethop\.toml' -or $source -match 'writeFile') { throw "settings page writes TOML directly" }
Write-Output "Phase N gate passed"
