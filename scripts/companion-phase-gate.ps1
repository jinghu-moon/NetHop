[CmdletBinding()]
param(
    [string]$EvidencePath = "artifacts/companion/host/phase-summary.json"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$results = [Collections.Generic.List[object]]::new()

function Invoke-GateCheck {
    param([string]$Name, [scriptblock]$Action)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & $Action
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) { $exitCode = 0 }
    $timer.Stop()
    $script:results.Add([ordered]@{ name = $Name; exit_code = $exitCode; duration_ms = $timer.ElapsedMilliseconds })
    if ($exitCode -ne 0) { throw "companion_gate_failed:$Name" }
}

Push-Location $workspace
try {
    $baselinePath = "tests/companion/fixtures/before-baseline.json"
    $baseline = Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json
    if ($baseline.control_protocol_version -ne 5 -or $baseline.status_document_schema_version -ne 1 -or
        $baseline.module_contains_companion -ne $false) {
        throw "companion_before_baseline_invalid"
    }
    Invoke-GateCheck "evidence" { pwsh -NoProfile -File "scripts/companion-evidence-contracts.ps1" -SelfTest }
    Invoke-GateCheck "module-contracts" { pwsh -NoProfile -File "scripts/module-contracts.ps1" }
    Invoke-GateCheck "installer-contracts" { pwsh -NoProfile -File "scripts/companion-installer-contracts.ps1" }
    Invoke-GateCheck "companion-jvm" { & "companion/gradlew.bat" --no-configuration-cache -p "companion" testDebugUnitTest }
    Invoke-GateCheck "companion-android-test-compile" { & "companion/gradlew.bat" --no-configuration-cache -p "companion" assembleDebugAndroidTest }
    Invoke-GateCheck "webui-typecheck" { npm --prefix "webui" run typecheck }
    Invoke-GateCheck "webui-full" { npm --prefix "webui" test }
    Invoke-GateCheck "protocol" { cargo test -p nethop-protocol }
    Invoke-GateCheck "daemon-status" { cargo test -p nethopd --test worker_application_contracts }
    Invoke-GateCheck "cli" { cargo test -p nethopctl --test cli_contracts }

    $gitCommit = (& git rev-parse HEAD | Out-String).Trim()
    if (& git status --porcelain) { $gitCommit = "$gitCommit-dirty" }
    $manifest = [ordered]@{
        schema_version = 1
        phase = "M"
        git_commit = $gitCommit
        commands = @($results | ForEach-Object { $_.name })
        results = @($results)
        artifacts = @([ordered]@{
            path = $baselinePath
            sha256 = (Get-FileHash -LiteralPath $baselinePath -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        before_manifest_sha256 = (Get-FileHash -LiteralPath $baselinePath -Algorithm SHA256).Hash.ToLowerInvariant()
        after_manifest_sha256 = $null
        contains_sensitive_data = $false
    }
    $absoluteEvidence = [IO.Path]::GetFullPath((Join-Path $workspace $EvidencePath))
    $workspacePrefix = [IO.Path]::GetFullPath($workspace).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $absoluteEvidence.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "companion_evidence_path_outside_workspace"
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $absoluteEvidence) -Force | Out-Null
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $absoluteEvidence -Encoding utf8NoBOM
    pwsh -NoProfile -File "scripts/companion-evidence-contracts.ps1" -Path $absoluteEvidence
    Write-Host "NetHop Companion phase gate passed"
}
finally {
    Pop-Location
}
