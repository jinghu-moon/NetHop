[CmdletBinding()]
param(
    [string]$Serial,
    [ValidateRange(1, 100)]
    [int]$Samples = 20,
    [string]$OutputDirectory = "artifacts/cli-performance/android",
    [ValidateRange(1, 60000)]
    [int]$ReadOnlyP95BudgetMs = 500,
    [ValidateRange(1, 60000)]
    [int]$CaptureP95BudgetMs = 2000,
    [switch]$IncludeCaptureToggle
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$cliPath = "/data/adb/modules/nethop/bin/nethopctl"
if ([string]::IsNullOrWhiteSpace($Serial)) {
    $Serial = (& adb get-serialno 2>&1 | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($Serial) -or $Serial -eq "unknown") {
        throw "no Android device is available"
    }
}
$outputRoot = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
$workspaceRoot = [IO.Path]::GetFullPath($workspace).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not ($outputRoot + [IO.Path]::DirectorySeparatorChar).StartsWith($workspaceRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "output directory must stay inside the workspace"
}

function Invoke-AdbRoot {
    param([Parameter(Mandatory)][string]$Command)

    $escaped = $Command.Replace("'", "'\''")
    $adbCommand = "su -c '" + $escaped + "'"
    $adbArgs = @()
    if (-not [string]::IsNullOrWhiteSpace($Serial)) {
        $adbArgs += @("-s", $Serial)
    }
    $adbArgs += @("shell", $adbCommand)
    $output = (& adb @adbArgs 2>&1 | Out-String).Trim()
    [pscustomobject]@{
        output = $output
        exit_code = $LASTEXITCODE
    }
}

function Invoke-DeviceValue {
    param([Parameter(Mandatory)][string]$Command)

    $result = Invoke-AdbRoot $Command
    if ($result.exit_code -ne 0) {
        throw "device command failed: $Command`n$($result.output)"
    }
    return $result.output
}

function Invoke-DeviceJson {
    param([Parameter(Mandatory)][string]$Command)

    $raw = Invoke-DeviceValue $Command
    try {
        return $raw | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "device command returned invalid JSON: $Command`n$raw"
    }
}

function Invoke-TimedCli {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Arguments
    )

    $command = "$cliPath $Arguments >/dev/null 2>&1"
    $remote = 'start=$(toybox date +%s%3N); ' + $command + '; rc=$?; end=$(toybox date +%s%3N); printf "__NH_CLI_PERF__ %s %s\n" "$rc" "$((end-start))"'
    $result = Invoke-AdbRoot $remote
    $match = [regex]::Match($result.output, "__NH_CLI_PERF__\s+(-?\d+)\s+(\d+)")
    if (-not $match.Success) {
        throw "timed CLI command returned no measurement: $Name`n$($result.output)"
    }
    [ordered]@{
        name = $Name
        command = "$cliPath $Arguments"
        exit_code = [int]$match.Groups[1].Value
        elapsed_ms = [int64]$match.Groups[2].Value
    }
}

function Get-Percentile {
    param(
        [Parameter(Mandatory)][double[]]$Values,
        [Parameter(Mandatory)][double]$Percent
    )

    $ordered = @($Values | Sort-Object)
    if ($ordered.Count -eq 0) { return 0 }
    $rank = ($ordered.Count - 1) * ($Percent / 100)
    $lower = [math]::Floor($rank)
    $upper = [math]::Ceiling($rank)
    if ($lower -eq $upper) { return [double]$ordered[$lower] }
    return [double]$ordered[$lower] + ([double]$ordered[$upper] - [double]$ordered[$lower]) * ($rank - $lower)
}

function Get-Stats {
    param([Parameter(Mandatory)][object[]]$Samples)

    $values = @($Samples | ForEach-Object { [double]$_.elapsed_ms })
    [ordered]@{
        count = $values.Count
        p50_ms = [math]::Round((Get-Percentile $values 50), 3)
        p95_ms = [math]::Round((Get-Percentile $values 95), 3)
        max_ms = [math]::Round((($values | Measure-Object -Maximum).Maximum), 3)
        failures = @($Samples | Where-Object { $_.exit_code -ne 0 }).Count
    }
}

