$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethopd --test selection_refactor_baseline_contracts
    if ($LASTEXITCODE -ne 0) {
        throw "selection refactor baseline contracts failed"
    }

    cargo test --workspace --locked
    if ($LASTEXITCODE -ne 0) {
        throw "workspace baseline failed"
    }
}
finally {
    Pop-Location
}
