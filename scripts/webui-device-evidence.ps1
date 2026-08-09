[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path (Split-Path -Parent $PSScriptRoot) "artifacts/webui/device-evidence.json")
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Path (Split-Path -Parent $OutputPath) -Force | Out-Null

$devices = @(& adb devices | Select-Object -Skip 1 | Where-Object { $_ -match "\sdevice$" })
if ($LASTEXITCODE -ne 0 -or $devices.Count -ne 1) {
    $result = [ordered]@{ schema = "nethop.webui.device-probe.v2"; evidence_kind = "probe_only"; connected = $false; manager = "unknown"; module_installed = $false; status_query = "not_run" }
} else {
    $remoteProbe = 'manager=$(if command -v ksud >/dev/null 2>&1; then echo KernelSU; elif command -v apd >/dev/null 2>&1; then echo APatch; elif command -v magisk >/dev/null 2>&1; then echo Magisk; else echo unknown; fi); echo manager=$manager; echo android_release=$(getprop ro.build.version.release); echo abi=$(getprop ro.product.cpu.abi); if [ -x /data/adb/modules/nethop/bin/nethopctl ]; then echo module_installed=true; else echo module_installed=false; fi; echo webview_version=unknown; echo module_commit=unknown'
    $probeLines = @(& adb shell "su -c '$remoteProbe'")
    if ($LASTEXITCODE -ne 0) { throw "Android root capability probe failed" }
    $probe = @{}
    foreach ($line in $probeLines) {
        $pair = $line.ToString().Trim() -split "=", 2
        if ($pair.Count -eq 2) { $probe[$pair[0]] = $pair[1] }
    }
    if (-not $probe.ContainsKey("manager") -or -not $probe.ContainsKey("android_release") -or -not $probe.ContainsKey("abi") -or -not $probe.ContainsKey("module_installed")) {
        throw "Android root capability probe returned incomplete fields"
    }
    $installed = $probe["module_installed"] -eq "true"
    $webviewDump = (& adb shell "dumpsys webviewupdate" | Out-String)
    $webviewMatch = [regex]::Match($webviewDump, "Current WebView package.*?,\s*([0-9]+(?:\.[0-9]+)+)\)")
    $webviewVersion = if ($webviewMatch.Success) { $webviewMatch.Groups[1].Value } else { "unknown" }
    $moduleCommit = "unknown"
    if ($installed) {
        $manifestText = (& adb shell "su -c 'cat /data/adb/modules/nethop/build-manifest.json 2>/dev/null'" | Out-String).Trim()
        if ($LASTEXITCODE -eq 0 -and $manifestText.Length -gt 0) {
            try { $moduleCommit = ($manifestText | ConvertFrom-Json).nethop_commit } catch { $moduleCommit = "unknown" }
        }
    }
    $status = "not_run"
    if ($installed) {
        & adb shell "su -c '/data/adb/modules/nethop/bin/nethopctl status --json >/dev/null 2>&1'"
        $status = if ($LASTEXITCODE -eq 0) { "passed" } else { "failed" }
    }
    $result = [ordered]@{
        schema = "nethop.webui.device-probe.v2"
        evidence_kind = "probe_only"
        connected = $true
        manager = $probe["manager"]
        android_release = $probe["android_release"]
        abi = $probe["abi"]
        module_installed = $installed
        status_query = $status
        webview_version = $webviewVersion
        module_commit = $moduleCommit
    }
}

$result | ConvertTo-Json | Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
$result | ConvertTo-Json