function Get-CaptureEnabled {
    $status = Invoke-DeviceJson "$cliPath status --json"
    $lifecycle = $status.result.lifecycle
    if ($null -ne $lifecycle -and $lifecycle.capture_state -eq "enabled") { return $true }
    if ($null -ne $status.result.capture -and $status.result.capture.active -eq $true) { return $true }
    return $false
}

Push-Location $workspace
try {
    $null = Invoke-DeviceValue "id"
    $model = Invoke-DeviceValue "getprop ro.product.model"
    $androidRelease = Invoke-DeviceValue "getprop ro.build.version.release"
    $androidSdk = Invoke-DeviceValue "getprop ro.build.version.sdk"
    $initialCaptureEnabled = Get-CaptureEnabled
    $allSamples = [Collections.Generic.List[object]]::new()

    $readOnlyCases = @(
        @{ name = "status"; args = "status --json" },
        @{ name = "config-check"; args = "config check --json" },
        @{ name = "capture-status"; args = "capture status --json" },
        @{ name = "core-status"; args = "core status --json" },
        @{ name = "node-list"; args = "node list --limit 64 --json" }
    )
    foreach ($case in $readOnlyCases) {
        for ($index = 0; $index -lt $Samples; $index++) {
            $sample = Invoke-TimedCli $case.name $case.args
            $allSamples.Add($sample)
        }
    }

    if ($IncludeCaptureToggle) {
        $enableFirst = -not $initialCaptureEnabled
        for ($index = 0; $index -lt $Samples; $index++) {
            $first = if ($enableFirst) { "enable" } else { "disable" }
            $second = if ($enableFirst) { "disable" } else { "enable" }
            $allSamples.Add((Invoke-TimedCli "capture-$first" "capture $first --json --wait"))
            $allSamples.Add((Invoke-TimedCli "capture-$second" "capture $second --json --wait"))
        }
        if ((Get-CaptureEnabled) -ne $initialCaptureEnabled) {
            $restore = if ($initialCaptureEnabled) { "enable" } else { "disable" }
            $null = Invoke-TimedCli "capture-restore" "capture $restore --json --wait"
        }
    }

    $sampleNames = @($allSamples | ForEach-Object { $_["name"] } | Select-Object -Unique)
    $summaries = @(
        $sampleNames | ForEach-Object {
            $name = [string]$_
            $group = @($allSamples | Where-Object { $_["name"] -eq $name })
            $stats = Get-Stats $group
            $isCapture = $name -in @("capture-enable", "capture-disable", "capture-restore")
            [ordered]@{
                name = $name
                command = $group[0]["command"]
                budget_p95_ms = if ($isCapture) { $CaptureP95BudgetMs } else { $ReadOnlyP95BudgetMs }
                stats = $stats
                passed = ($stats.failures -eq 0 -and $stats.p95_ms -le $(if ($isCapture) { $CaptureP95BudgetMs } else { $ReadOnlyP95BudgetMs }))
            }
        }
    )
    $passed = @($summaries | Where-Object { -not $_.passed }).Count -eq 0
    $revision = (git rev-parse HEAD).Trim()
    $report = [ordered]@{
        schema = "nethop-cli-backend-performance-v1"
        revision = $revision
        device = [ordered]@{
            serial = $Serial
            model = $model
            android_release = $androidRelease
            android_sdk = $androidSdk
            abi = (Invoke-DeviceValue "getprop ro.product.cpu.abi")
        }
        scope = "device-side nethopctl plus nethopd UDS handling; excludes WebUI and Android Bridge rendering"
        samples_per_read_only_case = $Samples
        include_capture_toggle = [bool]$IncludeCaptureToggle
        initial_capture_enabled = $initialCaptureEnabled
        budgets = [ordered]@{
            read_only_p95_ms = $ReadOnlyP95BudgetMs
            capture_p95_ms = $CaptureP95BudgetMs
        }
        summaries = @($summaries)
        samples = @($allSamples)
        passed = $passed
        contains_sensitive_data = $false
    }
    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $reportPath = Join-Path $outputRoot "cli-backend-$timestamp.json"
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8NoBOM
    $p95Text = ($summaries | ForEach-Object { "$($_.name)=$($_.stats.p95_ms)ms" }) -join ", "
    Write-Output "report: $reportPath"
    Write-Output "device: $model Android $androidRelease (API $androidSdk)"
    Write-Output "p95: $p95Text"
    Write-Output "passed: $passed"
    if (-not $passed) { exit 1 }
}
finally {
    Pop-Location
}
