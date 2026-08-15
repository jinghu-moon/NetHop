[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethop-protocol
    if ($LASTEXITCODE -ne 0) { throw "protocol v4 territory contracts failed" }
    cargo test --locked -p nethopctl
    if ($LASTEXITCODE -ne 0) { throw "CLI territory contracts failed" }
}
finally { Pop-Location }
Write-Output "territory protocol gate passed"
