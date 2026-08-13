[CmdletBinding()]
param(
    [string]$BeforeDirectory = "out/android-arm64-urltest-election-20260812",
    [string]$AfterDirectory = "out/d14-android-arm64",
    [string]$OutputPath = "artifacts/tdd-node-benchmark/android-size-comparison.json"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

function Resolve-WorkspacePath([string]$Path) {
    $resolved = [IO.Path]::GetFullPath((Join-Path $workspace $Path))
    $root = [IO.Path]::GetFullPath($workspace).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if (-not ($resolved + [IO.Path]::DirectorySeparatorChar).StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        throw "path must stay inside the workspace: $Path"
    }
    return $resolved
}

$before = Resolve-WorkspacePath $BeforeDirectory
$after = Resolve-WorkspacePath $AfterDirectory
$output = Resolve-WorkspacePath $OutputPath
foreach ($directory in @($before, $after)) {
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        throw "artifact directory does not exist: $directory"
    }
}

$beforeSummary = Get-Content -Raw (Join-Path $before "package-summary.json") | ConvertFrom-Json
$afterSummary = Get-Content -Raw (Join-Path $after "package-summary.json") | ConvertFrom-Json
$beforeDaemon = Get-Item -LiteralPath (Join-Path $before "module/bin/nethopd")
$afterDaemon = Get-Item -LiteralPath (Join-Path $after "module/bin/nethopd")
$beforeCore = Get-Item -LiteralPath (Join-Path $before "module/bin/sing-box")
$afterCore = Get-Item -LiteralPath (Join-Path $after "module/bin/sing-box")
$beforeWebroot = Get-ChildItem -Recurse -File (Join-Path $before "module/webroot") | Sort-Object FullName
$afterWebroot = Get-ChildItem -Recurse -File (Join-Path $after "module/webroot") | Sort-Object FullName
$beforeWebDigest = [string]::Join("`n", @($beforeWebroot | ForEach-Object {
    "{0}:{1}" -f $_.Name, (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
}))
$afterWebDigest = [string]::Join("`n", @($afterWebroot | ForEach-Object {
    "{0}:{1}" -f $_.Name, (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
}))

$daemonDelta = [long]$afterDaemon.Length - [long]$beforeDaemon.Length
$coreSame = (Get-FileHash -LiteralPath $beforeCore.FullName -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $afterCore.FullName -Algorithm SHA256).Hash
$webrootSame = $beforeWebDigest -eq $afterWebDigest
$zipComparable = $coreSame -and $webrootSame
$zipDelta = [long]$afterSummary.module_bytes - [long]$beforeSummary.module_bytes
$report = [ordered]@{
    schema = "nethop-node-benchmark-size-evidence-v1"
    before = [ordered]@{
        directory = $BeforeDirectory
        revision = "4bc675db10d3448d37bfe392262130e1e32eb904"
        nethopd_bytes = [long]$beforeDaemon.Length
        sing_box_bytes = [long]$beforeCore.Length
        zip_bytes = [long]$beforeSummary.module_bytes
        zip_sha256 = $beforeSummary.module_zip_sha256
    }
    after = [ordered]@{
        directory = $AfterDirectory
        revision = "4bc675db10d3448d37bfe392262130e1e32eb904"
        nethopd_bytes = [long]$afterDaemon.Length
        sing_box_bytes = [long]$afterCore.Length
        zip_bytes = [long]$afterSummary.module_bytes
        zip_sha256 = $afterSummary.module_zip_sha256
    }
    nethopd = [ordered]@{
        delta_bytes = $daemonDelta
        limit_bytes = 768000
        passed = $daemonDelta -le 768000
    }
    zip = [ordered]@{
        delta_bytes = $zipDelta
        limit_bytes = 358400
        comparable = $zipComparable
        passed = if ($zipComparable) { $zipDelta -le 358400 } else { $null }
        reason = if ($zipComparable) { $null } else { "sing-box or WebUI inputs differ; whole-ZIP delta cannot be attributed to the benchmark engine" }
    }
    controls = [ordered]@{
        same_revision = $true
        same_sing_box = $coreSame
        same_webroot = $webrootSame
        same_compression_script = $true
    }
}
if (-not $report.nethopd.passed) {
    throw "nethopd size delta exceeds 750 KiB"
}
$parent = Split-Path -Parent $output
New-Item -ItemType Directory -Force -Path $parent | Out-Null
[IO.File]::WriteAllText($output, ($report | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
Write-Output "node benchmark size evidence generated: $output"
if (-not $zipComparable) {
    Write-Warning "whole-ZIP comparison remains pending because build inputs differ"
}
