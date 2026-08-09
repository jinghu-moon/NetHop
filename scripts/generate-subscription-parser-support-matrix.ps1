$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$OutputRoot = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m012"

function Read-Json {
    param([Parameter(Mandatory = $true)][string]$RelativePath)
    return Get-Content -Raw -LiteralPath (Join-Path $WorkspaceRoot $RelativePath) | ConvertFrom-Json
}

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string]$RelativePath)
    $path = Join-Path $WorkspaceRoot $RelativePath
    $text = [System.IO.File]::ReadAllText($path, [System.Text.UTF8Encoding]::new($false, $true)).Replace("`r`n", "`n")
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [Convert]::ToHexString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($text))).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function New-EvidenceRef {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath (Join-Path $WorkspaceRoot $Path) -PathType Leaf)) {
        throw "missing support evidence: $Path"
    }
    return [ordered]@{ path = $Path; sha256 = Get-Sha256File $Path }
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $json = $Value | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

$AndroidEvidencePath = "crates/nethop-subscription/tests/fixtures/device/alioth-parser-integration.json"
$CrossEnvironmentPath = "crates/nethop-subscription/tests/fixtures/device/cross-environment-compatibility.json"
$MappingPath = "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json"
$PerformancePath = "docs/04-subscription-parser-phase0b-performance-report.md"
$AndroidScopePath = "docs/05-subscription-parser-android-scope.md"
$SurfboardManifestPath = "crates/nethop-subscription/tests/fixtures/surfboard/manifest.json"

$android = Read-Json $AndroidEvidencePath
$crossEnvironment = Read-Json $CrossEnvironmentPath
$mapping = Read-Json $MappingPath
if ($android.status -ne "reference_verified" -or $android.device.abi -ne "arm64-v8a") {
    throw "reference Android evidence is not release-eligible"
}
if (($android.variants | Where-Object { $_.features -eq "stable-parser" }).accepted -ne 9) {
    throw "reference Android evidence does not cover all nine parser protocols"
}
if ($mapping.protocols.Count -ne 9) {
    throw "sing-box mapping manifest must contain nine protocols"
}

$stableEvidence = @(
    New-EvidenceRef $AndroidEvidencePath
    New-EvidenceRef $PerformancePath
)
$formats = @(
    [ordered]@{ format = "uri_list"; adapter = "uri"; support_level = "reference_verified"; default_enabled = $true; evidence = $stableEvidence },
    [ordered]@{ format = "base64_list"; adapter = "base64"; support_level = "reference_verified"; default_enabled = $true; evidence = $stableEvidence },
    [ordered]@{ format = "clash_yaml"; adapter = "clash_yaml"; support_level = "reference_verified"; default_enabled = $true; evidence = $stableEvidence },
    [ordered]@{ format = "singbox_json"; adapter = "singbox_json"; support_level = "reference_verified"; default_enabled = $true; evidence = @((New-EvidenceRef $AndroidEvidencePath), (New-EvidenceRef $MappingPath)) },
    [ordered]@{ format = "surfboard_ini"; adapter = "surfboard"; support_level = "experimental"; default_enabled = $false; evidence = @((New-EvidenceRef $SurfboardManifestPath), (New-EvidenceRef $AndroidScopePath)) }
)

$protocols = @()
foreach ($protocol in $mapping.protocols) {
    $formatsForProtocol = if ($protocol.protocol -in @("http", "socks")) {
        @("clash_yaml", "singbox_json")
    } else {
        @("uri_list", "base64_list", "clash_yaml", "singbox_json")
    }
    $protocols += [ordered]@{
        protocol = $protocol.protocol
        parser_support = "reference_verified"
        android_data_plane = "best_effort"
        mapping_fields = @($protocol.mapped_fields)
        capability_variants = $protocol.capabilities.Count
        sing_box_version = $mapping.sing_box_version
        formats = $formatsForProtocol
        evidence = @((New-EvidenceRef $MappingPath), (New-EvidenceRef $AndroidEvidencePath))
    }
}

