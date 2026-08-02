$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$OutputRoot = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m011"
$FuzzRoot = Join-Path $WorkspaceRoot "crates/nethop-subscription/fuzz"
$Targets = @("detect_inputs", "uri_base64_inputs", "clash_yaml_inputs", "singbox_json_inputs", "surfboard_inputs")

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-DirectoryDigest {
    param([Parameter(Mandatory = $true)][string]$Path)
    $canonical = ""
    $files = @(Get-ChildItem -LiteralPath $Path -Recurse -File | Sort-Object FullName)
    foreach ($file in $files) {
        $relative = [System.IO.Path]::GetRelativePath($Path, $file.FullName).Replace("\", "/")
        $canonical += "$(Get-Sha256File $file.FullName)  $relative`n"
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($canonical)
        return [ordered]@{
            digest = [Convert]::ToHexString($sha.ComputeHash($bytes)).ToLowerInvariant()
            count = $files.Count
        }
    }
    finally {
        $sha.Dispose()
    }
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$corpora = @()
foreach ($target in $Targets) {
    $relative = "crates/nethop-subscription/fuzz/seeds/$target"
    $result = Get-DirectoryDigest (Join-Path $WorkspaceRoot $relative)
    if ($result.count -eq 0) {
        throw "empty fuzz corpus: $target"
    }
    $corpora += [ordered]@{
        target = $target
        path = $relative
        files = $result.count
        sha256 = $result.digest
    }
}

$schema = [ordered]@{
    schema_version = 1
    type = "object"
    required = @("target", "exit_code", "corpus_sha256", "artifact_sha256", "max_total_time_seconds", "rss_limit_mb", "toolchain")
    additional_properties = $false
}
$report = [ordered]@{
    schema_version = 1
    status = "dry_run_passed"
    runner = [ordered]@{
        pr_smoke_seconds = 60
        nightly_seconds_per_target = 1800
        release_candidate_seconds_per_target = 3600
        rss_limit_mb = 512
        parser_release_rss_budget_mb = 110
    }
    corpora = $corpora
    failure_artifact_schema = "artifacts/subscription-parser/m011/failure-artifact-schema.json"
}
foreach ($item in @(
    [ordered]@{ value = $schema; path = Join-Path $OutputRoot "failure-artifact-schema.json" },
    [ordered]@{ value = $report; path = Join-Path $OutputRoot "schedule-report.json" }
)) {
    $json = $item.value | ConvertTo-Json -Depth 20
    [System.IO.File]::WriteAllText($item.path, "$json`n", [System.Text.UTF8Encoding]::new($false))
}

Write-Host "Generated M011 fuzz evidence in $OutputRoot"
