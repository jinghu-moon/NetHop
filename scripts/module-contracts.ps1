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
    "rulesets/cn-domain.srs",
    "rulesets/cn-ip.srs",
    "bin/.gitkeep",
    "licenses/.gitkeep",
    "webroot/index.html"
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
Assert-True ($default -match '(?m)^schema_version = 3$') "default TOML schema is not frozen"
Assert-True ($default -match '(?m)^enabled = true$') "default service switch drifted"
Assert-True ($default -match '(?m)^\[\[subscriptions\.sources\]\]$') "default source table is missing"
Assert-True ($default -match '(?m)^name = "Primary"$') "default source name drifted"
Assert-True ($default -match '(?m)^url = ""$') "default primary source URL must not ship credentials"
Assert-True (-not $default.Contains('update.glados-config.com')) "default TOML leaks a development subscription"
Assert-True ($default -notmatch '(?m)^id\s*=') "user TOML must not expose source IDs"
Assert-True ($default -match '(?m)^bypass_cn = true$') "default rule mode must bypass audited CN rule sets"
Assert-True ($default -match '(?m)^tun_stack = "gvisor"$') "Android TUN must default to the device-verified gvisor stack"

$service = Get-Content -LiteralPath (Join-Path $module "service.sh") -Raw
Assert-True ($service -match 'MODDIR=\$\{0%/\*\}') "service must derive MODDIR from its own path"
Assert-True ($service -match 'exec "\$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop') "service must exec only the supervisor entrypoint"

