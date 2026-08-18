[CmdletBinding()]
param(
    [string]$DeviceSerial = "dc39c31d",
    [string]$OutputDirectory = "artifacts/data-plane/alioth-20260818",
    [string]$BuildManifestPath = "out/android-arm64-netproxy-improvements-20260818/module/build-manifest.json",
    [ValidateRange(5, 20)]
    [int]$Runs = 5
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $workspace $OutputDirectory))
$manifestPath = [IO.Path]::GetFullPath((Join-Path $workspace $BuildManifestPath))
$nethopctl = "/data/adb/modules/nethop/bin/nethopctl"
$downloadBytes = 20000000
$workloadTimeoutSeconds = 30
$latencySamples = 20
$modes = @("tproxy", "tun")

New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

function Invoke-Adb {
    param([Parameter(Mandatory)][string[]]$Arguments)
    $result = & adb -s $DeviceSerial @Arguments
    if ($LASTEXITCODE -ne 0) { throw "adb command failed: adb -s $DeviceSerial $($Arguments -join ' ')" }
    return ($result | Out-String).Trim()
}

function Invoke-Root {
    param([Parameter(Mandatory)][string]$Command)
    return Invoke-Adb @("shell", "su", "-c", $Command)
}

function Read-Json([string]$Text) {
    return ($Text | ConvertFrom-Json)
}

function Get-Config {
    return Read-Json (Invoke-Root "$nethopctl config get")
}

function Get-Status {
    return Read-Json (Invoke-Root "$nethopctl status --json")
}

function Set-CaptureMode([string]$Mode) {
    $config = Get-Config
    $digest = $config.result.active_config_digest
    $result = Read-Json (Invoke-Root "$nethopctl network set network.capture_mode $Mode --expected-digest $digest")
    if (-not $result.ok) { throw "capture mode mutation failed: $Mode" }
}

function Stop-Service {
    Invoke-Root "$nethopctl stop --wait" | Out-Null
}

function Start-Service {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Invoke-Root "$nethopctl start --wait" | Out-Null
    $watch.Stop()
    return [Math]::Round($watch.Elapsed.TotalMilliseconds, 3)
}

function Get-ProcessSample {
    $pids = @(Invoke-Root "pidof nethopd").Trim() -split '\s+' | Where-Object { $_ -match '^\d+$' }
    $ticks = [int64]0
    $rss = [int64]0
    foreach ($processId in $pids) {
        $stat = Invoke-Root "cat /proc/$processId/stat"
        $fields = $stat -split '\s+'
        if ($fields.Count -ge 15) { $ticks += [int64]$fields[13] + [int64]$fields[14] }
        $status = Invoke-Root "cat /proc/$processId/status"
        $match = [regex]::Match($status, '(?m)^VmRSS:\s+(\d+)\s+kB')
        if ($match.Success) { $rss += [int64]$match.Groups[1].Value * 1024 }
    }
    return [ordered]@{ pids = @($pids); cpu_ticks = $ticks; rss_bytes = $rss }
}

function Get-BatteryCounter {
    $value = Invoke-Root "cat /sys/class/power_supply/battery/charge_counter 2>/dev/null || true"
    if ($value -match '^\d+$') { return [int64]$value }
    return $null
}

function Get-LatencySamples {
    $values = @()
    $raw = @()
    for ($i = 0; $i -lt $latencySamples; $i++) {
        $line = Invoke-Adb @("shell", "curl", "--connect-timeout", "5", "--max-time", "10", "-sS", "-o", "/dev/null", "-w", "%{http_code}:%{time_total}", "https://cp.cloudflare.com/generate_204")
        $raw += $line
        $parts = $line -split ':'
        if ($parts.Count -ge 2 -and $parts[1] -as [double]) { $values += [double]$parts[1] * 1000 }
    }
    if ($values.Count -lt $latencySamples) { throw "latency sample count is $($values.Count), expected $latencySamples" }
    $ordered = @($values | Sort-Object)
    $index = [Math]::Ceiling($ordered.Count * 0.95) - 1
    return [ordered]@{ values_ms = $values; raw = $raw; p95_ms = [Math]::Round($ordered[$index], 3) }
}

function Get-ThroughputReport {
    $url = "https://speed.cloudflare.com/__down?bytes=$downloadBytes"
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        $result = & adb -s $DeviceSerial shell curl --connect-timeout 10 --max-time $workloadTimeoutSeconds -sS -o /dev/null -w "%{http_code}:%{size_download}:%{time_total}" $url 2>&1
        $exitCode = $LASTEXITCODE
        $text = ($result | Out-String).Trim()
        if ($exitCode -eq 0) {
            $parts = $text -split ':'
            $status = if ($parts.Count -ge 1) { $parts[0] } else { "" }
            $bytes = if ($parts.Count -ge 2) { [double]$parts[1] } else { 0 }
            $seconds = if ($parts.Count -ge 3) { [double]$parts[2] } else { 0 }
            $bps = if ($seconds -gt 0) { $bytes * 8 / $seconds } else { 0 }
            if ($status -ne "200") { throw "throughput endpoint returned HTTP $status" }
            if ($bps -le 0) { throw "throughput endpoint returned no positive receive throughput" }
            return [ordered]@{ bits_per_second = $bps; bytes = $bytes; seconds = $seconds; response = $text; attempt = $attempt }
        }
        if ($attempt -eq 3) {
            throw "throughput endpoint failed after $attempt attempt(s): $text"
        }
        Start-Sleep -Seconds 3
    }
    throw "throughput retry loop ended unexpectedly"
}

