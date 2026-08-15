[CmdletBinding()]
param(
    [string]$SingBoxSource = "refer/sing-box-v1.13.15",
    [string]$SingBoxArchive,
    [string]$NdkVersion = "29.0.14206865",
    [string]$OutputDirectory = "out/android-arm64",
    [switch]$AllowGoVersionMismatch
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$mappingPath = Join-Path $workspace "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json"
$moduleTemplate = Join-Path $workspace "module"
$webui = Join-Path $workspace "webui"
$outputRoot = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
$workspaceRoot = [IO.Path]::GetFullPath($workspace).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if (-not $outputRoot.StartsWith($workspaceRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "module output must stay inside the workspace"
}
$stage = Join-Path $outputRoot "module"
$ndk = Join-Path $env:LOCALAPPDATA "Android/Sdk/ndk/$NdkVersion/toolchains/llvm/prebuilt/windows-x86_64"
$clang = Join-Path $ndk "bin/aarch64-linux-android23-clang.cmd"
$clangxx = Join-Path $ndk "bin/aarch64-linux-android23-clang++.cmd"
$ar = Join-Path $ndk "bin/llvm-ar.exe"
$readelf = Join-Path $ndk "bin/llvm-readelf.exe"
$target = "aarch64-linux-android"

function Invoke-Checked {
    param(
        [Parameter(Mandatory)][string]$Program,
        [Parameter(Mandatory)][string[]]$Arguments,
        [string]$WorkingDirectory = $workspace
    )
    Push-Location $WorkingDirectory
    try {
        & $Program @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "command failed ($LASTEXITCODE): $Program $($Arguments -join ' ')"
        }
    }
    finally {
        Pop-Location
    }
}

function Get-GitValue {
    param([string]$Repository, [string[]]$Arguments)
    $value = & git -C $Repository @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git query failed for $Repository"
    }
    return ($value | Out-String).Trim()
}

