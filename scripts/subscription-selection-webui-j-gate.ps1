$ErrorActionPreference = "Stop"

$webui = Join-Path (Split-Path -Parent $PSScriptRoot) "webui"
Push-Location $webui
try {
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { throw "WebUI typecheck failed" }
    npm run test:unit
    if ($LASTEXITCODE -ne 0) { throw "WebUI unit contracts failed" }
    npm run test:browser
    if ($LASTEXITCODE -ne 0) { throw "WebUI browser contracts failed" }
}
finally { Pop-Location }