$supportMatrix = [ordered]@{
    schema_version = 1
    scope = "android_subscription_parser"
    runtime_core_claims = $false
    support_levels = @("reference_verified", "community_verified", "experimental", "best_effort", "unsupported")
    formats = $formats
    protocols = @($protocols | Sort-Object protocol)
    unsupported_protocols = @(
        [ordered]@{ protocol = "wireguard"; support_level = "unsupported"; reason = "sing_box_1_13_15_endpoint_outside_terminal_outbound_contract" },
        [ordered]@{ protocol = "naive"; support_level = "unsupported"; reason = "android_sing_box_1_13_15_missing_with_naive_outbound" },
        [ordered]@{ protocol = "mieru"; support_level = "unsupported"; reason = "not_implemented_by_sing_box_1_13_15" },
        [ordered]@{ protocol = "shadowsocksr"; support_level = "unsupported"; reason = "outside_alpha_protocol_whitelist" }
    )
    features = @(
        [ordered]@{ feature = "stable-parser"; support_level = "reference_verified"; default_enabled = $true },
        [ordered]@{ feature = "fetch"; support_level = "best_effort"; default_enabled = $false },
        [ordered]@{ feature = "format-surfboard"; support_level = "experimental"; default_enabled = $false }
    )
    environments = @($crossEnvironment.environments)
    reference_device = $android.device
    generated_from = @(
        New-EvidenceRef $AndroidEvidencePath
        New-EvidenceRef $CrossEnvironmentPath
        New-EvidenceRef $MappingPath
        New-EvidenceRef $PerformancePath
        New-EvidenceRef $AndroidScopePath
        New-EvidenceRef $SurfboardManifestPath
    )
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$supportMatrixPath = Join-Path $OutputRoot "support-matrix.json"
Write-JsonFile $supportMatrix $supportMatrixPath

$metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata --locked failed"
}
$package = $metadata.packages | Where-Object name -eq "nethop-subscription" | Select-Object -First 1
$artifactPaths = @(
    "artifacts/subscription-parser/m010/parser-only.cdx.json",
    "artifacts/subscription-parser/m010/fetch.cdx.json",
    "artifacts/subscription-parser/m010/dev-test.cdx.json",
    "artifacts/subscription-parser/m010/licenses.json",
    "artifacts/subscription-parser/m010/provenance.json",
    "artifacts/subscription-parser/m010/cargo-deny-report.json",
    "artifacts/subscription-parser/m011/schedule-report.json",
    "artifacts/subscription-parser/m011/failure-artifact-schema.json",
    "artifacts/subscription-parser/m012/support-matrix.json",
    $MappingPath,
    $PerformancePath,
    $AndroidScopePath
)
$artifactIndex = @($artifactPaths | ForEach-Object { New-EvidenceRef $_ })
$releaseManifest = [ordered]@{
    schema_version = 1
    package = $package.name
    version = $package.version
    status = "release_candidate"
    target = "aarch64-linux-android"
    minimum_android_api = 23
    features = [ordered]@{
        enabled = @("parser", "format-uri", "format-base64", "format-clash-yaml", "format-singbox-json", "fetch")
        disabled = @("format-surfboard", "experimental-formats")
    }
    schemas = [ordered]@{
        parser_ipc = $crossEnvironment.parser_ipc_schema_version
        fingerprint = $crossEnvironment.fingerprint_schema
    }
    limits = $crossEnvironment.limits
    sing_box = [ordered]@{
        version = $mapping.sing_box_version
        commit = $mapping.sing_box_commit
        mapping_digest = $crossEnvironment.mapping_digest
    }
    support_matrix_sha256 = Get-Sha256File "artifacts/subscription-parser/m012/support-matrix.json"
    artifacts = $artifactIndex
}
Write-JsonFile $releaseManifest (Join-Path $OutputRoot "release-manifest.json")

Write-Host "Generated M012 support matrix and release manifest in $OutputRoot"
