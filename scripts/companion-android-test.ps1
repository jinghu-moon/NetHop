[CmdletBinding()]
param([string]$Serial)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$adbArguments = if ([string]::IsNullOrWhiteSpace($Serial)) { @() } else { @("-s", $Serial) }
$appApk = Join-Path $workspace "companion/app/build/outputs/apk/debug/app-debug.apk"
$testApk = Join-Path $workspace "companion/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
$remoteApp = "/data/local/tmp/nethop-companion-debug.apk"
$remoteTest = "/data/local/tmp/nethop-companion-debug-test.apk"

function Invoke-Adb {
    param([Parameter(Mandatory)][string[]]$Arguments)
    & adb @adbArguments @Arguments
    if ($LASTEXITCODE -ne 0) { throw "companion_android_test_adb_failed:$($Arguments[0])" }
}

Push-Location $workspace
try {
    & "companion/gradlew.bat" --no-configuration-cache -p "companion" assembleDebug assembleDebugAndroidTest
    if ($LASTEXITCODE -ne 0) { throw "companion_android_test_build_failed" }
    Invoke-Adb @("push", $appApk, $remoteApp)
    Invoke-Adb @("push", $testApk, $remoteTest)
    Invoke-Adb @("shell", "su", "-c", "pm install -r -t --user 0 $remoteApp")
    Invoke-Adb @("shell", "su", "-c", "pm install -r -t --user 0 $remoteTest")
    $result = & adb @adbArguments shell am instrument -w -r `
        "com.jinghumoon.nethop.companion.debug.test/androidx.test.runner.AndroidJUnitRunner" | Out-String
    if ($LASTEXITCODE -ne 0 -or $result -notmatch 'OK \([1-9][0-9]* tests?\)' -or
        $result -notmatch 'INSTRUMENTATION_CODE: -1') {
        throw "companion_android_instrumentation_failed`n$result"
    }
    Write-Host $result.TrimEnd()
    Write-Host "NetHop Companion Android instrumentation passed"
}
finally {
    & adb @adbArguments shell su -c "rm -f $remoteApp $remoteTest" | Out-Null
    Pop-Location
}