function Get-Sha256Text([string]$Text) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return (-join ($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") })) }
    finally { $sha.Dispose() }
}

function Get-DeviceFact([string]$Command) { return Invoke-Adb @("shell", $Command) }

$buildManifestSha = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
$fingerprint = Get-DeviceFact "getprop ro.build.fingerprint"
$kernel = Get-DeviceFact "uname -r"
$apiLevel = [int](Get-DeviceFact "getprop ro.build.version.sdk")
$clockTicks = [int](Get-DeviceFact "getconf CLK_TCK")
$allModes = [ordered]@{}

try {
    foreach ($mode in $modes) {
        $modeRuns = @()
        for ($run = 1; $run -le $Runs; $run++) {
            Stop-Service
            Set-CaptureMode $mode
            $readyMs = Start-Service
            $beforeStatus = Get-Status
            $beforeProcess = Get-ProcessSample
            $beforeBattery = Get-BatteryCounter
            $workloadWatch = [Diagnostics.Stopwatch]::StartNew()
            $throughput = Get-ThroughputReport
            $latency = Get-LatencySamples
            $reloadWatch = [Diagnostics.Stopwatch]::StartNew()
            Invoke-Root "$nethopctl config reload --wait" | Out-Null
            $reloadWatch.Stop()
            $workloadWatch.Stop()
            $afterProcess = Get-ProcessSample
            $afterBattery = Get-BatteryCounter
            $afterStatus = Get-Status
            $cpuPercent = if ($afterProcess.cpu_ticks -ge $beforeProcess.cpu_ticks) {
                [Math]::Round((($afterProcess.cpu_ticks - $beforeProcess.cpu_ticks) / $clockTicks / $workloadWatch.Elapsed.TotalSeconds) * 100, 3)
            } else { 0 }
            $powerDelta = if ($null -ne $beforeBattery -and $null -ne $afterBattery) {
                [Math]::Round([Math]::Max(0, ($beforeBattery - $afterBattery) / 1000), 3)
            } else { 0 }
            $rawObject = [ordered]@{
                schema = "nethop.data-plane.raw-v1"
                mode = $mode
                run = $run
                ready_ms = $readyMs
                before_status = $beforeStatus
                after_status = $afterStatus
                before_process = $beforeProcess
                after_process = $afterProcess
                before_battery_charge_counter_uah = $beforeBattery
                after_battery_charge_counter_uah = $afterBattery
                throughput = $throughput
                latency = $latency
                measurement_elapsed_ms = [Math]::Round($workloadWatch.Elapsed.TotalMilliseconds, 3)
                config_reload_elapsed_ms = [Math]::Round($reloadWatch.Elapsed.TotalMilliseconds, 3)
            }
            $rawText = $rawObject | ConvertTo-Json -Depth 30 -Compress
            $rawPath = Join-Path $outputRoot "$mode-run-$run.raw.json"
            [IO.File]::WriteAllText($rawPath, $rawText, [Text.UTF8Encoding]::new($false))
            $modeRuns += [ordered]@{
                run = $run
                ready_ms = $readyMs
                latency_p95_ms = $latency.p95_ms
                latency_sample_count = $latency.values_ms.Count
                throughput_mbps = [Math]::Round($throughput.bits_per_second / 1e6, 3)
                cpu_percent = $cpuPercent
                rss_bytes = [long]$afterProcess.rss_bytes
                power_delta_mah = $powerDelta
                update_interruption_ms = [Math]::Round($reloadWatch.Elapsed.TotalMilliseconds, 3)
                raw_sha256 = Get-Sha256Text $rawText
                raw_artifact = [IO.Path]::GetRelativePath($workspace, $rawPath).Replace('\', '/')
            }
            Stop-Service
        }
        $allModes[$mode] = [ordered]@{ mode = $mode; runs = @($modeRuns) }
    }
}
finally {
    try {
        Stop-Service
        Set-CaptureMode "auto"
        Start-Service | Out-Null
    } catch {
        Write-Error "failed to restore auto capture mode: $($_.Exception.Message)"
    }
}

$evidence = [ordered]@{
    schema = "nethop-data-plane-benchmark-v1"
    status = "measured_nonpaired_public_endpoint"
    build_manifest_sha256 = $buildManifestSha
    device = [ordered]@{
        serial = $DeviceSerial
        model = Get-DeviceFact "getprop ro.product.model"
        api_level = $apiLevel
        kernel_release = $kernel
        build_fingerprint_sha256 = Get-Sha256Text $fingerprint
    }
    workload = [ordered]@{
        id = "https-fixed-download-v1"
        endpoint = "https://speed.cloudflare.com/__down?bytes=$downloadBytes"
        duration_seconds = $workloadTimeoutSeconds
        transfer_bytes = $downloadBytes
        latency_endpoint = "https://cp.cloudflare.com/generate_204"
        latency_samples_per_run = $latencySamples
        clock_ticks_per_second = $clockTicks
        pairing = "none_public_endpoint"
    }
    modes = @($allModes.Values)
}
$evidencePath = Join-Path $outputRoot "evidence.json"
$evidence | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $evidencePath -Encoding utf8NoBOM
Write-Output "data-plane evidence written: $evidencePath"
