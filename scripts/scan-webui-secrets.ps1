[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string[]]$Path = @(".")
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$canaryPath = Join-Path $workspace "tests/webui/fixtures/secret-canaries.json"
$canaryFullPath = (Resolve-Path -LiteralPath $canaryPath).Path
$canaries = (Get-Content -LiteralPath $canaryPath -Raw | ConvertFrom-Json).canaries

$scanRoots = New-Object System.Collections.Generic.List[string]
foreach ($item in $Path) {
    $resolved = if ([System.IO.Path]::IsPathRooted($item)) { $item } else { Join-Path $workspace $item }
    $scanRoots.Add((Resolve-Path -LiteralPath $resolved).Path)
}

$hits = New-Object System.Collections.Generic.List[string]
foreach ($root in $scanRoots) {
    if (Test-Path -LiteralPath $root -PathType Leaf) {
        [string]$content = Get-Content -LiteralPath $root -Raw
        foreach ($canary in $canaries) {
            if ($null -ne $content -and $content.Contains([string]$canary)) {
                $hits.Add("$root :: $canary")
            }
        }
        continue
    }
    $files = Get-ChildItem -LiteralPath $root -File -Recurse | Where-Object {
        $_.FullName -ne $canaryFullPath
    }
    foreach ($file in $files) {
        [string]$content = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($canary in $canaries) {
            if ($null -ne $content -and $content.Contains([string]$canary)) {
                $hits.Add("$($file.FullName) :: $canary")
            }
        }
    }
}

if ($hits.Count -gt 0) {
    $hits | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host "secret canary scan passed"
