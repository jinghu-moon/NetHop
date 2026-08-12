$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethopd --test candidate_pool_contracts
    if ($LASTEXITCODE -ne 0) {
        throw "candidate pool contracts failed"
    }

    cargo run --locked --release -p nethopd --example fair_pool_bench
    if ($LASTEXITCODE -ne 0) {
        throw "fair pool release benchmark failed"
    }
}
finally {
    Pop-Location
}
