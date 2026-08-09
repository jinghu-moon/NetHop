[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$beforeCli = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/cli-v1-before.json") -Raw | ConvertFrom-Json
$afterCli = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/cli-v2-after.json") -Raw | ConvertFrom-Json
$beforeProtocol = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/protocol-v1-before.json") -Raw | ConvertFrom-Json
$afterProtocol = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/protocol-v2-after.json") -Raw | ConvertFrom-Json
$errors = Get-Content -LiteralPath (Join-Path $workspace "tests/webui/fixtures/webui-error-codes.json") -Raw | ConvertFrom-Json

Assert-True ($beforeCli.protocol_version -eq 1 -and $beforeProtocol.protocol_version -eq 1) "before protocol evidence must remain v1"
Assert-True ($afterCli.protocol_version -eq 2 -and $afterProtocol.protocol_version -eq 2) "after protocol evidence must be v2"
Assert-True ($afterCli.inherits_legacy_commands_from -eq "cli-v1-before.json") "after CLI must inherit the audited legacy map"
Assert-True ($afterCli.allowed_legacy_method_changes.Count -eq 0) "legacy CLI method changes are forbidden"

$legacyIds = @($beforeCli.commands | ForEach-Object { $_.id })
Assert-True ($legacyIds.Count -eq ($legacyIds | Sort-Object -Unique).Count) "legacy CLI IDs are not unique"
$addedIds = @($afterCli.added_commands | ForEach-Object { $_.id })
Assert-True ($addedIds.Count -eq 4) "exactly four payload commands must be added"
Assert-True (($legacyIds | Where-Object { $addedIds -contains $_ }).Count -eq 0) "payload commands replace a legacy command"

$beforeEvents = @($beforeProtocol.events | ForEach-Object { $_.event_kind })
$afterEvents = @($afterProtocol.event_kinds)
$eventDelta = @($afterEvents | Where-Object { $beforeEvents -notcontains $_ })
Assert-True ($eventDelta.Count -eq 1 -and $eventDelta[0] -eq "traffic") "traffic must be the only event-kind addition"
Assert-True (($beforeEvents | Where-Object { $afterEvents -notcontains $_ }).Count -eq 0) "a legacy event kind was removed"

$expectedCodes = @(
    "NH-CORE-INCOMPATIBLE", "NH-CORE-TIMEOUT", "NH-CONFIG-CONFLICT",
    "NH-CONFIG-INVALID-PAYLOAD", "NH-CORE-LIMIT", "NH-CORE-UNAVAILABLE"
)
$actualCodes = @($errors.codes.PSObject.Properties.Value)
Assert-True (($expectedCodes | Where-Object { $actualCodes -notcontains $_ }).Count -eq 0) "WebUI error code fixture drifted"

Write-Host "webui phase B contracts passed"