function Get-Sha256 {
    param([string]$Path)
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-TreeSha256 {
    param([string[]]$Paths, [string]$Root)
    $records = foreach ($path in $Paths) {
        if (Test-Path -LiteralPath $path -PathType Container) {
            Get-ChildItem -LiteralPath $path -Recurse -File | ForEach-Object {
                "$([IO.Path]::GetRelativePath($Root, $_.FullName).Replace('\', '/')) $(Get-Sha256 $_.FullName)"
            }
        } else {
            "$([IO.Path]::GetRelativePath($Root, $path).Replace('\', '/')) $(Get-Sha256 $path)"
        }
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes((($records | Sort-Object) -join "`n"))
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}

foreach ($required in @($mappingPath, $moduleTemplate, (Join-Path $webui "package.json"), (Join-Path $webui "package-lock.json"), $clang, $clangxx, $ar, $readelf)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "required build input is missing: $required"
    }
}

Invoke-Checked npm @("ci", "--ignore-scripts") $webui
foreach ($script in @("build", "check:imports", "check:dependencies", "check:bundle", "check:security", "report:release")) {
    Invoke-Checked npm @("run", $script) $webui
}
$webroot = Join-Path $moduleTemplate "webroot"
if (-not (Test-Path -LiteralPath (Join-Path $webroot "index.html") -PathType Leaf)) {
    throw "WebUI production build did not publish module/webroot/index.html"
}
$webuiSourceSha256 = Get-TreeSha256 -Paths @(
    (Join-Path $webui "src"), (Join-Path $webui "package.json"),
    (Join-Path $webui "package-lock.json"), (Join-Path $webui "vite.config.ts"),
    (Join-Path $webui "webui-budget.json"), (Join-Path $webui "scripts")
) -Root $workspace
$webuiPackage = Get-Content -LiteralPath (Join-Path $webui "package.json") -Raw | ConvertFrom-Json
$webuiArtifactRoot = Join-Path $workspace "artifacts/webui"
foreach ($requiredArtifact in @("production-bundle.json", "bundle-metafile.json", "webui-sbom.cdx.json", "webui-licenses.json")) {
    if (-not (Test-Path -LiteralPath (Join-Path $webuiArtifactRoot $requiredArtifact) -PathType Leaf)) {
        throw "WebUI release artifact is missing: $requiredArtifact"
    }
}

$singBoxSourcePath = (Resolve-Path -LiteralPath (Join-Path $workspace $SingBoxSource)).Path
$mapping = Get-Content -LiteralPath $mappingPath -Raw | ConvertFrom-Json
$sourceCommit = Get-GitValue $singBoxSourcePath @("rev-parse", "HEAD")
$sourceTag = Get-GitValue $singBoxSourcePath @("describe", "--tags", "--exact-match")
$sourceDirty = Get-GitValue $singBoxSourcePath @("status", "--porcelain", "--untracked-files=no")
if ($sourceCommit -ne $mapping.sing_box_commit -or $sourceTag -ne $mapping.sing_box_tag) {
    throw "sing-box source does not match the frozen mapping manifest"
}
if ($sourceDirty) {
    throw "sing-box source must be clean"
}

$goVersionOutput = (& go version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $goVersionOutput -notmatch '^go version go([^ ]+) ') {
    throw "Go toolchain is unavailable or has an unexpected version string"
}
$actualGoVersion = $Matches[1]
$goVersionMatches = $actualGoVersion -eq $mapping.go_version
if (-not $SingBoxArchive -and -not $goVersionMatches -and -not $AllowGoVersionMismatch) {
    throw "Go $($mapping.go_version) is required; found $actualGoVersion. Use -AllowGoVersionMismatch only for local smoke builds."
}

$rustCommit = Get-GitValue $workspace @("rev-parse", "HEAD")
$rustDirty = Get-GitValue $workspace @("status", "--porcelain", "--untracked-files=no")
$buildTags = @($mapping.build_tags) -join ","
$sharedLdflags = (Get-Content -LiteralPath (Join-Path $singBoxSourcePath "release/LDFLAGS") -Raw).Trim()
$ldflags = "-X github.com/sagernet/sing-box/constant.Version=$($mapping.sing_box_version) $sharedLdflags -s -w -buildid="
$coreOrigin = "source"
$coreArchiveSha256 = $null
$coreGoVersion = $actualGoVersion
$coreBuildTags = @($mapping.build_tags)
$coreProvenanceVerified = $goVersionMatches

if (Test-Path -LiteralPath $outputRoot) {
    Remove-Item -LiteralPath $outputRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item -Path (Join-Path $moduleTemplate "*") -Destination $stage -Recurse -Force
New-Item -ItemType Directory -Path (Join-Path $stage "bin") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "licenses") -Force | Out-Null
Get-ChildItem -LiteralPath $stage -Recurse -Force -Filter ".gitkeep" | Remove-Item -Force

$env:CC_aarch64_linux_android = $clang
$env:CXX_aarch64_linux_android = $clangxx
$env:AR_aarch64_linux_android = $ar
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
Invoke-Checked cargo @("build", "--release", "--locked", "--target", $target, "-p", "nethopd", "-p", "nethopctl")
Copy-Item -LiteralPath (Join-Path $workspace "target/$target/release/nethopd") -Destination (Join-Path $stage "bin/nethopd")
Copy-Item -LiteralPath (Join-Path $workspace "target/$target/release/nethopctl") -Destination (Join-Path $stage "bin/nethopctl")

if ($SingBoxArchive) {
    $archivePath = (Resolve-Path -LiteralPath (Join-Path $workspace $SingBoxArchive)).Path
    $listing = @(& tar -tzf $archivePath)
    if ($LASTEXITCODE -ne 0 -or $listing.Count -ne 3) {
        throw "sing-box archive must contain one root directory, LICENSE, and sing-box"
    }
    $rootEntry = $listing | Where-Object { $_ -match '/$' }
    $binaryEntry = $listing | Where-Object { $_ -match '/sing-box$' }
    $licenseEntry = $listing | Where-Object { $_ -match '/LICENSE$' }
    if (@($rootEntry).Count -ne 1 -or @($binaryEntry).Count -ne 1 -or @($licenseEntry).Count -ne 1) {
        throw "sing-box archive layout is invalid"
    }
    $extractRoot = Join-Path $outputRoot "sing-box-prebuilt"
    New-Item -ItemType Directory -Path $extractRoot -Force | Out-Null
    Invoke-Checked tar @("-xzf", $archivePath, "-C", $extractRoot)
    $prebuiltBinary = Join-Path $extractRoot $binaryEntry
    $prebuiltLicense = Join-Path $extractRoot $licenseEntry
    $buildInfo = (& go version -m $prebuiltBinary | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "sing-box prebuilt Go metadata is unavailable"
    }
    $moduleVersion = [regex]::Match($buildInfo, '(?m)^\s*mod\s+github\.com/sagernet/sing-box\s+v([^\s]+)')
    $revision = [regex]::Match($buildInfo, '(?m)^\s*build\s+vcs\.revision=([0-9a-f]{40})')
    $goBuildVersion = [regex]::Match($buildInfo, '(?m)^.*sing-box:\s+go([^\s]+)\r?$')
    $tagsMatch = [regex]::Match($buildInfo, '(?m)^\s*build\s+-tags=([^\r\n]+)\r?$')
    if (-not $moduleVersion.Success -or $moduleVersion.Groups[1].Value -ne $mapping.sing_box_version -or
        -not $revision.Success -or $revision.Groups[1].Value -ne $mapping.sing_box_commit -or
        -not $goBuildVersion.Success -or -not $tagsMatch.Success -or
        $buildInfo -notmatch '(?m)^\s*build\s+CGO_ENABLED=1\r?$' -or
        $buildInfo -notmatch '(?m)^\s*build\s+GOOS=android\r?$' -or
        $buildInfo -notmatch '(?m)^\s*build\s+GOARCH=arm64\r?$') {
        throw "sing-box prebuilt metadata does not match the frozen Android target"
    }
    $coreBuildTags = @($tagsMatch.Groups[1].Value -split ',')
    foreach ($requiredTag in @("with_gvisor", "with_quic", "with_utls", "with_clash_api")) {
        if ($coreBuildTags -notcontains $requiredTag) {
            throw "sing-box prebuilt is missing required tag: $requiredTag"
        }
    }
    Copy-Item -LiteralPath $prebuiltBinary -Destination (Join-Path $stage "bin/sing-box")
    Copy-Item -LiteralPath $prebuiltLicense -Destination (Join-Path $stage "licenses/sing-box-GPL-3.0.txt")
    $coreOrigin = "official_prebuilt"
    $coreArchiveSha256 = Get-Sha256 $archivePath
    $coreGoVersion = $goBuildVersion.Groups[1].Value
    $coreProvenanceVerified = $true
}
else {
    $oldGoEnvironment = @{
        GOOS = $env:GOOS
        GOARCH = $env:GOARCH
        CGO_ENABLED = $env:CGO_ENABLED
        CC = $env:CC
        CXX = $env:CXX
        GOTOOLCHAIN = $env:GOTOOLCHAIN
    }
    try {
        $env:GOOS = "android"
        $env:GOARCH = "arm64"
        $env:CGO_ENABLED = "1"
        $env:CC = $clang
        $env:CXX = $clangxx
        $env:GOTOOLCHAIN = "local"
        Invoke-Checked go @(
            "build", "-trimpath",
            "-tags", $buildTags,
            "-ldflags", $ldflags,
            "-o", (Join-Path $stage "bin/sing-box"),
            "./cmd/sing-box"
        ) $singBoxSourcePath
    }
    finally {
        foreach ($entry in $oldGoEnvironment.GetEnumerator()) {
            Set-Item -Path "Env:$($entry.Key)" -Value $entry.Value
        }
    }
}

foreach ($binary in @("nethopd", "nethopctl", "sing-box")) {
    $path = Join-Path $stage "bin/$binary"
    $header = & $readelf -h $path 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $header -notmatch 'Machine:\s+AArch64') {
        throw "$binary is not an AArch64 ELF binary"
    }
}

