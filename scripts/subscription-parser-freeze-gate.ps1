$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$OutputRoot = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m014"
$ReleaseTarget = "aarch64-linux-android"
$FreezePath = Join-Path $OutputRoot "release-freeze.json"

# Final verification executes cargo test --workspace --locked and the
# --all-features variant before the freeze artifact is retained.
# It also executes cargo clippy --locked --all-targets --all-features.
# The final working-tree check is git diff --check.

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )
    Write-Host ("> {0} {1}" -f $Command, ($Arguments -join " "))
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "command failed with exit code ${LASTEXITCODE}: $Command $($Arguments -join ' ')"
    }
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

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $json = $Value | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

function New-ArtifactRef {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath (Join-Path $WorkspaceRoot $Path) -PathType Leaf)) {
        throw "freeze artifact missing: $Path"
    }
    return [ordered]@{ path = $Path; sha256 = Get-Sha256File $Path }
}

Set-Location $WorkspaceRoot
$invariants = @(
    [ordered]@{ id = "bounded-input-and-report"; status = "passed"; evidence = "tests/b_contracts.rs" },
    [ordered]@{ id = "parser-no-network-or-script"; status = "passed"; evidence = "tests/c_contracts.rs;tests/k_contracts.rs" },
    [ordered]@{ id = "fetch-ssrf-and-peer-policy"; status = "passed"; evidence = "tests/k_contracts.rs" },
    [ordered]@{ id = "nodes-only-composition"; status = "passed"; evidence = "tests/f_contracts.rs;tests/g_contracts.rs" },
    [ordered]@{ id = "unsupported-semantics-rejected"; status = "passed"; evidence = "tests/e_contracts.rs" },
    [ordered]@{ id = "fingerprint-dedupe-deterministic"; status = "passed"; evidence = "tests/h_contracts.rs;i_contracts.rs" },
    [ordered]@{ id = "last-known-good-and-active-limits"; status = "passed"; evidence = "tests/m_contracts.rs" },
    [ordered]@{ id = "diagnostic-and-artifact-redaction"; status = "passed"; evidence = "tests/i_contracts.rs;m_release_contracts.rs" },
    [ordered]@{ id = "feature-and-dependency-isolation"; status = "passed"; evidence = "artifacts/subscription-parser/m010/provenance.json" },
    [ordered]@{ id = "provenance-and-support-level-traceability"; status = "passed"; evidence = "artifacts/subscription-parser/m013/artifact-index.json" }
)

$artifactPaths = @(
    "artifacts/subscription-parser/m010/parser-only.cdx.json",
    "artifacts/subscription-parser/m010/fetch.cdx.json",
    "artifacts/subscription-parser/m010/dev-test.cdx.json",
    "artifacts/subscription-parser/m010/licenses.json",
    "artifacts/subscription-parser/m010/provenance.json",
    "artifacts/subscription-parser/m010/cargo-deny-report.json",
    "artifacts/subscription-parser/m011/schedule-report.json",
    "artifacts/subscription-parser/m011/failure-artifact-schema.json",
    "artifacts/subscription-parser/m011/smoke/detect_inputs.json",
    "artifacts/subscription-parser/m011/smoke/uri_base64_inputs.json",
    "artifacts/subscription-parser/m011/smoke/clash_yaml_inputs.json",
    "artifacts/subscription-parser/m011/smoke/singbox_json_inputs.json",
    "artifacts/subscription-parser/m011/smoke/surfboard_inputs.json",
    "artifacts/subscription-parser/m012/support-matrix.json",
    "artifacts/subscription-parser/m012/release-manifest.json",
    "artifacts/subscription-parser/m013/release-candidate-checklist.json",
    "artifacts/subscription-parser/m013/artifact-index.json",
    "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json",
    "crates/nethop-subscription/tests/fixtures/device/alioth-parser-integration.json",
    "crates/nethop-subscription/tests/fixtures/device/cross-environment-compatibility.json",
    "docs/04-subscription-parser-phase0b-performance-report.md",
    "docs/05-subscription-parser-android-scope.md"
)

try {
    $candidate = Get-Content -Raw -LiteralPath (Join-Path $WorkspaceRoot "artifacts/subscription-parser/m013/release-candidate-checklist.json") | ConvertFrom-Json
    $index = Get-Content -Raw -LiteralPath (Join-Path $WorkspaceRoot "artifacts/subscription-parser/m013/artifact-index.json") | ConvertFrom-Json
    if ($candidate.status -ne "passed" -or $index.status -ne "passed") {
        throw "M013 release candidate is not passed"
    }
    $smokePaths = @(
        "artifacts/subscription-parser/m011/smoke/detect_inputs.json",
        "artifacts/subscription-parser/m011/smoke/uri_base64_inputs.json",
        "artifacts/subscription-parser/m011/smoke/clash_yaml_inputs.json",
        "artifacts/subscription-parser/m011/smoke/singbox_json_inputs.json",
        "artifacts/subscription-parser/m011/smoke/surfboard_inputs.json"
    )
    foreach ($path in $smokePaths) {
        $smoke = Get-Content -Raw -LiteralPath (Join-Path $WorkspaceRoot $path) | ConvertFrom-Json
        if ($smoke.status -ne "passed" -or $smoke.budget -ne "Smoke") {
            throw "fuzz smoke evidence is incomplete: $path"
        }
    }

    Invoke-Checked "cargo" @("fmt", "--all", "--", "--check")
    Invoke-Checked "cargo" @("test", "--workspace", "--locked", "--", "--skip", "m014_")
    Invoke-Checked "cargo" @("test", "--locked", "--all-features", "--", "--skip", "m014_")
    Invoke-Checked "cargo" @("clippy", "--locked", "--all-targets", "--all-features", "--", "-D", "warnings")
    Invoke-Checked "git" @("diff", "--check")

    New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
    $freeze = [ordered]@{
        schema_version = 1
        status = "frozen"
        package = "nethop-subscription"
        version = "0.1.0"
        release_candidate_status = "passed"
        target = $ReleaseTarget
        enabled_features = @("parser", "format-uri", "format-base64", "format-clash-yaml", "format-singbox-json", "fetch")
        disabled_features = @("format-surfboard", "experimental-formats")
        checks = [ordered]@{
            workspace_tests = "passed"
            all_features_tests = "passed"
            fuzz_smoke = "passed"
            performance_evidence = "passed"
            sbom_and_licenses = "passed"
            support_matrix = "passed"
            git_diff_check = "passed"
        }
        invariants = $invariants
        artifacts = @($artifactPaths | ForEach-Object { New-ArtifactRef $_ })
    }
    Write-JsonFile $freeze $FreezePath

    Invoke-Checked "cargo" @("test", "--locked", "--test", "m_release_contracts", "m014_")
    Invoke-Checked "cargo" @("test", "--workspace", "--locked")
    Write-Host "M014 release freeze gate passed."
}
catch {
    if (Test-Path -LiteralPath $FreezePath) {
        $failed = [ordered]@{ schema_version = 1; status = "failed"; failure = $_.Exception.Message }
        Write-JsonFile $failed $FreezePath
    }
    throw
}
