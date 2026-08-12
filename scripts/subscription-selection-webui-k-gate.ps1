$ErrorActionPreference = "Stop"

$webui = Join-Path (Split-Path -Parent $PSScriptRoot) "webui"
Push-Location $webui
try {
    npx playwright test tests/e2e/foundation.spec.ts -g "subscription"
    if ($LASTEXITCODE -ne 0) { throw "subscription interaction E2E failed" }
    npx playwright test tests/e2e/release-quality.spec.ts -g "subscription cards"
    if ($LASTEXITCODE -ne 0) { throw "subscription visual E2E failed" }
}
finally { Pop-Location }
