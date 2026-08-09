[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"
$webroot = Join-Path $workspace "module/webroot"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

foreach ($path in @("package.json", "package-lock.json", "src/main.ts", "vite.config.ts", "vitest.unit.config.ts", "vitest.browser.config.ts", "playwright.config.ts")) {
    Assert-True (Test-Path -LiteralPath (Join-Path $webui $path) -PathType Leaf) "missing WebUI workspace file: $path"
}
$package = Get-Content -LiteralPath (Join-Path $webui "package.json") -Raw | ConvertFrom-Json
foreach ($forbidden in @("pinia", "axios", "vue-virtual-scroller", "unplugin-vue-components")) {
    Assert-True (-not $package.dependencies.$forbidden -and -not $package.devDependencies.$forbidden) "forbidden WebUI dependency: $forbidden"
}
$vite = Get-Content -LiteralPath (Join-Path $webui "vite.config.ts") -Raw
Assert-True ($vite.Contains('base: "./"')) "Vite base must be relative"
Assert-True ($vite.Contains('target: "chrome105"')) "Vite target must be chrome105"
Assert-True ($vite.Contains('sourcemap: false')) "production source maps must be disabled"
$router = Get-Content -LiteralPath (Join-Path $webui "src/router.ts") -Raw
Assert-True ($router.Contains("createWebHashHistory")) "WebUI router must use hash history"
$moduleBuild = Get-Content -LiteralPath (Join-Path $workspace "scripts/build-android-module.ps1") -Raw
foreach ($contract in @('npm @("ci", "--ignore-scripts")', '"check:bundle"', '"check:security"', 'webui = [ordered]@{', '"webroot/index.html"')) {
    Assert-True ($moduleBuild.Contains($contract)) "Android module build omits WebUI contract: $contract"
}

Assert-True (Test-Path -LiteralPath (Join-Path $webroot "index.html") -PathType Leaf) "production webroot is missing"
$productionIndex = Get-Content -LiteralPath (Join-Path $webroot "index.html") -Raw
Assert-True ($productionIndex.Contains("Content-Security-Policy")) "production CSP is missing"
Assert-True (-not $productionIndex.Contains("http://") -and -not $productionIndex.Contains("https://")) "production index contains a remote URL"

Push-Location $webui
try {
    & node "scripts/check-imports.mjs" "tests/contracts/import-valid.ts.txt"
    Assert-True ($LASTEXITCODE -eq 0) "import guard rejected the valid fixture"
    & node "scripts/check-imports.mjs" "tests/contracts/import-invalid-tdesign.ts.txt" 2>$null
    Assert-True ($LASTEXITCODE -ne 0) "import guard accepted global TDesign registration"
    & node "scripts/check-imports.mjs" "tests/contracts/import-invalid-tabler.ts.txt" 2>$null
    Assert-True ($LASTEXITCODE -ne 0) "import guard accepted a Tabler namespace import"
    & node "scripts/check-production-security.mjs" "--scan-fixture" "tests/contracts/security-valid.txt"
    Assert-True ($LASTEXITCODE -eq 0) "security guard rejected the valid fixture"
    & node "scripts/check-production-security.mjs" "--scan-fixture" "tests/contracts/security-invalid.txt" 2>$null
    Assert-True ($LASTEXITCODE -ne 0) "security guard accepted a remote connect-src"
    & node "scripts/check-bundle-budget.mjs" "--probe-gzip" "81920"
    Assert-True ($LASTEXITCODE -eq 0) "bundle guard rejected the exact limit"
    & node "scripts/check-bundle-budget.mjs" "--probe-gzip" "81921" 2>$null
    Assert-True ($LASTEXITCODE -ne 0) "bundle guard accepted an over-limit chunk"
}
finally {
    Pop-Location
}

Write-Host "webui phase C contracts passed"
