[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

Assert-True (Test-Path -LiteralPath (Join-Path $workspace "tests/webui/fixtures/cli-v1-before.json") -PathType Leaf) "missing cli fixture"
Assert-True (Test-Path -LiteralPath (Join-Path $workspace "tests/webui/fixtures/protocol-v1-before.json") -PathType Leaf) "missing protocol fixture"
Assert-True (Test-Path -LiteralPath (Join-Path $workspace "tests/webui/fixtures/module-no-webui-before.json") -PathType Leaf) "missing module fixture"
Assert-True (Test-Path -LiteralPath (Join-Path $workspace "tests/webui/fixtures/secret-canaries.json") -PathType Leaf) "missing secret canary fixture"
Assert-True (Test-Path -LiteralPath (Join-Path $workspace "tests/webui/fixtures/destructive-change-valid.json") -PathType Leaf) "missing change evidence fixture"

$moduleFixture = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/module-no-webui-before.json") -Raw | ConvertFrom-Json
Assert-True (-not [bool]$moduleFixture.webroot_present) "module before fixture must freeze webroot_present=false"
foreach ($relativePath in $moduleFixture.required_files) {
    Assert-True (Test-Path -LiteralPath (Join-Path $workspace "module/$relativePath") -PathType Leaf) "module before file missing: $relativePath"
}

& pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") @("tests/webui/fixtures")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& pwsh -NoProfile -File (Join-Path $workspace "scripts/validate-webui-change-evidence.ps1") @("tests/webui/fixtures/destructive-change-valid.json")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# The scanner and evidence validator must prove both failure and success paths.
$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("nethop-webui-phase-a-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
try {
    $canary = ((Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/secret-canaries.json") -Raw | ConvertFrom-Json).canaries)[0]
    $probe = Join-Path $temporaryDirectory "canary.txt"
    Set-Content -LiteralPath $probe -Value $canary -NoNewline
    & pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") @($probe) 2>$null
    Assert-True ($LASTEXITCODE -ne 0) "secret scanner accepted an injected canary"
    Set-Content -LiteralPath $probe -Value "safe fixture" -NoNewline
    & pwsh -NoProfile -File (Join-Path $workspace "scripts/scan-webui-secrets.ps1") @($probe)
    Assert-True ($LASTEXITCODE -eq 0) "secret scanner rejected a clean fixture"
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}

& pwsh -NoProfile -File (Join-Path $workspace "scripts/validate-webui-change-evidence.ps1") @("tests/webui/fixtures/destructive-change-invalid.json") 2>$null
Assert-True ($LASTEXITCODE -ne 0) "change evidence validator accepted an invalid manifest"

& pwsh -NoProfile -File (Join-Path $workspace "scripts/test-webui.ps1") -Suite Frontend -FailForTest 2>$null
Assert-True ($LASTEXITCODE -ne 0) "unified WebUI test entry accepted a failed suite"

$adr = Get-Content -LiteralPath (Join-Path $workspace "docs/adr/WEBUI-TEMPLATE.md") -Raw
foreach ($heading in @("状态", "问题", "数据与测量", "选择", "删除的旧路径", "before evidence", "RED evidence", "after evidence", "regression evidence", "回滚条件", "复测命令")) {
    Assert-True ($adr.Contains($heading)) "ADR template is missing: $heading"
}

Write-Host "webui phase A contracts passed"
