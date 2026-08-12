[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$module = Join-Path $workspace "module"
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("nethop-fake-magisk-" + [Guid]::NewGuid().ToString("N"))
$currentSchemaVersion = 3

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Install-FakeModule {
    param([string]$DataRoot, [string]$ModuleRoot)

    $directories = @(
        "config", "generations", "subscriptions/cache", "subscriptions/reports",
        "rulesets", "stats", "state", "run", "logs"
    )
    foreach ($relative in $directories) {
        New-Item -ItemType Directory -Force -Path (Join-Path $DataRoot $relative) | Out-Null
    }
    $config = Join-Path $DataRoot "config/nethop.toml"
    if (-not (Test-Path -LiteralPath $config)) {
        Copy-Item -LiteralPath (Join-Path $ModuleRoot "defaults/nethop.toml") -Destination $config
    }
    elseif ((Get-Item -LiteralPath $config).PSIsContainer) {
        throw "existing managed config is not a regular file"
    }
    elseif ((Get-Content -LiteralPath $config -Raw) -notmatch "(?m)^\s*schema_version\s*=\s*$currentSchemaVersion\s*$") {
        $backup = "$config.pre-v3"
        if (-not (Test-Path -LiteralPath $backup)) {
            Copy-Item -LiteralPath $config -Destination $backup
        }
        Copy-Item -LiteralPath (Join-Path $ModuleRoot "defaults/nethop.toml") -Destination $config -Force
    }
    foreach ($asset in @("cn-domain.srs", "cn-ip.srs")) {
        $source = Join-Path $ModuleRoot "rulesets/$asset"
        $destination = Join-Path $DataRoot "rulesets/$asset"
        $temporary = "$destination.new"
        Copy-Item -LiteralPath $source -Destination $temporary -Force
        Move-Item -LiteralPath $temporary -Destination $destination -Force
    }
    return $config
}

function Invoke-FakeActivation {
    param([string]$DataRoot, [string]$FailAt)

    $current = Join-Path $DataRoot "state/current"
    $candidate = Join-Path $DataRoot "generations/2"
    New-Item -ItemType Directory -Force -Path $candidate | Out-Null
    Set-Content -LiteralPath (Join-Path $candidate "config.json") -NoNewline -Value '{"candidate":true}'
    foreach ($boundary in @("prepared", "checked", "core_started", "network_applied", "health_passed")) {
        if ($FailAt -eq $boundary) {
            Remove-Item -LiteralPath $candidate -Recurse -Force
            return $false
        }
    }
    Set-Content -LiteralPath $current -NoNewline -Value "2"
    return $true
}

try {
    $moduleRoot = Join-Path $sandbox "data/adb/modules/nethop"
    $dataRoot = Join-Path $sandbox "data/adb/nethop"
    New-Item -ItemType Directory -Force -Path $moduleRoot, $dataRoot | Out-Null
    Copy-Item -LiteralPath (Join-Path $module "defaults") -Destination $moduleRoot -Recurse
    Copy-Item -LiteralPath (Join-Path $module "rulesets") -Destination $moduleRoot -Recurse
    Copy-Item -LiteralPath (Join-Path $module "service.sh") -Destination $moduleRoot
    Copy-Item -LiteralPath (Join-Path $module "action.sh") -Destination $moduleRoot

    $config = Install-FakeModule -DataRoot $dataRoot -ModuleRoot $moduleRoot
    Assert-True (Test-Path -LiteralPath $config -PathType Leaf) "fake install did not publish persistent config"
    $customCurrent = @"
schema_version = 3
[service]
enabled = false
[[subscriptions.sources]]
name = "Preserved"
enabled = true
url = "https://subscription.example.invalid/private-marker"
"@
    Set-Content -LiteralPath $config -NoNewline -Value $customCurrent
    $sameConfig = Install-FakeModule -DataRoot $dataRoot -ModuleRoot $moduleRoot
    Assert-True ((Get-Content -LiteralPath $sameConfig -Raw) -eq $customCurrent) "upgrade overwrote current user config"

    $legacyConfig = "schema_version = 2`n[service]`nenabled = true`n"
    Set-Content -LiteralPath $config -NoNewline -Value $legacyConfig
    $resetConfig = Install-FakeModule -DataRoot $dataRoot -ModuleRoot $moduleRoot
    $backup = "$config.pre-v3"
    Assert-True ((Get-Content -LiteralPath $backup -Raw) -eq $legacyConfig) "upgrade did not preserve the pre-v3 config"
    Assert-True ((Get-Content -LiteralPath $resetConfig -Raw) -match '(?m)^schema_version = 3$') "upgrade did not reset a pre-v3 config"

    Set-Content -LiteralPath $config -NoNewline -Value "schema_version = 1`n[service]`nenabled = false`n"
    [void](Install-FakeModule -DataRoot $dataRoot -ModuleRoot $moduleRoot)
    Assert-True ((Get-Content -LiteralPath $backup -Raw) -eq $legacyConfig) "upgrade overwrote the first pre-v3 backup"
    foreach ($asset in @("cn-domain.srs", "cn-ip.srs")) {
        $moduleDigest = (Get-FileHash -LiteralPath (Join-Path $moduleRoot "rulesets/$asset") -Algorithm SHA256).Hash
        $persistentDigest = (Get-FileHash -LiteralPath (Join-Path $dataRoot "rulesets/$asset") -Algorithm SHA256).Hash
        Assert-True ($moduleDigest -eq $persistentDigest) "persistent $asset does not match the verified module baseline"
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $dataRoot "rulesets/$asset.new"))) "persistent $asset leaked a staging file"
    }

    Set-Content -LiteralPath (Join-Path $dataRoot "state/current") -NoNewline -Value "1"
    foreach ($boundary in @("prepared", "checked", "core_started", "network_applied", "health_passed")) {
        Assert-True (-not (Invoke-FakeActivation -DataRoot $dataRoot -FailAt $boundary)) "fault was not injected at $boundary"
        Assert-True ((Get-Content -LiteralPath (Join-Path $dataRoot "state/current") -Raw) -eq "1") "failed activation changed current at $boundary"
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $dataRoot "generations/2"))) "failed activation leaked candidate at $boundary"
    }
    Assert-True (Invoke-FakeActivation -DataRoot $dataRoot -FailAt "") "healthy activation failed"
    Assert-True ((Get-Content -LiteralPath (Join-Path $dataRoot "state/current") -Raw) -eq "2") "healthy activation did not commit"

    $service = Get-Content -LiteralPath (Join-Path $moduleRoot "service.sh") -Raw
    $action = Get-Content -LiteralPath (Join-Path $moduleRoot "action.sh") -Raw
    Assert-True ($service.Contains('exec "$MODDIR/bin/nethopd" --supervise --root /data/adb/nethop')) "service bypasses supervisor"
    Assert-True ($action.IndexOf('config reload --wait') -lt $action.IndexOf('update --if-needed --wait')) "action ordering drifted"
    Assert-True ($action.IndexOf('update --if-needed --wait') -lt $action.IndexOf('"$CTL" status --human')) "action status ordering drifted"

    Write-Host "NetHop fake Magisk smoke passed"
}
finally {
    if (Test-Path -LiteralPath $sandbox) {
        Remove-Item -LiteralPath $sandbox -Recurse -Force
    }
}
