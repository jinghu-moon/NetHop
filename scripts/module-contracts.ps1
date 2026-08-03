[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$module = Join-Path $workspace "module"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

$required = @(
    "module.prop",
    "customize.sh",
    "service.sh",
    "action.sh",
    "uninstall.sh",
    "defaults/nethop.json",
    "bin/.gitkeep",
    "licenses/.gitkeep"
)
foreach ($relative in $required) {
    Assert-True (Test-Path -LiteralPath (Join-Path $module $relative) -PathType Leaf) "missing module file: $relative"
}

$properties = @{}
foreach ($line in Get-Content -LiteralPath (Join-Path $module "module.prop")) {
    if ($line -match '^([^=]+)=(.*)$') {
        $properties[$Matches[1]] = $Matches[2]
    }
}
Assert-True ($properties.id -eq "nethop") "module id must be nethop"
Assert-True ($properties.versionCode -match '^[1-9][0-9]*$') "versionCode must be positive"

$default = Get-Content -LiteralPath (Join-Path $module "defaults/nethop.json") -Raw | ConvertFrom-Json
Assert-True ($default.schema -eq "nethop-worker-v1") "default worker schema is not frozen"
Assert-True ($default.inbound_port -eq 7893) "default inbound port drifted"
Assert-True ($default.allocations.Count -ge 1 -and $default.allocations.Count -le 16) "default allocation count is invalid"

$service = Get-Content -LiteralPath (Join-Path $module "service.sh") -Raw
Assert-True ($service -match 'MODDIR=\$\{0%/\*\}') "service must derive MODDIR from its own path"
Assert-True ($service -match 'exec "\$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop') "service must exec only the supervisor entrypoint"

$customize = Get-Content -LiteralPath (Join-Path $module "customize.sh") -Raw
foreach ($asset in @("bin/nethopd", "bin/nethopctl", "bin/sing-box", "build-manifest.json")) {
    Assert-True ($customize.Contains("verify_asset `"$asset`"")) "installer does not verify $asset"
}
Assert-True ($customize.Contains('[ "${API:-0}" -ge 33 ]')) "installer does not enforce API 33"
Assert-True ($customize.Contains('[ "${ARCH:-}" = "arm64" ]')) "installer does not enforce arm64"
Assert-True ($customize.Contains('if [ ! -e "$DATA_ROOT/config/nethop.json" ]; then')) "installer may overwrite managed config"

$action = Get-Content -LiteralPath (Join-Path $module "action.sh") -Raw
Assert-True ($action.Contains('"$CTL" stop')) "action does not support stopping"
Assert-True ($action.Contains('"$CTL" start')) "action does not support starting"

$uninstall = Get-Content -LiteralPath (Join-Path $module "uninstall.sh") -Raw
Assert-True ($uninstall.Contains('stat_fields=${stat_line##*) }')) "uninstaller does not isolate proc stat fields"
Assert-True ($uninstall.Contains("awk '{print `$20}'")) "uninstaller does not verify process start time"
Assert-True ($uninstall.Contains('rm -rf "$DATA_ROOT"')) "uninstaller does not remove the exact persistent root"

$allShell = Get-ChildItem -LiteralPath $module -Filter "*.sh" | ForEach-Object {
    Get-Content -LiteralPath $_.FullName -Raw
}
$joined = $allShell -join "`n"
foreach ($forbidden in @("iptables", "ip6tables", "nft ", "curl ", "wget ", "http://", "https://")) {
    Assert-True (-not $joined.Contains($forbidden)) "module shell contains forbidden business/network command: $forbidden"
}

Write-Host "NetHop module contracts passed"
