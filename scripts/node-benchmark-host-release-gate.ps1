[CmdletBinding()]
param(
    [string]$OutputDirectory = "artifacts/tdd-node-benchmark/host-release"
)

$ErrorActionPreference = "Stop"

$workspace = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
$workspaceRoot = [IO.Path]::GetFullPath($workspace).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not ($outputRoot + [IO.Path]::DirectorySeparatorChar).StartsWith($workspaceRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "output directory must stay inside the workspace"
}

New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$reportPath = Join-Path $outputRoot "report.json"
$postprocessPath = Join-Path $outputRoot "postprocess.json"
$manifestPath = Join-Path $outputRoot "manifest.json"
$targetDirectory = Join-Path $workspace "target/d14-host-release"

Push-Location $workspace
try {
    $previousTarget = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = $targetDirectory
    try {
        $raw = (& cargo run --locked --release -p nethopd --example node_benchmark_evidence --features benchmark-evidence 2>$null | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) {
            throw "release evidence runner failed"
        }
        $postprocessRaw = (& cargo run --locked --release -p nethopd --example node_benchmark_postprocess_evidence --features benchmark-evidence 2>$null | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($postprocessRaw)) {
            throw "release postprocess evidence runner failed"
        }
    }
    finally {
        $env:CARGO_TARGET_DIR = $previousTarget
    }

    try {
        $report = $raw | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "release evidence runner did not emit strict JSON"
    }
    if ($report.schema -ne "nethop-node-benchmark-host-release-v1" -or -not $report.release_profile -or -not $report.passed) {
        throw "release evidence summary is invalid"
    }
    $expectedCases = @(
        @{ scenario = "success"; candidates = 1; samples = 20 },
        @{ scenario = "success"; candidates = 16; samples = 20 },
        @{ scenario = "success"; candidates = 27; samples = 20 },
        @{ scenario = "success"; candidates = 64; samples = 20 },
        @{ scenario = "mixed"; candidates = 64; samples = 3 },
        @{ scenario = "timeout"; candidates = 64; samples = 3 }
    )
    if (@($report.cases).Count -ne $expectedCases.Count -or @($report.bootstrap_raw_micros).Count -ne 100) {
        throw "release evidence sample matrix is incomplete"
    }
    foreach ($expected in $expectedCases) {
        $case = @($report.cases | Where-Object {
            $_.scenario -eq $expected.scenario -and $_.candidates -eq $expected.candidates
        })
        if ($case.Count -ne 1 -or @($case[0].samples).Count -ne $expected.samples) {
            throw "release evidence case is missing: $($expected.scenario)/$($expected.candidates)"
        }
        if ([double]$case[0].wall_ms.p95 -gt 5000) {
            throw "release evidence p95 exceeds 5 seconds: $($expected.scenario)/$($expected.candidates)"
        }
        foreach ($sample in @($case[0].samples)) {
            if ([double]$sample.wall_ms -gt 5000 -or
                [int]$sample.peak_tasks -gt 64 -or
                [int]$sample.peak_sockets -gt 64 -or
                [int]$sample.residual_tasks -ne 0 -or
                [int]$sample.residual_sockets -ne 0 -or
                [long]$sample.peak_heap_delta_bytes -gt 4194304) {
                throw "release evidence resource or SLA sample failed: $($expected.scenario)/$($expected.candidates)"
            }
        }
    }
    if ($raw -match "Bearer|terminal-|subscription|token=|https://") {
        throw "release evidence contains a sensitive implementation value"
    }
    try {
        $postprocess = $postprocessRaw | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "postprocess evidence runner did not emit strict JSON"
    }
    if ($postprocess.schema -ne "nethop-node-benchmark-postprocess-v1" -or
        -not $postprocess.passed -or
        @($postprocess.samples_ms).Count -ne 20 -or
        [double]$postprocess.elapsed_ms.p95 -gt 100 -or
        [int]$postprocess.request_count -ne [int]$postprocess.expected_request_count -or
        [int]$postprocess.put_count -ne [int]$postprocess.expected_put_count) {
        throw "postprocess evidence violates the 100ms or selector request contract"
    }

    [IO.File]::WriteAllText($reportPath, ($report | ConvertTo-Json -Depth 100), [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($postprocessPath, ($postprocess | ConvertTo-Json -Depth 100), [Text.UTF8Encoding]::new($false))
    $revision = (& git rev-parse HEAD).Trim()
    $rustc = (& rustc --version).Trim()
    $cargo = (& cargo --version).Trim()
    $reportDigest = (Get-FileHash -LiteralPath $reportPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $postprocessDigest = (Get-FileHash -LiteralPath $postprocessPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $manifest = [ordered]@{
        schema = "nethop-tdd-evidence-v1"
        task_ids = @("E009", "E010", "F011", "L004", "L005", "L008")
        revision = $revision
        dirty_worktree = [bool](-not [string]::IsNullOrWhiteSpace((& git status --porcelain | Out-String)))
        command = "cargo run --locked --release -p nethopd --example node_benchmark_evidence --features benchmark-evidence"
        exit_code = 0
        target = "$($report.target_arch)-$($report.target_os)"
        rustc = $rustc
        cargo = $cargo
        report_sha256 = $reportDigest
        postprocess_sha256 = $postprocessDigest
        android_resource_gate = "pending"
    }
    [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
    Write-Output "node benchmark host release gate passed"
    Write-Output "report: $reportPath"
    Write-Output "postprocess: $postprocessPath"
    Write-Output "manifest: $manifestPath"
}
finally {
    Pop-Location
}
