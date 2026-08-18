[CmdletBinding()]
param(
    [string]$EvidenceRoot = "artifacts/application-icons"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$started = Get-Date
$results = [System.Collections.Generic.List[object]]::new()

function Invoke-GateCommand {
    param([string]$Id, [string]$File, [string[]]$Arguments, [string]$WorkingDirectory = $root)
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    Push-Location $WorkingDirectory
    try {
        & $File @Arguments *> $null
        $exitCode = $LASTEXITCODE
    } finally { Pop-Location }
    $timer.Stop()
    $displayFile = if ($File.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) { $File.Substring($root.Length).TrimStart('\', '/') } else { $File }
    $results.Add([ordered]@{ id = $Id; command = ($displayFile + " " + ($Arguments -join " ")); exit_code = $exitCode; elapsed_ms = $timer.ElapsedMilliseconds })
    if ($exitCode -ne 0) { throw "gate failed: $Id ($exitCode)" }
}

New-Item -ItemType Directory -Force -Path (Join-Path $root $EvidenceRoot) | Out-Null
Invoke-GateCommand "rust-provider" "cargo" @("test", "-p", "nethop-android")
Invoke-GateCommand "protocol-tests" "cargo" @("test", "-p", "nethop-protocol")
Invoke-GateCommand "daemon-tests" "cargo" @("test", "-p", "nethopd")
Invoke-GateCommand "cli-tests" "cargo" @("test", "-p", "nethopctl")
Invoke-GateCommand "webui-gate" "npm" @("--prefix", "webui", "run", "gate")
Invoke-GateCommand "companion-unit" (Join-Path $root "companion/gradlew.bat") @("--no-configuration-cache", "-p", "companion", "testDebugUnitTest")
Invoke-GateCommand "companion-kotlin" (Join-Path $root "companion/gradlew.bat") @("--no-configuration-cache", "-p", "companion", ":app:compileDebugKotlin")
Invoke-GateCommand "module-contracts" "pwsh" @("-NoProfile", "-File", "scripts/module-contracts.ps1")

$adb = Get-Command adb -ErrorAction SilentlyContinue
$deviceState = if ($null -eq $adb) { "not_run_device_unavailable" } else {
    $deviceLines = & $adb.Source devices 2>$null
    if ($deviceLines | Select-String "`tdevice$" -Quiet) { "available_manual_acceptance_required" } else { "not_run_device_unavailable" }
}
$manifest = [ordered]@{
    schema_version = 1
    task = "D20-application-icons"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    contains_sensitive_data = $false
    host_results = $results
    release = [ordered]@{
        module_zip = $null
        module_zip_sha256 = $null
        module_bytes = $null
        companion_apk_sha256 = $null
        companion_apk_bytes = $null
    }
    device_status = $deviceState
    git_dirty = $true
}
$moduleZip = Get-ChildItem -LiteralPath (Join-Path $root "out/android-arm64") -Filter "*.zip" -File -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -Last 1
$apk = Join-Path $root "companion/app/build/outputs/apk/release/app-release.apk"
if ($null -ne $moduleZip) {
    $manifest.release.module_zip = "out/android-arm64/$($moduleZip.Name)"
    $manifest.release.module_zip_sha256 = (Get-FileHash -LiteralPath $moduleZip.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest.release.module_bytes = $moduleZip.Length
}
if (Test-Path -LiteralPath $apk -PathType Leaf) {
    $manifest.release.companion_apk_sha256 = (Get-FileHash -LiteralPath $apk -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest.release.companion_apk_bytes = (Get-Item -LiteralPath $apk).Length
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $root "$EvidenceRoot/host-gate-manifest.json")
Write-Output ($manifest | ConvertTo-Json -Depth 6)
