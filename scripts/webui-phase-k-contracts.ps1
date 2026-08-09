$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$operations = Get-Content -Raw (Join-Path $PSScriptRoot "..\webui\src\views\SubscriptionsView.vue")
if ($operations -match 'request_profile' -or $operations -match 'source[_ -]?id.*input') { throw "subscription UI exposes internal fields" }
if ($operations -notmatch 'config-mutate' -or $operations -notmatch 'expected_config_digest') { throw "subscription mutations are not CAS-bound private payloads" }
Write-Output "Phase K static contracts passed"
