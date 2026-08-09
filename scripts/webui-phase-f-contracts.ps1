[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"
function Assert-True { param([bool]$Condition, [string]$Message); if (-not $Condition) { throw $Message } }
foreach ($path in @("src/runtime/event-state.ts", "src/runtime/event-session.ts", "src/runtime/reconnect.ts", "src/runtime/traffic-ring.ts", "src/runtime/use-event-lifecycle.ts", "tests/unit/event-runtime.test.ts")) {
    Assert-True (Test-Path -LiteralPath (Join-Path $webui $path) -PathType Leaf) "missing runtime file: $path"
}
$state = Get-Content -LiteralPath (Join-Path $webui "src/runtime/event-state.ts") -Raw
$ring = Get-Content -LiteralPath (Join-Path $webui "src/runtime/traffic-ring.ts") -Raw
$operations = Get-Content -LiteralPath (Join-Path $webui "src/bridge/operations.ts") -Raw
Assert-True ($state.Contains("awaiting_snapshot") -and $state.Contains("resync_required")) "snapshot/resync state machine is incomplete"
Assert-True ($ring.Contains("capacity") -and $ring.Contains("Math.min(this.length + 1")) "traffic ring is not bounded"
Assert-True ($operations.Contains("EVENT_SESSION_MAX_RUNTIME_SECONDS = 300") -and $operations.Contains('"--max-runtime-seconds"')) "event process lifetime is not bounded"
Push-Location $webui
try {
    & npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run test:unit
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run test:browser
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run test:e2e
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }
Write-Host "webui phase F contracts passed"
