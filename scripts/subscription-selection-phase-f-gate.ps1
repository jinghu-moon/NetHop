$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethopd --test subscription_transaction_contracts --test config_reconciler_contracts --test source_update_contracts --test worker_application_contracts
    if ($LASTEXITCODE -ne 0) { throw "transaction/recovery contracts failed" }
}
finally { Pop-Location }
