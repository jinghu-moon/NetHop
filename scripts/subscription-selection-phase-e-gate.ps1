[CmdletBinding()]
param(
    [string]$SingBoxBinary = $env:NETHOP_SING_BOX_1_13_15
)

$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethop-core --test generation_node_registry_contracts --test selection_generation_golden_contracts
    if ($LASTEXITCODE -ne 0) { throw "generation registry/golden contracts failed" }
    cargo test --locked -p nethopd --test composer_registry_contracts --test runner_contracts
    if ($LASTEXITCODE -ne 0) { throw "composer/runner contracts failed" }

    if ([string]::IsNullOrWhiteSpace($SingBoxBinary)) {
        $SingBoxBinary = Join-Path $workspace "out/tools/sing-box-1.13.15-windows-amd64/sing-box.exe"
    }
    $SingBoxBinary = [IO.Path]::GetFullPath($SingBoxBinary)
    if (-not (Test-Path -LiteralPath $SingBoxBinary -PathType Leaf)) {
        throw "official sing-box v1.13.15 binary is required: pass -SingBoxBinary or set NETHOP_SING_BOX_1_13_15"
    }
    $version = (& $SingBoxBinary version 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0 -or $version -notmatch "sing-box version 1\.13\.15(?:\r?\n|$)") {
        throw "selection golden must be checked by sing-box v1.13.15"
    }

    $fixtureRoot = Join-Path $workspace "out/selection-check-v1.13.15"
    New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
    foreach ($mode in @("single", "merge")) {
        $fixture = Join-Path $fixtureRoot "$mode.json"
        cargo run --locked -p nethop-core --example selection_check_fixture -- $mode $fixture
        if ($LASTEXITCODE -ne 0) { throw "$mode selection fixture generation failed" }
        & $SingBoxBinary check -c $fixture
        if ($LASTEXITCODE -ne 0) { throw "sing-box v1.13.15 rejected $mode selection fixture" }
    }
}
finally { Pop-Location }
