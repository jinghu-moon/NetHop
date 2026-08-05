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
    "defaults/nethop.toml",
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

$default = Get-Content -LiteralPath (Join-Path $module "defaults/nethop.toml") -Raw
Assert-True ($default -match '(?m)^schema_version = 1$') "default TOML schema is not frozen"
Assert-True ($default -match '(?m)^enabled = true$') "default service switch drifted"
Assert-True ($default -match '(?m)^\[\[subscriptions\.sources\]\]$') "default source table is missing"
Assert-True ($default -match '(?m)^name = "Primary"$') "default source name drifted"
Assert-True ($default -match '(?m)^url = ""$') "default source URL must be empty"
Assert-True ($default -notmatch '(?m)^id\s*=') "user TOML must not expose source IDs"

$service = Get-Content -LiteralPath (Join-Path $module "service.sh") -Raw
Assert-True ($service -match 'MODDIR=\$\{0%/\*\}') "service must derive MODDIR from its own path"
Assert-True ($service -match 'exec "\$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop') "service must exec only the supervisor entrypoint"

$customize = Get-Content -LiteralPath (Join-Path $module "customize.sh") -Raw
# Root managers source customize.sh into the installer process. Enabling
# nounset here would leak into the host and break helpers with optional args.
Assert-True ($customize -notmatch '(?m)^\s*set\s+-[^\r\n#]*u') "installer must not enable nounset in sourced customize.sh"
Assert-True ($customize -notmatch '(?m)^\s*set\s+-o\s+nounset(?:\s|$)') "installer must not enable nounset in sourced customize.sh"
foreach ($asset in @("bin/nethopd", "bin/nethopctl", "bin/sing-box", "build-manifest.json")) {
    Assert-True ($customize.Contains("verify_asset `"$asset`"")) "installer does not verify $asset"
}
Assert-True ($customize.Contains('[ "${API:-0}" -ge 33 ]')) "installer does not enforce API 33"
Assert-True ($customize.Contains('[ "${ARCH:-}" = "arm64" ]')) "installer does not enforce arm64"
Assert-True ($customize.Contains('if [ ! -e "$DATA_ROOT/config/nethop.toml" ]; then')) "installer may overwrite managed config"
Assert-True ($customize.Contains('chmod 0600 "$DATA_ROOT/config/nethop.toml"')) "installer does not protect the managed config"
Assert-True ($customize.Contains('ln -s "$DATA_ROOT/config/nethop.toml" "$MODPATH/config/nethop.toml"')) "installer does not publish the controlled config link"
Assert-True (-not $customize.Contains('nethop.json')) "installer retains the removed JSON worker config"
Assert-True (-not $customize.Contains('sources.json')) "installer retains the removed JSON source config"

$action = Get-Content -LiteralPath (Join-Path $module "action.sh") -Raw
Assert-True ($action.Contains('"$CTL" config reload')) "action does not force a config reload"
Assert-True ($action.Contains('"$CTL" update')) "action does not retry subscription update"
Assert-True ($action.Contains('"$CTL" status')) "action does not render status"

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
