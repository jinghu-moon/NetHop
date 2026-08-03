[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $workspace "scripts/build-android-module.ps1"
$source = Get-Content -LiteralPath $scriptPath -Raw

function Assert-Contains([string]$Needle, [string]$Message) {
    if (-not $source.Contains($Needle)) { throw $Message }
}

Assert-Contains 'sing-box-1.13.15-mapping.json' "build must consume the frozen mapping manifest"
Assert-Contains 'rev-parse", "HEAD"' "build must pin the sing-box source commit"
Assert-Contains 'describe", "--tags", "--exact-match"' "build must require an exact sing-box tag"
Assert-Contains 'status", "--porcelain", "--untracked-files=no"' "build must reject dirty tracked source"
Assert-Contains 'Go $($mapping.go_version) is required' "build must reject Go version drift by default"
Assert-Contains 'development_override = (-not $goVersionMatches) -and -not [bool]$SingBoxArchive' "development override must be observable"
Assert-Contains 'official_prebuilt' "build must distinguish official prebuilt core input"
Assert-Contains 'vcs\.revision=([0-9a-f]{40})' "prebuilt core must expose its source revision"
Assert-Contains 'with_gvisor", "with_quic", "with_utls", "with_clash_api' "prebuilt core must carry the minimum Android tags"
Assert-Contains 'stats_attribution_patch = $false' "unpatched upstream core must not claim terminal attribution"
Assert-Contains 'reproducible = ($coreOrigin -eq "source") -and $goVersionMatches' "prebuilt core must not claim source reproducibility"
Assert-Contains 'Machine:\s+AArch64' "build must verify ELF architecture"
Assert-Contains 'checksums.sha256' "build must publish the installer checksum manifest"
Assert-Contains 'staged asset checksum verification failed' "build must reverify staged assets"
Assert-Contains 'Compress-Archive' "build must generate an installable module archive"
Assert-Contains 'module archive is missing' "build must validate archive root layout"
Assert-Contains 'out/android-arm64' "build output must stay outside the source template"

Write-Host "NetHop Android build contracts passed"
