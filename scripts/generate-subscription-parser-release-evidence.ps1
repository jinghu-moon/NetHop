param(
    [string]$CargoDenyPath = ""
)

$ErrorActionPreference = "Stop"

# Generates deterministic CycloneDX and provenance evidence from cargo metadata --locked.
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$OutputRoot = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m010"
$PackageName = "nethop-subscription"
$StableFeatures = "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json"

$AllowedLicenseExpressions = @(
    "(Apache-2.0 OR MIT) AND BSD-3-Clause",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
    "0BSD OR MIT OR Apache-2.0",
    "AGPL-3.0-only",
    "Apache-2.0",
    "Apache-2.0 AND ISC",
    "Apache-2.0 OR ISC OR MIT",
    "Apache-2.0 OR MIT",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
    "BSD-2-Clause OR Apache-2.0 OR MIT",
    "BSD-3-Clause",
    "CDLA-Permissive-2.0",
    "ISC",
    "MIT",
    "MIT OR Apache-2.0",
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
    "MIT OR Zlib OR Apache-2.0",
    "Unicode-3.0",
    "Unlicense OR MIT"
    "Zlib"
)

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string]$Path)
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false, $true))
    return Get-Sha256Text ($text.Replace("`r`n", "`n"))
}

function Get-Sha256Text {
    param([Parameter(Mandatory = $true)][string]$Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return [Convert]::ToHexString($sha.ComputeHash($bytes)).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $json = $Value | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

function Invoke-CargoMetadata {
    param(
        [Parameter(Mandatory = $true)][string]$Features,
        [Parameter(Mandatory = $true)][bool]$AllFeatures
    )
    $arguments = @("metadata", "--locked", "--format-version", "1", "--no-default-features")
    if ($AllFeatures) {
        $arguments += "--all-features"
    }
    else {
        $arguments += @("--features", $Features)
    }
    $json = & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata --locked failed with exit code $LASTEXITCODE"
    }
    return ($json | ConvertFrom-Json)
}

function Normalize-LicenseExpression {
    param([AllowNull()][string]$License)
    if ([string]::IsNullOrWhiteSpace($License)) {
        return "UNKNOWN"
    }
    switch ($License) {
        "MIT/Apache-2.0" { return "MIT OR Apache-2.0" }
        "Unlicense/MIT" { return "Unlicense OR MIT" }
        default { return $License }
    }
}

function Get-ReachablePackageIds {
    param(
        [Parameter(Mandatory = $true)]$Metadata,
        [Parameter(Mandatory = $true)][bool]$IncludeDev
    )
    $root = $Metadata.packages | Where-Object { $_.name -eq $PackageName -and $null -eq $_.source } | Select-Object -First 1
    if ($null -eq $root) {
        throw "workspace package $PackageName not found"
    }
    $nodes = @{}
    foreach ($node in $Metadata.resolve.nodes) {
        $nodes[$node.id] = $node
    }
    $seen = @{}
    $stack = [System.Collections.Generic.Stack[string]]::new()
    $stack.Push($root.id)
    while ($stack.Count -gt 0) {
        $id = $stack.Pop()
        if ($seen.ContainsKey($id)) {
            continue
        }
        $seen[$id] = $true
        $node = $nodes[$id]
        foreach ($dependency in $node.deps) {
            $include = $false
            foreach ($kind in $dependency.dep_kinds) {
                if ($IncludeDev -or $null -eq $kind.kind -or $kind.kind -eq "build") {
                    $include = $true
                }
            }
            if ($include -and -not $seen.ContainsKey($dependency.pkg)) {
                $stack.Push($dependency.pkg)
            }
        }
    }
    return @($seen.Keys | Sort-Object)
}

function Get-BomReference {
    param([Parameter(Mandatory = $true)]$Package)
    return "pkg:cargo/$($Package.name)@$($Package.version)"
}

function New-CycloneDxBom {
    param(
        [Parameter(Mandatory = $true)]$Metadata,
        [Parameter(Mandatory = $true)][string[]]$ReachableIds,
        [Parameter(Mandatory = $true)][string]$Profile,
        [Parameter(Mandatory = $true)][string]$SourceDigest,
        [Parameter(Mandatory = $true)][bool]$IncludeDev
    )
    $packages = @{}
    foreach ($package in $Metadata.packages) {
        $packages[$package.id] = $package
    }
    $reachable = @{}
    foreach ($id in $ReachableIds) {
        $reachable[$id] = $true
    }
    $root = $Metadata.packages | Where-Object { $_.name -eq $PackageName -and $null -eq $_.source } | Select-Object -First 1
    $components = @()
    foreach ($id in $ReachableIds) {
        if ($id -eq $root.id) {
            continue
        }
        $package = $packages[$id]
        $license = Normalize-LicenseExpression $package.license
        $component = [ordered]@{
            type = "library"
            "bom-ref" = Get-BomReference $package
            name = $package.name
            version = $package.version
            purl = Get-BomReference $package
            licenses = @([ordered]@{ expression = $license })
            properties = @(
                [ordered]@{ name = "nethop:cargo-id"; value = $package.id },
                [ordered]@{ name = "nethop:cargo-license-original"; value = [string]$package.license }
            )
        }
        if (-not [string]::IsNullOrWhiteSpace($package.checksum)) {
            $component.hashes = @([ordered]@{ alg = "SHA-256"; content = $package.checksum })
        }
        $components += $component
    }
    $components = @($components | Sort-Object name, version)

    $nodes = @{}
    foreach ($node in $Metadata.resolve.nodes) {
        $nodes[$node.id] = $node
    }
    $dependencies = @()
    foreach ($id in $ReachableIds) {
        $package = $packages[$id]
        $dependsOn = @()
        foreach ($dependency in $nodes[$id].deps) {
            $include = $false
            foreach ($kind in $dependency.dep_kinds) {
                if ($IncludeDev -or $null -eq $kind.kind -or $kind.kind -eq "build") {
                    $include = $true
                }
            }
            if ($include -and $reachable.ContainsKey($dependency.pkg)) {
                $dependsOn += Get-BomReference $packages[$dependency.pkg]
            }
        }
        $dependencies += [ordered]@{
            ref = Get-BomReference $package
            dependsOn = @($dependsOn | Sort-Object -Unique)
        }
    }
    $uuidHex = $SourceDigest.Substring(0, 32)
    $serial = "urn:uuid:$($uuidHex.Substring(0,8))-$($uuidHex.Substring(8,4))-$($uuidHex.Substring(12,4))-$($uuidHex.Substring(16,4))-$($uuidHex.Substring(20,12))"
    return [ordered]@{
        "`$schema" = "https://cyclonedx.org/schema/bom-1.6.schema.json"
        bomFormat = "CycloneDX"
        specVersion = "1.6"
        serialNumber = $serial
        version = 1
        metadata = [ordered]@{
            component = [ordered]@{
                type = "library"
                "bom-ref" = Get-BomReference $root
                name = $root.name
                version = $root.version
                purl = Get-BomReference $root
                licenses = @([ordered]@{ expression = Normalize-LicenseExpression $root.license })
            }
            properties = @([ordered]@{ name = "nethop:dependency-profile"; value = $Profile })
        }
        components = $components
        dependencies = @($dependencies | Sort-Object ref)
    }
}

Set-Location $WorkspaceRoot
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$sourcePaths = @(
    "Cargo.toml",
    "Cargo.lock",
    "crates/nethop-core/Cargo.toml",
    "crates/nethop-subscription/Cargo.toml",
    "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json",
    "deny.toml",
    "scripts/generate-subscription-parser-release-evidence.ps1"
)
$sourcePaths += Get-ChildItem -LiteralPath "crates/nethop-subscription/src" -Recurse -File -Filter "*.rs" |
    ForEach-Object { [System.IO.Path]::GetRelativePath($WorkspaceRoot, $_.FullName).Replace("\", "/") }
$sourcePaths += Get-ChildItem -LiteralPath "crates/nethop-core/src" -Recurse -File -Filter "*.rs" |
    ForEach-Object { [System.IO.Path]::GetRelativePath($WorkspaceRoot, $_.FullName).Replace("\", "/") }
$sourcePaths = @($sourcePaths | Sort-Object -Unique)
$sourceFiles = @()
$canonicalSource = ""
foreach ($relativePath in $sourcePaths) {
    $digest = Get-Sha256File (Join-Path $WorkspaceRoot $relativePath)
    $sourceFiles += [ordered]@{ path = $relativePath; sha256 = $digest }
    $canonicalSource += "$digest  $relativePath`n"
}
$sourceDigest = Get-Sha256Text $canonicalSource
Write-JsonFile ([ordered]@{ schema_version = 1; files = $sourceFiles }) (Join-Path $OutputRoot "source-files.json")

$profiles = @(
    [ordered]@{ name = "parser-only"; features = $StableFeatures; all_features = $false; include_dev = $false; forbidden = @("flate2", "ureq", "url", "criterion", "proptest", "tempfile") },
    [ordered]@{ name = "fetch"; features = "$StableFeatures,fetch"; all_features = $false; include_dev = $false; forbidden = @("criterion", "proptest", "tempfile") },
    [ordered]@{ name = "dev-test"; features = "all"; all_features = $true; include_dev = $true; forbidden = @() }
)
$profileResults = [ordered]@{}
$profileData = @{}
foreach ($profile in $profiles) {
    $metadata = Invoke-CargoMetadata $profile.features $profile.all_features
    $reachableIds = @(Get-ReachablePackageIds $metadata $profile.include_dev)
    $packagesById = @{}
    foreach ($package in $metadata.packages) { $packagesById[$package.id] = $package }
    $names = @($reachableIds | ForEach-Object { $packagesById[$_].name } | Sort-Object -Unique)
    $leaked = @($profile.forbidden | Where-Object { $names -contains $_ })
    if ($leaked.Count -gt 0) {
        throw "$($profile.name) dependency profile leaked: $($leaked -join ', ')"
    }
    $bom = New-CycloneDxBom $metadata $reachableIds $profile.name $sourceDigest $profile.include_dev
    $bomPath = Join-Path $OutputRoot "$($profile.name).cdx.json"
    Write-JsonFile $bom $bomPath
    $profileResults[$profile.name] = [ordered]@{
        features = $profile.features
        package_count = $reachableIds.Count
        feature_leakage = $false
        bom_sha256 = Get-Sha256File $bomPath
    }
    $profileData[$profile.name] = [ordered]@{ metadata = $metadata; ids = $reachableIds }
}

$licensePackages = @()
$devData = $profileData["dev-test"]
$devPackages = @{}
foreach ($package in $devData.metadata.packages) { $devPackages[$package.id] = $package }
$unknownLicenses = 0
$disallowedLicenses = @()
foreach ($id in $devData.ids) {
    $package = $devPackages[$id]
    $normalized = Normalize-LicenseExpression $package.license
    if ($normalized -eq "UNKNOWN") { $unknownLicenses++ }
    if ($AllowedLicenseExpressions -notcontains $normalized) {
        $disallowedLicenses += "$($package.name)@$($package.version): $normalized"
    }
    $licensePackages += [ordered]@{
        name = $package.name
        version = $package.version
        license = $normalized
        cargo_license_original = [string]$package.license
        source = [string]$package.source
    }
}
if ($unknownLicenses -ne 0 -or $disallowedLicenses.Count -ne 0) {
    throw "license gate failed: unknown=$unknownLicenses disallowed=$($disallowedLicenses -join '; ')"
}
Write-JsonFile ([ordered]@{
    schema_version = 1
    policy = "explicit-allowlist-v1"
    unknown_licenses = $unknownLicenses
    disallowed_licenses = @($disallowedLicenses)
    packages = @($licensePackages | Sort-Object name, version)
}) (Join-Path $OutputRoot "licenses.json")

$rustcVersion = (& rustc -Vv) -join "`n"
$cargoVersion = (& cargo -Vv) -join "`n"
$gitHead = (& git rev-parse HEAD).Trim()
$gitStatus = & git status --porcelain --untracked-files=no
$cargoDenyCommand = if (-not [string]::IsNullOrWhiteSpace($CargoDenyPath)) {
    Get-Item -LiteralPath $CargoDenyPath -ErrorAction Stop
}
else {
    Get-Command cargo-deny -ErrorAction SilentlyContinue
}
if ($null -eq $cargoDenyCommand) {
    throw "cargo-deny is required for M010; pass -CargoDenyPath or install it outside the runtime dependency graph"
}
$cargoDenyExecutable = if ($cargoDenyCommand -is [System.IO.FileInfo]) {
    $cargoDenyCommand.FullName
}
else {
    $cargoDenyCommand.Source
}
$cargoDenyOutput = @(& $cargoDenyExecutable deny check advisories bans licenses sources 2>&1)
$cargoDenyExitCode = $LASTEXITCODE
$cargoDenyOutput | ForEach-Object { Write-Host $_ }
if ($cargoDenyExitCode -ne 0) {
    throw "cargo-deny gate failed with exit code $cargoDenyExitCode"
}
$cargoDenyVersion = (& $cargoDenyExecutable deny --version) -join "`n"
$cargoDenyWarnings = @(
    $cargoDenyOutput |
        ForEach-Object { [regex]::Match([string]$_, 'warning\[([^]]+)\]') } |
        Where-Object Success |
        ForEach-Object { $_.Groups[1].Value } |
        Group-Object |
        Sort-Object Name |
        ForEach-Object { [ordered]@{ code = $_.Name; count = $_.Count } }
)
$cargoDenyReportPath = Join-Path $OutputRoot "cargo-deny-report.json"
Write-JsonFile ([ordered]@{
    schema_version = 1
    status = "passed"
    version = $cargoDenyVersion
    config_sha256 = Get-Sha256File (Join-Path $WorkspaceRoot "deny.toml")
    checks = @("advisories", "bans", "licenses", "sources")
    warnings = $cargoDenyWarnings
}) $cargoDenyReportPath
$cargoCycloneDxAvailable = $null -ne (Get-Command cargo-cyclonedx -ErrorAction SilentlyContinue)
$provenance = [ordered]@{
    schema_version = 1
    status = "passed_with_tooling_disclosure"
    generator = "scripts/generate-subscription-parser-release-evidence.ps1"
    inputs = [ordered]@{
        cargo_lock_sha256 = Get-Sha256File (Join-Path $WorkspaceRoot "Cargo.lock")
        workspace_manifest_sha256 = Get-Sha256File (Join-Path $WorkspaceRoot "Cargo.toml")
        crate_manifest_sha256 = Get-Sha256File (Join-Path $WorkspaceRoot "crates/nethop-subscription/Cargo.toml")
        mapping_manifest_sha256 = Get-Sha256File (Join-Path $WorkspaceRoot "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json")
        workspace_source_sha256 = $sourceDigest
    }
    toolchain = [ordered]@{ rustc = $rustcVersion; cargo = $cargoVersion }
    source_control = [ordered]@{
        git_head = $gitHead
        working_tree = $(if ($gitStatus.Count -eq 0) { "clean" } else { "dirty" })
    }
    gates = [ordered]@{
        cargo_metadata_locked = "passed"
        unknown_license = "passed"
        license_allowlist = "passed"
        feature_leakage = "passed"
    }
    dependency_profiles = $profileResults
    tools = [ordered]@{
        cargo_deny = [ordered]@{ status = "passed"; version = $cargoDenyVersion; report_sha256 = Get-Sha256File $cargoDenyReportPath; required_for_runtime = $false }
        cargo_cyclonedx = [ordered]@{ status = $(if ($cargoCycloneDxAvailable) { "available_not_executed" } else { "not_available" }); required_for_runtime = $false }
        repository_generator = [ordered]@{ status = "passed"; format = "CycloneDX-1.6" }
    }
}
Write-JsonFile $provenance (Join-Path $OutputRoot "provenance.json")

Write-Host "Generated M010 release evidence in $OutputRoot"
