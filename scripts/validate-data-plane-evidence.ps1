[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$EvidencePath
)

$ErrorActionPreference = "Stop"

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Assert-Number($Value, [double]$Minimum, [double]$Maximum, [string]$Name) {
    Assert-True ($null -ne $Value -and $Value -is [ValueType]) "$Name must be numeric"
    $number = [double]$Value
    Assert-True (-not [double]::IsNaN($number) -and -not [double]::IsInfinity($number)) "$Name must be finite"
    Assert-True ($number -ge $Minimum -and $number -le $Maximum) "$Name is outside its bounded range"
}

$resolved = (Resolve-Path -LiteralPath $EvidencePath).Path
$document = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
Assert-True ($document.schema -eq "nethop-data-plane-benchmark-v1") "data-plane evidence schema is invalid"
Assert-True ($document.build_manifest_sha256 -match '^[a-f0-9]{64}$') "build manifest digest is invalid"
Assert-True ($document.device.api_level -is [ValueType] -and [int]$document.device.api_level -ge 33) "device API level is invalid"
Assert-True ($document.device.kernel_release -is [string] -and $document.device.kernel_release.Length -in 1..128) "kernel release is invalid"
Assert-True ($document.device.build_fingerprint_sha256 -match '^[a-f0-9]{64}$') "device fingerprint digest is invalid"
Assert-True ($document.workload.id -match '^[A-Za-z0-9_.-]{1,64}$') "workload id is invalid"
Assert-True ($document.workload.duration_seconds -is [ValueType] -and [int]$document.workload.duration_seconds -in 10..3600) "workload duration is invalid"

$modes = @($document.modes)
Assert-True ($modes.Count -eq 2) "evidence must contain exactly TPROXY and TUN modes"
Assert-True ((@($modes.mode | Sort-Object -Unique) -join ',') -eq 'tproxy,tun') "evidence mode set is invalid"

$summary = [ordered]@{}
foreach ($mode in $modes) {
    $runs = @($mode.runs)
    Assert-True ($runs.Count -ge 5 -and $runs.Count -le 20) "$($mode.mode) run count is invalid"
    $expectedRun = 1
    foreach ($run in $runs) {
        Assert-True ([int]$run.run -eq $expectedRun) "$($mode.mode) run sequence is invalid"
        Assert-Number $run.ready_ms 0 120000 "$($mode.mode).ready_ms"
        Assert-Number $run.latency_p95_ms 0 120000 "$($mode.mode).latency_p95_ms"
        Assert-True ([int]$run.latency_sample_count -ge 20 -and [int]$run.latency_sample_count -le 10000) "$($mode.mode) latency sample count is invalid"
        Assert-Number $run.throughput_mbps 0.001 100000 "$($mode.mode).throughput_mbps"
        Assert-Number $run.cpu_percent 0 1000 "$($mode.mode).cpu_percent"
        Assert-Number $run.rss_bytes 1048576 4294967296 "$($mode.mode).rss_bytes"
        Assert-Number $run.power_delta_mah 0 10000 "$($mode.mode).power_delta_mah"
        Assert-Number $run.update_interruption_ms 0 120000 "$($mode.mode).update_interruption_ms"
        Assert-True ($run.raw_sha256 -match '^[a-f0-9]{64}$') "$($mode.mode) raw sample digest is invalid"
        $expectedRun += 1
    }
    $orderedThroughput = @($runs.throughput_mbps | ForEach-Object { [double]$_ } | Sort-Object)
    $middle = [int][Math]::Floor($orderedThroughput.Count / 2)
    $medianThroughput = if ($orderedThroughput.Count % 2 -eq 0) {
        ($orderedThroughput[$middle - 1] + $orderedThroughput[$middle]) / 2
    } else {
        $orderedThroughput[$middle]
    }
    $summary[$mode.mode] = [ordered]@{
        runs = $runs.Count
        median_throughput_mbps = [Math]::Round($medianThroughput, 3)
        worst_latency_p95_ms = [Math]::Round(($runs.latency_p95_ms | Measure-Object -Maximum).Maximum, 3)
        worst_ready_ms = [Math]::Round(($runs.ready_ms | Measure-Object -Maximum).Maximum, 3)
        worst_update_interruption_ms = [Math]::Round(($runs.update_interruption_ms | Measure-Object -Maximum).Maximum, 3)
        peak_rss_bytes = [long](($runs.rss_bytes | Measure-Object -Maximum).Maximum)
    }
}

[ordered]@{
    schema = "nethop-data-plane-benchmark-summary-v1"
    evidence_sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    modes = $summary
} | ConvertTo-Json -Depth 6 -Compress
