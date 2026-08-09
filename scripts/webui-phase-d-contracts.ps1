[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$package = Get-Content -LiteralPath (Join-Path $webui "package.json") -Raw | ConvertFrom-Json
Assert-True ($package.dependencies.kernelsu -eq "3.0.2") "KernelSU bridge must be pinned to 3.0.2"
foreach ($path in @(
    "src/bridge/host.ts", "src/bridge/kernelsu-host.ts", "src/bridge/mock-host.ts",
    "src/bridge/operations.ts", "src/bridge/command.ts", "src/bridge/json.ts",
    "src/bridge/jsonl.ts", "src/bridge/event-process.ts", "src/bridge/private-payload.ts",
    "src/bridge/package-adapter.ts"
)) {
    Assert-True (Test-Path -LiteralPath (Join-Path $webui $path) -PathType Leaf) "missing bridge file: $path"
}
$source = Get-ChildItem -LiteralPath (Join-Path $webui "src") -Recurse -File | Where-Object { $_.Extension -in @(".ts", ".vue") } | Get-Content -Raw
Assert-True (-not $source.Contains("exec(command: string)")) "raw string command API leaked"
Assert-True (-not $source.Contains("shell=true")) "shell execution option leaked"
$operations = Get-Content -LiteralPath (Join-Path $webui "src/bridge/operations.ts") -Raw
$kernelSuHost = Get-Content -LiteralPath (Join-Path $webui "src/bridge/kernelsu-host.ts") -Raw
Assert-True ($operations.Contains('"events.terminate"') -and $operations.Contains('"--session-id"')) "event process ownership command is missing"
Assert-True ($kernelSuHost.Contains('id: "events.terminate"')) "KernelSU event process cleanup is missing"
Push-Location $workspace
try {
    & cargo test -p nethopctl --test webui_v2_contracts
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }
Push-Location $webui
try {
    & npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run test:unit
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & node "scripts/check-imports.mjs"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }
Write-Host "webui phase D contracts passed"
