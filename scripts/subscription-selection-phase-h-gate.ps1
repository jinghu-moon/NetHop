$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    cargo test --locked -p nethopd --test clash_api_contracts --test operational_control_contracts --test api_secret_contracts --test supervisor_contracts
    if ($LASTEXITCODE -ne 0) { throw "core control contracts failed" }
    $tree = cargo tree --locked -p nethopd | Out-String
    if ($LASTEXITCODE -ne 0) { throw "dependency tree failed" }
    if ($tree -match '(?i)\b(tokio|tonic|prost|grpc)\b') { throw "forbidden async/gRPC dependency detected" }
    $forbidden = rg -n 'services\.api|libbox|\.aar\b' "crates/nethopd/src" "crates/nethop-core/src" "module/defaults" 2>$null
    if ($LASTEXITCODE -eq 0) { throw "forbidden core integration path detected: $forbidden" }
    if ($LASTEXITCODE -ne 1) { throw "architecture scan failed" }
}
finally { Pop-Location }
exit 0
