[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethop-core --test generation_node_registry_contracts --test selection_generation_golden_contracts
    if ($LASTEXITCODE -ne 0) { throw "generation territory contracts failed" }
    cargo test --locked -p nethopd --test selection_domain_contracts --test selection_resolution_contracts
    if ($LASTEXITCODE -ne 0) { throw "daemon territory propagation failed" }
}
finally { Pop-Location }
Write-Output "territory generation gate passed"
