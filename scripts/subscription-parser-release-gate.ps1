param(
    [Parameter(Mandatory = $true)]
    [string]$CargoDenyPath
)

$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$OutputRoot = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m013"
$ReleaseTarget = "aarch64-linux-android"
$StableFeatures = "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json"
$Gates = [System.Collections.Generic.List[object]]::new()

# Release gate commands include cargo metadata --locked, cargo fmt, cargo clippy,
# cargo-deny, Android aarch64-linux-android evidence, and bounded cargo-fuzz smoke.
function Add-PassedGate {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [Parameter(Mandatory = $true)][string]$Evidence
    )
    $Gates.Add([ordered]@{ id = $Id; status = "passed"; evidence = $Evidence })
}

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

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $json = $Value | ConvertTo-Json -Depth 100
    [System.IO.File]::WriteAllText($Path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string]$RelativePath)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $WorkspaceRoot $RelativePath)).Hash.ToLowerInvariant()
}

function New-ArtifactRef {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath (Join-Path $WorkspaceRoot $Path) -PathType Leaf)) {
        throw "release artifact missing: $Path"
    }
    return [ordered]@{ path = $Path; sha256 = Get-Sha256File $Path }
}

function Write-Checklist {
    param(
        [Parameter(Mandatory = $true)][string]$Status,
        [AllowNull()][string]$Failure
    )
    New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
    $value = [ordered]@{
        schema_version = 1
        status = $Status
        target = $ReleaseTarget
        release_features = "$StableFeatures,fetch"
        gates = @($Gates)
    }
    if (-not [string]::IsNullOrWhiteSpace($Failure)) {
        $value.failure = $Failure
    }
    Write-JsonFile $value (Join-Path $OutputRoot "release-candidate-checklist.json")
}

Set-Location $WorkspaceRoot
try {
    & "scripts/generate-subscription-parser-release-evidence.ps1" -CargoDenyPath $CargoDenyPath
    Add-PassedGate "cargo-deny" "artifacts/subscription-parser/m010/cargo-deny-report.json"

    & "scripts/generate-subscription-parser-fuzz-evidence.ps1"
    & "scripts/generate-subscription-parser-support-matrix.ps1"

    Invoke-Checked "cargo" @("fmt", "--all", "--", "--check")
    Add-PassedGate "format" "cargo fmt --all -- --check"

    Invoke-Checked "cargo" @("metadata", "--locked", "--format-version", "1")
    Add-PassedGate "locked-metadata" "Cargo.lock"

    Invoke-Checked "cargo" @("test", "--workspace", "--locked", "--", "--skip", "m013_", "--skip", "m014_")
    Add-PassedGate "workspace-tests" "cargo test --workspace --locked"

    Invoke-Checked "cargo" @("test", "--locked", "--all-features", "--", "--skip", "m013_", "--skip", "m014_")
    Add-PassedGate "all-features-tests" "cargo test --locked --all-features"

    Invoke-Checked "cargo" @("test", "--locked", "--release", "--no-default-features", "--features", "$StableFeatures,fetch", "--", "--skip", "m013_", "--skip", "m014_")
    Add-PassedGate "release-tests" "release stable parser plus fetch"

    Invoke-Checked "cargo" @("clippy", "--locked", "--all-targets", "--all-features", "--", "-D", "warnings")
    Add-PassedGate "clippy" "all targets and all features"

    Invoke-Checked "cargo" @("test", "--locked", "--release", "--test", "j_contracts")
    Add-PassedGate "performance-evidence" "docs/04-subscription-parser-phase0b-performance-report.md"

    foreach ($target in @("detect_inputs", "uri_base64_inputs", "clash_yaml_inputs", "singbox_json_inputs", "surfboard_inputs")) {
        & "scripts/run-subscription-parser-fuzz.ps1" -Target $target -Budget Smoke
        if ($LASTEXITCODE -ne 0) {
            throw "fuzz target failed: $target"
        }
    }
    Add-PassedGate "fuzz-smoke" "artifacts/subscription-parser/m011/smoke"

    $androidEvidencePath = "crates/nethop-subscription/tests/fixtures/device/alioth-parser-integration.json"
    $android = Get-Content -Raw -LiteralPath $androidEvidencePath | ConvertFrom-Json
    if ($android.status -ne "reference_verified" -or $android.build.target -ne $ReleaseTarget) {
        throw "Android reference evidence is missing or targets the wrong ABI"
    }
    Add-PassedGate "android-evidence" $androidEvidencePath

    Add-PassedGate "support-matrix" "artifacts/subscription-parser/m012/support-matrix.json"

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
        "crates/nethop-subscription/manifests/sing-box-1.13.15-mapping.json",
        "crates/nethop-subscription/tests/fixtures/device/alioth-parser-integration.json",
        "crates/nethop-subscription/tests/fixtures/device/cross-environment-compatibility.json",
        "docs/04-subscription-parser-phase0b-performance-report.md",
        "docs/05-subscription-parser-android-scope.md"
    )
    $artifactIndex = [ordered]@{
        schema_version = 1
        status = "passed"
        target = $ReleaseTarget
        artifacts = @($artifactPaths | ForEach-Object { New-ArtifactRef $_ })
    }
    New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
    Write-JsonFile $artifactIndex (Join-Path $OutputRoot "artifact-index.json")
    Write-Checklist "passed" $null

    Invoke-Checked "cargo" @("test", "--locked", "--test", "m_release_contracts", "m013_")
    Write-Host "M013 release candidate gate passed."
}
catch {
    Write-Checklist "failed" $_.Exception.Message
    throw
}
