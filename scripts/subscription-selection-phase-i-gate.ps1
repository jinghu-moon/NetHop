$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethop-protocol -p nethopctl
    if ($LASTEXITCODE -ne 0) { throw "Protocol/CLI contracts failed" }
    cargo test --locked -p nethopd --test event_contracts --test uds_contracts --test worker_application_contracts
    if ($LASTEXITCODE -ne 0) { throw "daemon route/event contracts failed" }
}
finally { Pop-Location }
