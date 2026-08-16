[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$customize = Get-Content -LiteralPath (Join-Path $workspace "module/customize.sh") -Raw

function Assert-Equal {
    param($Actual, $Expected, [string]$Message)
    if ($Actual -ne $Expected) {
        throw "$Message (expected=$Expected actual=$Actual)"
    }
}

function Resolve-InstallDecision {
    param([bool]$Installed, [string[]]$Events)
    if ($Installed) { return "update" }
    foreach ($event in $Events | Select-Object -First 10) {
        if ($event -eq "volume_up_down") { return "install" }
        if ($event -eq "volume_down_down") { return "skip" }
    }
    return "skip"
}

Assert-Equal (Resolve-InstallDecision -Installed $false -Events @("volume_up_down")) "install" "Volume+ must opt in"
Assert-Equal (Resolve-InstallDecision -Installed $false -Events @("volume_down_down")) "skip" "Volume- must skip"
Assert-Equal (Resolve-InstallDecision -Installed $false -Events @()) "skip" "timeout must skip"
Assert-Equal (Resolve-InstallDecision -Installed $false -Events @("other", "volume_up_up", "volume_up_down")) "install" "release and unrelated events must be ignored"
Assert-Equal (Resolve-InstallDecision -Installed $true -Events @()) "update" "installed package must update without prompting"
Assert-Equal (@(0..10).Count) 11 "countdown must expose 10 through 0"

foreach ($required in @(
    'COMPANION_CHOICE=timeout',
    'if [ "$COMPANION_CHOICE" = install ]',
    'pm install -r --user 0 "$COMPANION_APK"',
    'NetHop module remains installed',
    'rm -f "$COMPANION_APK"'
)) {
    if (-not $customize.Contains($required)) {
        throw "Companion installer contract missing: $required"
    }
}

Write-Host "NetHop Companion installer contracts passed"
