[CmdletBinding()]
param(
    [ValidateSet("PhaseA", "Rust", "Frontend", "All")]
    [string]$Suite = "All",
    [switch]$FailForTest
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$hasWebUi = Test-Path -LiteralPath (Join-Path $workspace "webui/package.json") -PathType Leaf

if ($FailForTest) {
    Write-Error "intentional WebUI test entry failure"
    exit 1
}

function Invoke-Script([string]$RelativePath, [string[]]$Args = @()) {
    & pwsh -NoProfile -File (Join-Path $workspace $RelativePath) @Args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

function Invoke-Frontend {
    if (-not $hasWebUi) {
        Write-Host "webui workspace not initialized yet"
        return
    }
    $package = Get-Content -LiteralPath (Join-Path $workspace "webui/package.json") -Raw | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace($package.scripts.test) -or [string]::IsNullOrWhiteSpace($package.scripts.typecheck)) {
        throw "webui package must define test and typecheck scripts"
    }
    Push-Location (Join-Path $workspace "webui")
    try {
        & npm test
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        & npm run typecheck
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
}

switch ($Suite) {
    "PhaseA" {
        Invoke-Script "scripts/webui-phase-a-contracts.ps1"
        Invoke-Script "scripts/run-webui-regression-matrix.ps1"
    }
    "Rust" {
        & cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    "Frontend" {
        Invoke-Frontend
    }
    default {
        Invoke-Script "scripts/webui-phase-a-contracts.ps1"
        Invoke-Script "scripts/run-webui-regression-matrix.ps1"
        & cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Invoke-Frontend
    }
}
