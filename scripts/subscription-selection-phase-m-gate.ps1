$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw "workspace integration failed" }
    cargo run --locked --release -p nethopd --example fair_pool_bench
    if ($LASTEXITCODE -ne 0) { throw "backend 10,000-node benchmark failed" }
    & (Join-Path $PSScriptRoot "subscription-selection-phase-h-gate.ps1")
    if ($LASTEXITCODE -ne 0) { throw "architecture gate failed" }
    Push-Location (Join-Path $workspace "webui")
    try {
        npm run gate
        if ($LASTEXITCODE -ne 0) { throw "WebUI release gate failed" }
    }
    finally { Pop-Location }
}
finally { Pop-Location }
