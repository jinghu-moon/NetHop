param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("detect_inputs", "uri_base64_inputs", "clash_yaml_inputs", "singbox_json_inputs", "surfboard_inputs")]
    [string]$Target,

    [ValidateSet("Smoke", "Nightly", "ReleaseCandidate")]
    [string]$Budget = "Smoke",

    [string]$FailureRoot = "artifacts/subscription-parser/m011/failures"
)

$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$CrateRoot = Join-Path $WorkspaceRoot "crates/nethop-subscription"
$FuzzRoot = Join-Path $CrateRoot "fuzz"
$SeedRoot = Join-Path $FuzzRoot "seeds/$Target"
$CorpusRoot = Join-Path $FuzzRoot "target/fuzz-corpus/$Target"
$FailurePath = Join-Path $WorkspaceRoot "$FailureRoot/$Target"
$SuccessPath = Join-Path $WorkspaceRoot "artifacts/subscription-parser/m011/smoke/$Target.json"
$RssLimitMb = 512
$MaxTotalTime = switch ($Budget) {
    "Smoke" { 60 }
    "Nightly" { 1800 }
    "ReleaseCandidate" { 3600 }
}

if ($IsWindows) {
    $clang = Get-Command clang -ErrorAction SilentlyContinue
    if ($null -ne $clang) {
        $llvmRoot = Split-Path -Parent (Split-Path -Parent $clang.Source)
        $runtimeRoot = Join-Path $llvmRoot "lib/clang"
        $runtime = Get-ChildItem -LiteralPath $runtimeRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            ForEach-Object { Join-Path $_.FullName "lib/windows" } |
            Where-Object { Test-Path -LiteralPath (Join-Path $_ "clang_rt.asan_dynamic_runtime_thunk-x86_64.lib") } |
            Select-Object -First 1
        if ($null -ne $runtime) {
            $env:LIB = "$runtime;$env:LIB"
            $env:PATH = "$runtime;$env:PATH"
        }
    }
}

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-DirectoryDigest {
    param([Parameter(Mandatory = $true)][string]$Path)
    $canonical = ""
    foreach ($file in Get-ChildItem -LiteralPath $Path -Recurse -File | Sort-Object FullName) {
        $relative = [System.IO.Path]::GetRelativePath($Path, $file.FullName).Replace("\", "/")
        $canonical += "$(Get-Sha256File $file.FullName)  $relative`n"
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($canonical)
        return [Convert]::ToHexString($sha.ComputeHash($bytes)).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

New-Item -ItemType Directory -Force -Path $FailurePath,$CorpusRoot | Out-Null
Get-ChildItem -LiteralPath $SeedRoot -File | ForEach-Object {
    $destination = Join-Path $CorpusRoot $_.Name
    if (-not (Test-Path -LiteralPath $destination)) {
        Copy-Item -LiteralPath $_.FullName -Destination $destination
    }
}
$artifactPrefix = "$($FailurePath.Replace('\', '/'))/"
Push-Location $CrateRoot
try {
    & cargo +nightly fuzz run $Target $CorpusRoot -- "-max_total_time=$MaxTotalTime" "-rss_limit_mb=$RssLimitMb" "-artifact_prefix=$artifactPrefix"
    $exitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}

if ($exitCode -ne 0) {
    $artifact = Get-ChildItem -LiteralPath $FailurePath -File |
        Where-Object { $_.Name -ne "failure.json" } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    $failure = [ordered]@{
        schema_version = 1
        target = $Target
        exit_code = $exitCode
        corpus_sha256 = Get-DirectoryDigest $SeedRoot
        artifact_sha256 = $(if ($null -eq $artifact) { $null } else { Get-Sha256File $artifact.FullName })
        max_total_time_seconds = $MaxTotalTime
        rss_limit_mb = $RssLimitMb
        toolchain = ((& rustc +nightly -Vv) -join "`n")
    }
    $json = $failure | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText(
        (Join-Path $FailurePath "failure.json"),
        "$json`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    exit $exitCode
}

$success = [ordered]@{
    schema_version = 1
    status = "passed"
    target = $Target
    budget = $Budget
    seed_corpus_sha256 = Get-DirectoryDigest $SeedRoot
    max_total_time_seconds = $MaxTotalTime
    rss_limit_mb = $RssLimitMb
    sanitizer = "address"
    toolchain = ((& rustc +nightly -Vv) -join "`n")
}
$successJson = $success | ConvertTo-Json -Depth 10
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $SuccessPath) | Out-Null
[System.IO.File]::WriteAllText(
    $SuccessPath,
    "$successJson`n",
    [System.Text.UTF8Encoding]::new($false)
)
Write-Host "Fuzz target $Target passed the $Budget budget."
