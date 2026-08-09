[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$ManifestPath = (Join-Path (Split-Path -Parent $PSScriptRoot) "tests/webui/fixtures/destructive-change-valid.json")
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$manifest = (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json)

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-RelativeWorkspacePath {
    param([string]$RelativePath)
    Assert-True (-not [System.IO.Path]::IsPathRooted($RelativePath)) "path must be relative: $RelativePath"
    Assert-True (-not $RelativePath.Contains("..")) "path traversal is forbidden: $RelativePath"
    $resolved = Join-Path $workspace $RelativePath
    Assert-True (Test-Path -LiteralPath $resolved) "evidence path missing: $RelativePath"
}

Assert-True ($manifest.schema_version -eq 1) "schema_version must be 1"
Assert-True (-not [string]::IsNullOrWhiteSpace($manifest.change_id)) "change_id is required"
Assert-True ([bool]$manifest.breaking) "breaking must be true for phase A destructive changes"

foreach ($field in @("before", "new_red", "after", "regression")) {
    $entry = $manifest.$field
    Assert-True ($null -ne $entry) "$field evidence is required"
    Assert-True (-not [string]::IsNullOrWhiteSpace($entry.test_id)) "$field.test_id is required"
    Assert-True (-not [string]::IsNullOrWhiteSpace($entry.path)) "$field.path is required"
    Assert-RelativeWorkspacePath $entry.path
}

Write-Host "webui change evidence passed"