Copy-Item -LiteralPath (Join-Path $workspace "LICENSE") -Destination (Join-Path $stage "licenses/NetHop-AGPL-3.0.txt")
if (-not $SingBoxArchive) {
    Copy-Item -LiteralPath (Join-Path $singBoxSourcePath "LICENSE") -Destination (Join-Path $stage "licenses/sing-box-GPL-3.0.txt")
}
Copy-Item -LiteralPath (Join-Path $workspace "licenses/Unicode-3.0.txt") -Destination (Join-Path $stage "licenses/Unicode-3.0.txt")
Copy-Item -LiteralPath (Join-Path $workspace "licenses/country-flag-icons-MIT.txt") -Destination (Join-Path $stage "licenses/country-flag-icons-MIT.txt")
Copy-Item -LiteralPath (Join-Path $webuiArtifactRoot "webui-sbom.cdx.json") -Destination (Join-Path $stage "licenses/webui-sbom.cdx.json")
Copy-Item -LiteralPath (Join-Path $webuiArtifactRoot "webui-licenses.json") -Destination (Join-Path $stage "licenses/webui-licenses.json")
Copy-Item -LiteralPath (Join-Path $webuiArtifactRoot "production-bundle.json") -Destination (Join-Path $stage "licenses/webui-production-bundle.json")
Copy-Item -LiteralPath (Join-Path $webuiArtifactRoot "bundle-metafile.json") -Destination (Join-Path $stage "licenses/webui-bundle-metafile.json")

