[CmdletBinding()]
param(
    [string]$EvidenceDirectory = "artifacts/tdd-node-benchmark"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$root = [IO.Path]::GetFullPath((Join-Path $workspace $EvidenceDirectory))
$workspaceRoot = [IO.Path]::GetFullPath($workspace).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not ($root + [IO.Path]::DirectorySeparatorChar).StartsWith($workspaceRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "evidence directory must stay inside the workspace"
}

$reportPath = Join-Path $root "host-release/report.json"
$postprocessPath = Join-Path $root "host-release/postprocess.json"
$manifestPath = Join-Path $root "host-release/manifest.json"
$sizePath = Join-Path $root "android-size-comparison.json"
foreach ($path in @($reportPath, $postprocessPath, $manifestPath, $sizePath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required evidence is missing: $path"
    }
    if ((Get-Item -LiteralPath $path).Length -gt 512KB) {
        throw "evidence file exceeds its bounded size: $path"
    }
}

$report = Get-Content -Raw $reportPath | ConvertFrom-Json -Depth 100
$postprocess = Get-Content -Raw $postprocessPath | ConvertFrom-Json -Depth 100
$manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json -Depth 100
$size = Get-Content -Raw $sizePath | ConvertFrom-Json -Depth 100
if ($report.schema -ne "nethop-node-benchmark-host-release-v1" -or
    $postprocess.schema -ne "nethop-node-benchmark-postprocess-v1" -or
    $manifest.schema -ne "nethop-tdd-evidence-v1" -or
    $size.schema -ne "nethop-node-benchmark-size-evidence-v1") {
    throw "evidence schema is unsupported"
}
if (-not $report.passed -or -not $postprocess.passed -or -not $size.nethopd.passed) {
    throw "a completed host evidence gate is not passing"
}
if ($size.zip.comparable -and -not $size.zip.passed) {
    throw "comparable whole-ZIP evidence exceeds the budget"
}
if ((Get-FileHash -LiteralPath $reportPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.report_sha256 -or
    (Get-FileHash -LiteralPath $postprocessPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.postprocess_sha256) {
    throw "evidence digest does not match its manifest"
}
$requiredTasks = @("E009", "E010", "F011", "L004", "L005", "L008")
foreach ($task in $requiredTasks) {
    if (@($manifest.task_ids) -notcontains $task) {
        throw "evidence task is missing: $task"
    }
}
$serializedParts = foreach ($path in @($reportPath, $postprocessPath, $manifestPath, $sizePath)) {
    Get-Content -Raw -LiteralPath $path
}
$serialized = [string]::Join("`n", $serializedParts)
foreach ($forbidden in @("glados", "baac5688", "f936155", "121525", "token=", "authorization", "api_secret", "private_key")) {
    if ($serialized.Contains($forbidden, [StringComparison]::OrdinalIgnoreCase)) {
        throw "evidence contains forbidden sensitive material: $forbidden"
    }
}

$hostPanic = (& rustc --print cfg | Select-String '^panic="([^"]+)"$').Matches.Groups[1].Value
$androidPanic = (& rustc --print cfg --target aarch64-linux-android | Select-String '^panic="([^"]+)"$').Matches.Groups[1].Value
if ($hostPanic -ne "unwind" -or $androidPanic -ne "unwind") {
    throw "catch_unwind evidence requires panic=unwind for host and Android release targets"
}
Write-Output "node benchmark evidence validation passed"
Write-Output "panic strategy: host=$hostPanic android=$androidPanic"
