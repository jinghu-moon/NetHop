$ErrorActionPreference = "Stop"

$webui = Join-Path (Split-Path -Parent $PSScriptRoot) "webui"
Push-Location $webui
try {
    npx playwright test tests/e2e/foundation.spec.ts -g "node page|overview presents"
    if ($LASTEXITCODE -ne 0) { throw "node/overview interaction E2E failed" }
    npx playwright test tests/e2e/release-quality.spec.ts -g "visual baseline|icon and text"
    if ($LASTEXITCODE -ne 0) { throw "node/overview visual E2E failed" }
}
finally { Pop-Location }
