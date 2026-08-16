[CmdletBinding()]
param([switch]$RequirePublishable)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$buildTools = Join-Path $env:LOCALAPPDATA "Android/Sdk/build-tools/36.0.0"
$apk = Join-Path $workspace "companion/app/build/outputs/apk/release/app-release.apk"

function Assert-True {
    param([bool]$Condition, [string]$Code)
    if (-not $Condition) { throw $Code }
}

Push-Location $workspace
try {
    pwsh -NoProfile -File "scripts/companion-phase-gate.ps1"
    if ($LASTEXITCODE -ne 0) { throw "companion_phase_gate_failed" }
    foreach ($script in @("build", "check:imports", "check:dependencies", "check:bundle", "check:security", "report:release")) {
        npm --prefix "webui" run $script
        if ($LASTEXITCODE -ne 0) { throw "companion_webui_release_failed:$script" }
    }
    & "companion/gradlew.bat" --no-configuration-cache -p "companion" lintRelease assembleRelease assembleDebugAndroidTest writeReleaseRuntimeComponents
    if ($LASTEXITCODE -ne 0) { throw "companion_release_build_failed" }
    & (Join-Path $buildTools "apksigner.bat") verify --verbose $apk
    if ($LASTEXITCODE -ne 0) { throw "companion_apk_signature_invalid" }
    $badging = & (Join-Path $buildTools "aapt.exe") dump badging $apk | Out-String
    $entries = @(& (Join-Path $buildTools "aapt.exe") list $apk)
    $components = Get-Content -LiteralPath "companion/app/build/reports/release-runtime-components.txt"
    $source = Get-ChildItem -LiteralPath "companion/app/src/main" -Recurse -File | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -ErrorAction SilentlyContinue } | Out-String

    Assert-True ((Get-Item -LiteralPath $apk).Length -le 2.5MB) "companion_apk_size_exceeded"
    Assert-True ($badging.Contains("package: name='com.jinghumoon.nethop.companion'")) "companion_package_invalid"
    Assert-True (-not $badging.Contains("android.permission.INTERNET")) "companion_internet_permission_forbidden"
    Assert-True (-not $badging.Contains("android.permission.REQUEST_INSTALL_PACKAGES")) "companion_install_permission_forbidden"
    Assert-True ($badging.Contains("android.permission.QUERY_ALL_PACKAGES")) "companion_package_query_permission_missing"
    Assert-True ($entries -contains "assets/fallback/error.html") "companion_fallback_missing"
    Assert-True ($entries -contains "assets/webui-asset-manifest.json") "companion_webui_manifest_missing"
    Assert-True (-not ($entries | Where-Object { $_ -match '^assets/.+\.(?:js|css|svg|woff2)$' })) "companion_contains_webui_bundle"
    Assert-True ($components -contains "com.github.topjohnwu.libsu:core:6.0.0") "companion_libsu_core_missing"
    Assert-True ($components -contains "com.github.topjohnwu.libsu:io:6.0.0") "companion_libsu_io_missing"
    Assert-True (-not ($components | Where-Object { $_ -match 'libsu:service' })) "companion_libsu_service_forbidden"
    Assert-True (-not ($source -match 'addJavascriptInterface|https?://localhost|https?://127\.0\.0\.1')) "companion_forbidden_bridge_or_server"

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($apk)
    try {
        $entry = $archive.GetEntry("assets/webui-asset-manifest.json")
        Assert-True ($null -ne $entry) "companion_webui_manifest_missing"
        $reader = [IO.StreamReader]::new($entry.Open(), [Text.Encoding]::UTF8)
        try { $apkManifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
    }
    finally { $archive.Dispose() }
    $releaseManifest = Get-Content -LiteralPath "artifacts/webui/webui-asset-manifest.json" -Raw
    Assert-True ($apkManifest -ceq $releaseManifest) "companion_webui_identity_mismatch"

    if ($RequirePublishable) {
        $certs = & (Join-Path $buildTools "apksigner.bat") verify --print-certs $apk | Out-String
        Assert-True (-not $certs.Contains("CN=Android Debug")) "companion_publishable_signature_required"
    }
    Write-Host "NetHop Companion release gate passed"
}
finally {
    Pop-Location
}
