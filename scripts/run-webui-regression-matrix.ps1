[CmdletBinding()]
param(
    [string]$MatrixPath = (Join-Path (Split-Path -Parent $PSScriptRoot) "tests/webui/fixtures/legacy-capability-matrix.json")
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

function Invoke-TestCase {
    param([string]$Package, [string]$Target, [string]$Name)
    $targetPath = Join-Path $workspace "crates/$Package/tests/$Target.rs"
    if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
        throw "matrix target missing: $Package / $Target"
    }
    $source = Get-Content -LiteralPath $targetPath -Raw
    if ($source -notmatch ("(?m)^fn\s+" + [regex]::Escape($Name) + "\s*\(")) {
        throw "matrix test missing: $Package / $Target / $Name"
    }
    & cargo test -p $Package --test $Target $Name -- --exact
    if ($LASTEXITCODE -ne 0) {
        throw "matrix test failed: $Package / $Target / $Name"
    }
}

$matrix = (Get-Content -LiteralPath $MatrixPath -Raw | ConvertFrom-Json)
foreach ($item in $matrix.domains) {
    Invoke-TestCase -Package $item.positive.package -Target $item.positive.test_target -Name $item.positive.test_name
    Invoke-TestCase -Package $item.failure.package -Target $item.failure.test_target -Name $item.failure.test_name
}

Write-Host "webui regression matrix passed"
