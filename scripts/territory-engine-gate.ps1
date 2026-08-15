[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethop-subscription --test territory_contracts
    if ($LASTEXITCODE -ne 0) { throw "territory engine contracts failed" }
    cargo test --locked -p nethop-subscription --test filter_contracts --test core_adapter_contracts
    if ($LASTEXITCODE -ne 0) { throw "subscription pipeline regression failed" }
    $report = cargo run --quiet --locked --release -p nethop-subscription --example territory_benchmark | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $report.schema -ne "nethop-territory-benchmark-v1" -or
        -not $report.passed -or $report.name_count -ne 2000 -or $report.max_name_bytes -gt 128 -or
        $report.p95_us -gt 5000) {
        throw "territory release performance budget failed"
    }
    Write-Output "territory benchmark passed: p50_us=$($report.p50_us) p95_us=$($report.p95_us)"
}
finally { Pop-Location }
Write-Output "territory engine gate passed"
