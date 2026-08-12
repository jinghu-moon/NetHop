$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethopd --test selection_domain_contracts --test selection_resolution_contracts --test operational_control_contracts
    if ($LASTEXITCODE -ne 0) { throw "selection domain contracts failed" }
}
finally { Pop-Location }
