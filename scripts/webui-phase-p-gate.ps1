[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"

function Invoke-ExpectedFailure([string]$Program, [string[]]$Arguments) {
    $previous = $PSNativeCommandUseErrorActionPreference
    $PSNativeCommandUseErrorActionPreference = $false
    try {
        & $Program @Arguments *> $null
        $exitCode = $LASTEXITCODE
    } finally { $PSNativeCommandUseErrorActionPreference = $previous }
    if ($exitCode -eq 0) { throw "negative fixture unexpectedly passed: $Program $($Arguments -join ' ')" }
}

& (Join-Path $PSScriptRoot "webui-phase-o-gate.ps1")
Push-Location $webui
try {
    npm run build
    npm run check:imports
    npm run check:dependencies
    npm run check:security
    npm run check:bundle
    npm run report:release
    npm run test:quality
    node scripts/check-dependencies.mjs --scan-fixture tests/contracts/dependency-valid.ts.txt
    Invoke-ExpectedFailure node @("scripts/check-dependencies.mjs", "--scan-fixture", "tests/contracts/dependency-invalid-fetch.ts.txt")
    Invoke-ExpectedFailure node @("scripts/check-dependencies.mjs", "--scan-fixture", "tests/contracts/dependency-invalid-interval.ts.txt")
} finally { Pop-Location }

& (Join-Path $PSScriptRoot "scan-webui-secrets.ps1") @("module/webroot")
& (Join-Path $PSScriptRoot "webui-release-readiness-contracts.ps1")
& (Join-Path $PSScriptRoot "webui-device-evidence.ps1") | Out-Null
& (Join-Path $PSScriptRoot "webui-release-readiness.ps1") | Out-Null
Write-Output "Phase P automatic gate passed; consult artifacts/webui/release-readiness.json for device blockers"