$binaryRecords = foreach ($binary in @("nethopd", "nethopctl", "sing-box")) {
    $path = Join-Path $stage "bin/$binary"
    [ordered]@{
        path = "bin/$binary"
        bytes = (Get-Item -LiteralPath $path).Length
        sha256 = Get-Sha256 $path
    }
}
$ruleSetRecords = foreach ($ruleSet in @("cn-domain.srs", "cn-ip.srs")) {
    $path = Join-Path $stage "rulesets/$ruleSet"
    [ordered]@{
        path = "rulesets/$ruleSet"
        bytes = (Get-Item -LiteralPath $path).Length
        sha256 = Get-Sha256 $path
    }
}
$webuiRecords = Get-ChildItem -LiteralPath (Join-Path $stage "webroot") -Recurse -File | ForEach-Object {
    [ordered]@{
        path = [IO.Path]::GetRelativePath($stage, $_.FullName).Replace('\', '/')
        bytes = $_.Length
        sha256 = Get-Sha256 $_.FullName
    }
}
$webuiMetadataRecords = Get-ChildItem -LiteralPath (Join-Path $stage "licenses") -File | Where-Object {
    $_.Name -like "webui-*" -or $_.Name -in @("Unicode-3.0.txt", "country-flag-icons-MIT.txt")
} | ForEach-Object {
    [ordered]@{
        path = [IO.Path]::GetRelativePath($stage, $_.FullName).Replace('\', '/')
        bytes = $_.Length
        sha256 = Get-Sha256 $_.FullName
    }
}
$manifest = [ordered]@{
    schema = "nethop.android-build.v1"
    target = $target
    android_min_api = 33
    ndk_version = $NdkVersion
    rustc = (& rustc -Vv | Out-String).Trim()
    cargo = (& cargo -V | Out-String).Trim()
    nethop_commit = $rustCommit
    nethop_worktree_clean = -not [bool]$rustDirty
    sing_box_version = $mapping.sing_box_version
    sing_box_tag = $sourceTag
    sing_box_commit = $sourceCommit
    sing_box_go_required = $mapping.go_version
    sing_box_go_actual = $actualGoVersion
    sing_box_core_origin = $coreOrigin
    sing_box_archive_sha256 = $coreArchiveSha256
    sing_box_core_go = $coreGoVersion
    sing_box_build_tags = @($coreBuildTags)
    sing_box_ldflags = $ldflags
    sing_box_provenance_verified = $coreProvenanceVerified
    stats_attribution_patch = $false
    mapping_manifest_sha256 = Get-Sha256 $mappingPath
    reproducible = ($coreOrigin -eq "source") -and $goVersionMatches -and -not [bool]$rustDirty
    development_override = (-not $goVersionMatches) -and -not [bool]$SingBoxArchive
    binaries = @($binaryRecords)
    rule_sets = @($ruleSetRecords)
    webui = [ordered]@{
        version = $webuiPackage.version
        source_sha256 = $webuiSourceSha256
        asset_sha256 = Get-TreeSha256 -Paths @((Join-Path $stage "webroot")) -Root $stage
        vue = $webuiPackage.dependencies.vue
        tdesign_mobile_vue = $webuiPackage.dependencies.'tdesign-mobile-vue'
        tabler_icons_vue = $webuiPackage.dependencies.'@tabler/icons-vue'
        tanstack_vue_virtual = $webuiPackage.dependencies.'@tanstack/vue-virtual'
        assets = @($webuiRecords)
        release_metadata = @($webuiMetadataRecords)
    }
}
$manifestPath = Join-Path $stage "build-manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

$checksumEntries = @($binaryRecords | ForEach-Object { "$($_.sha256)  $($_.path)" })
$checksumEntries += @($ruleSetRecords | ForEach-Object { "$($_.sha256)  $($_.path)" })
$checksumEntries += @($webuiRecords | ForEach-Object { "$($_.sha256)  $($_.path)" })
$checksumEntries += @($webuiMetadataRecords | ForEach-Object { "$($_.sha256)  $($_.path)" })
$checksumEntries += "$(Get-Sha256 $manifestPath)  build-manifest.json"
$checksumPath = Join-Path $stage "checksums.sha256"
[IO.File]::WriteAllText($checksumPath, (($checksumEntries -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
$checksumBytes = [IO.File]::ReadAllBytes($checksumPath)
if ($checksumBytes.Contains([byte]0x0D) -or ($checksumBytes.Length -ge 3 -and $checksumBytes[0] -eq 0xEF -and $checksumBytes[1] -eq 0xBB -and $checksumBytes[2] -eq 0xBF)) {
    throw "checksum manifest must be LF-only UTF-8 without BOM"
}
foreach ($entry in $checksumEntries) {
    if ($entry -notmatch '^([0-9a-f]{64})  ([A-Za-z0-9._/-]+)$') {
        throw "generated checksum entry is invalid"
    }
    $asset = Join-Path $stage $Matches[2]
    if ((Get-Sha256 $asset) -ne $Matches[1]) {
        throw "staged asset checksum verification failed: $($Matches[2])"
    }
}

Invoke-Checked pwsh @("-NoProfile", "-File", (Join-Path $workspace "scripts/module-contracts.ps1"))
Invoke-Checked pwsh @("-NoProfile", "-File", (Join-Path $workspace "scripts/fake-magisk-smoke.ps1"))

$zipPath = Join-Path $outputRoot "NetHop-$rustCommit-arm64.zip"
$archiveInputs = Get-ChildItem -LiteralPath $stage -Force | ForEach-Object { $_.FullName }
Compress-Archive -LiteralPath $archiveInputs -DestinationPath $zipPath -CompressionLevel Optimal
$archiveListing = & tar -tf $zipPath
if ($LASTEXITCODE -ne 0 -or $archiveListing -contains ".gitkeep") {
    throw "module archive layout is invalid"
}
foreach ($required in @("module.prop", "customize.sh", "service.sh", "action.sh", "uninstall.sh", "build-manifest.json", "checksums.sha256", "bin/nethopd", "bin/nethopctl", "bin/sing-box", "rulesets/cn-domain.srs", "rulesets/cn-ip.srs", "webroot/index.html", "licenses/Unicode-3.0.txt", "licenses/country-flag-icons-MIT.txt", "licenses/webui-sbom.cdx.json", "licenses/webui-licenses.json", "licenses/webui-production-bundle.json", "licenses/webui-bundle-metafile.json")) {
    if ($archiveListing -notcontains $required) {
        throw "module archive is missing: $required"
    }
}
$summary = [ordered]@{
    module_zip = $zipPath
    module_zip_sha256 = Get-Sha256 $zipPath
    module_bytes = (Get-Item -LiteralPath $zipPath).Length
    reproducible = $manifest.reproducible
}
$summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot "package-summary.json") -Encoding utf8NoBOM
$summary | ConvertTo-Json
