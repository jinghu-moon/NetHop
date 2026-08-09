[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"
function Assert-True { param([bool]$Condition, [string]$Message); if (-not $Condition) { throw $Message } }
foreach ($path in @("src/model/bounds.ts", "src/model/dto.ts", "src/model/client.ts", "tests/unit/dto.test.ts")) {
    Assert-True (Test-Path -LiteralPath (Join-Path $webui $path) -PathType Leaf) "missing DTO file: $path"
}
$source = (Get-ChildItem -LiteralPath (Join-Path $webui "src/model") -Recurse -File -Filter *.ts | Get-Content -Raw) -join "`n"
Assert-True (-not $source.Contains(" as any")) "DTO model contains an any escape hatch"
Assert-True ($source.Contains("unknown field")) "unknown field rejection is missing"
Assert-True ($source.Contains("prototype-shaped key")) "prototype-shaped key guard is missing"
Push-Location $webui
try {
    & npm run typecheck
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & npm run test:unit
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally { Pop-Location }
Write-Host "webui phase E contracts passed"