$customize = Get-Content -LiteralPath (Join-Path $module "customize.sh") -Raw
$buildScript = Get-Content -LiteralPath (Join-Path $workspace "scripts/build-android-module.ps1") -Raw
# Root managers source customize.sh into the installer process. Enabling
# nounset here would leak into the host and break helpers with optional args.
Assert-True ($customize -notmatch '(?m)^\s*set\s+-[^\r\n#]*u') "installer must not enable nounset in sourced customize.sh"
Assert-True ($customize -notmatch '(?m)^\s*set\s+-o\s+nounset(?:\s|$)') "installer must not enable nounset in sourced customize.sh"
foreach ($asset in @("bin/nethopd", "bin/nethopctl", "bin/sing-box", "rulesets/cn-domain.srs", "rulesets/cn-ip.srs", "build-manifest.json")) {
    Assert-True ($customize.Contains($asset)) "installer checksum allowlist omits $asset"
}
Assert-True ($customize.Contains('verify_asset "$relative"')) "installer does not verify each allowed checksum target"
Assert-True ($customize.Contains('webroot/index.html|webroot/.vite/manifest.json|webroot/assets/*')) "installer does not constrain WebUI checksum targets"
foreach ($asset in @("licenses/webui-sbom.cdx.json", "licenses/webui-licenses.json", "licenses/webui-production-bundle.json", "licenses/webui-bundle-metafile.json", "licenses/webui-asset-manifest.json")) {
    Assert-True ($customize.Contains($asset)) "installer checksum allowlist omits $asset"
}
foreach ($asset in @("companion/nethop-companion.apk", "licenses/companion-sbom.cdx.json", "licenses/companion-licenses.json", "licenses/companion-provenance.json")) {
    Assert-True ($customize.Contains($asset)) "installer checksum allowlist omits $asset"
    Assert-True ($buildScript.Contains($asset)) "build does not package $asset"
}
Assert-True ($customize.Contains("remaining=10")) "Companion prompt does not start at 10 seconds"
Assert-True ($customize.Contains('remaining=$((remaining - 1))')) "Companion prompt does not count down"
Assert-True ($customize.Contains("printf '\r- Waiting for input: %2d seconds'")) "Companion prompt does not prefer carriage-return refresh"
Assert-True ($customize.Contains("timeout 1 getevent -qlc 1")) "Companion key read is not bounded to one second"
Assert-True ($customize.Contains("*KEY_VOLUMEUP*DOWN*")) "Companion installer does not require Volume+ key-down"
Assert-True ($customize.Contains("*KEY_VOLUMEDOWN*DOWN*")) "Companion installer does not require Volume- key-down"
Assert-True ($customize.Contains('pm install -r --user 0 "$COMPANION_APK"')) "Companion installer does not use user-0 replacement"
Assert-True ($customize.Contains('if companion_installed; then')) "Companion installer does not update an existing package"
Assert-True ($customize.Contains('rm -f "$COMPANION_APK"')) "Companion staging APK is not removed"
foreach ($asset in @("licenses/Unicode-3.0.txt", "licenses/country-flag-icons-MIT.txt")) {
    Assert-True ($customize.Contains($asset)) "installer checksum allowlist omits territory license $asset"
    Assert-True ($buildScript.Contains($asset.Substring("licenses/".Length))) "build does not package territory license $asset"
}
foreach ($asset in @("cn-domain.srs", "cn-ip.srs")) {
    Assert-True ($customize.Contains("publish_persistent_asset `"rulesets/$asset`" `"`$DATA_ROOT/rulesets/$asset`"")) "installer does not publish persistent $asset"
}
Assert-True ($customize.Contains('temporary="${destination}.new"')) "persistent asset publication does not use a same-directory temporary file"
Assert-True ($customize.Contains('mv -f "$temporary" "$destination"')) "persistent asset publication is not atomic"
Assert-True ($customize.Contains('[ ! -L "$destination" ]')) "persistent asset publication does not reject symlink targets"
Assert-True ($customize.Contains('chown 0:0 "$temporary" || fail')) "persistent asset staging is not root-owned"
Assert-True ($customize.Contains('chmod 0600 "$temporary" || fail')) "persistent asset staging is not private"
Assert-True ($customize.Contains('[ "${API:-0}" -ge 33 ]')) "installer does not enforce API 33"
Assert-True ($customize.Contains('[ "${ARCH:-}" = "arm64" ]')) "installer does not enforce arm64"
Assert-True ($customize.Contains('if [ ! -e "$DATA_ROOT/config/nethop.toml" ]; then')) "installer does not initialize managed config"
Assert-True ($customize.Contains('CONFIG_SCHEMA_VERSION=3')) "installer schema ABI drifted from the default config"
Assert-True ($customize.Contains('${CONFIG_SCHEMA_VERSION}[[:space:]]*$')) "installer does not validate the current config ABI"
Assert-True ($customize.Contains('nethop.toml.pre-v3')) "installer does not preserve one private pre-v3 backup"
Assert-True ($customize.Contains('chmod 0600 "$DATA_ROOT/config/nethop.toml"')) "installer does not protect the managed config"
Assert-True ($customize.Contains('  "$DATA_ROOT/subscriptions" \')) "installer does not protect the manual-source parent directory"
Assert-True ($customize.Contains('ln -s "$DATA_ROOT/config/nethop.toml" "$MODPATH/config/nethop.toml"')) "installer does not publish the controlled config link"
Assert-True (-not $customize.Contains('nethop.json')) "installer retains the removed JSON worker config"
Assert-True (-not $customize.Contains('sources.json')) "installer retains the removed JSON source config"

Assert-True ($buildScript.Contains('scripts/fake-magisk-smoke.ps1')) "build does not run the persistent-config upgrade smoke test"
Assert-True ($buildScript.Contains('[IO.File]::WriteAllText($checksumPath, (($checksumEntries -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))')) "build does not force an LF-only checksum manifest"
Assert-True ($buildScript.Contains('$checksumBytes.Contains([byte]0x0D)')) "build does not reject CR bytes in the checksum manifest"
Assert-True ($buildScript.Contains('$checksumBytes[0] -eq 0xEF')) "build does not reject a UTF-8 BOM in the checksum manifest"
Assert-True ($buildScript.Contains('"lintRelease"')) "module build does not run Companion release lint"
Assert-True ($buildScript.Contains('"assembleRelease"')) "module build does not create the minified Companion APK"
Assert-True ($buildScript.Contains('"assembleDebugAndroidTest"')) "module build does not compile Companion instrumentation tests"
Assert-True ($buildScript.Contains('apksigner')) "module build does not verify Companion signing"
Assert-True ($buildScript.Contains('webui_identity_sha256')) "module manifest does not bind Companion to the WebUI identity"
Assert-True ($buildScript.Contains('$companionIncrementBytes -gt 3MB')) "module build does not enforce the Companion ZIP increment budget"
Assert-True ($buildScript.Contains('module archive entry must occur exactly once')) "module build does not reject duplicate release entries"
Assert-True ($buildScript.Contains('module archive must contain exactly one APK')) "module build does not reject extra APK payloads"

$action = Get-Content -LiteralPath (Join-Path $module "action.sh") -Raw
Assert-True ($action.Contains('"$CTL" config reload')) "action does not force a config reload"
Assert-True ($action.Contains('"$CTL" update')) "action does not retry subscription update"
Assert-True ($action.Contains('"$CTL" status')) "action does not render status"
Assert-True ($action.Contains('"$CTL" status --human')) "action does not render human-readable status"

$uninstall = Get-Content -LiteralPath (Join-Path $module "uninstall.sh") -Raw
Assert-True ($uninstall.Contains('stat_fields=${stat_line##*) }')) "uninstaller does not isolate proc stat fields"
Assert-True ($uninstall.Contains("awk '{print `$20}'")) "uninstaller does not verify process start time"
Assert-True ($uninstall.Contains('rm -rf "$DATA_ROOT"')) "uninstaller does not remove the exact persistent root"
Assert-True (-not $uninstall.Contains("pm uninstall")) "module uninstall must not uninstall Companion"

$allShell = Get-ChildItem -LiteralPath $module -Filter "*.sh" | ForEach-Object {
    Get-Content -LiteralPath $_.FullName -Raw
}
$joined = $allShell -join "`n"
foreach ($forbidden in @("iptables", "ip6tables", "nft ", "curl ", "wget ", "http://", "https://")) {
    Assert-True (-not $joined.Contains($forbidden)) "module shell contains forbidden business/network command: $forbidden"
}

Write-Host "NetHop module contracts passed"
