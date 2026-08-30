[CmdletBinding()]
param(
    [int]$Samples = 20,
    [switch]$SkipBackend,
    [switch]$IncludeCaptureToggle,
    [int]$ReadOnlyP95BudgetMs = 500,
    [int]$CaptureP95BudgetMs = 2000
)

$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path (Join-Path $PSScriptRoot "..\")).Path
$backendReport = $null

if (-not $SkipBackend) {
    $backendArgs = @(
        "-NoProfile", "-File", (Join-Path $workspace "scripts\cli-backend-performance.ps1"),
        "-Samples", $Samples,
        "-ReadOnlyP95BudgetMs", $ReadOnlyP95BudgetMs,
        "-CaptureP95BudgetMs", $CaptureP95BudgetMs
    )
    if ($IncludeCaptureToggle) { $backendArgs += "-IncludeCaptureToggle" }
    & pwsh @backendArgs
    if ($LASTEXITCODE -ne 0) { throw "backend performance test failed with exit code $LASTEXITCODE" }
    $backendRoot = Join-Path $workspace "artifacts\cli-performance\android"
    $backendReport = Get-ChildItem -LiteralPath $backendRoot -Filter "cli-backend-*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($null -eq $backendReport) { throw "backend performance report was not generated" }
}

$env:NETHOP_FRONTEND_SAMPLES = [string]$Samples
Push-Location (Join-Path $workspace "webui")
try {
    & npm run test:e2e -- tests/e2e/frontend-performance.spec.ts
    if ($LASTEXITCODE -ne 0) { throw "frontend performance test failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
    Remove-Item Env:NETHOP_FRONTEND_SAMPLES -ErrorAction SilentlyContinue
}

$frontendRoot = Join-Path $workspace "artifacts\cli-performance\webui"
$frontendReport = Get-ChildItem -LiteralPath $frontendRoot -Filter "frontend-*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($null -eq $frontendReport) { throw "frontend performance report was not generated" }

$combinedRoot = Join-Path $workspace "artifacts\cli-performance"
$combined = [ordered]@{
    schema = "nethop-cli-frontend-performance-v1"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    backend = if ($null -eq $backendReport) { $null } else { Get-Content -Raw -LiteralPath $backendReport.FullName | ConvertFrom-Json }
    frontend = Get-Content -Raw -LiteralPath $frontendReport.FullName | ConvertFrom-Json
    passed = ((Get-Content -Raw -LiteralPath $frontendReport.FullName | ConvertFrom-Json).passed -and ($null -eq $backendReport -or (Get-Content -Raw -LiteralPath $backendReport.FullName | ConvertFrom-Json).passed))
}
$combinedPath = Join-Path $combinedRoot ("cli-frontend-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".json")
$combined | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $combinedPath -Encoding utf8NoBOM
Write-Output "frontend report: $($frontendReport.FullName)"
Write-Output "combined report: $combinedPath"
Write-Output "passed: $($combined.passed)"
if (-not $combined.passed) { exit 1 }
