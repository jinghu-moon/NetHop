[CmdletBinding()]
param(
    [string]$Path,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"

function Assert-Evidence {
    param([Parameter(Mandatory)][string]$Json)
    if ($Json -match '(?i)-----BEGIN [A-Z ]*PRIVATE KEY-----' -or
        $Json -match '(?i)"(?:token|password|ssid|bssid|subscription_url|android_id)"\s*:\s*"(?!\[REDACTED\])[^"\s]+"' -or
        $Json -match '(?i)https?://(?!appassets\.androidplatform\.net)') {
        throw "companion_evidence_sensitive_data"
    }
    $document = $Json | ConvertFrom-Json
    if ($document.schema_version -ne 1 -or $document.phase -notmatch '^[A-N]$' -or
        [string]::IsNullOrWhiteSpace($document.git_commit) -or $null -eq $document.commands -or
        $null -eq $document.results -or $null -eq $document.artifacts -or
        $document.contains_sensitive_data -ne $false) {
        throw "companion_evidence_schema_invalid"
    }
    foreach ($artifact in @($document.artifacts)) {
        if ($artifact.path -match '^[A-Za-z]:[/\\]' -or $artifact.path.StartsWith('/') -or
            $artifact.sha256 -notmatch '^[a-f0-9]{64}$') {
            throw "companion_evidence_artifact_invalid"
        }
    }
}

if ($SelfTest) {
    $valid = [ordered]@{
        schema_version = 1
        phase = "A"
        git_commit = "dirty-tree"
        commands = @("contract")
        results = @([ordered]@{ name = "contract"; exit_code = 0; duration_ms = 1 })
        artifacts = @([ordered]@{ path = "tests/fixture.json"; sha256 = "0" * 64 })
        contains_sensitive_data = $false
    } | ConvertTo-Json -Depth 5
    Assert-Evidence -Json $valid
    $invalidSchema = $valid | ConvertFrom-Json
    $invalidSchema.contains_sensitive_data = $true
    try {
        Assert-Evidence -Json ($invalidSchema | ConvertTo-Json -Depth 5)
        throw "companion_evidence_self_test_failed"
    } catch {
        if ($_.Exception.Message -eq "companion_evidence_self_test_failed") { throw }
    }
    $secret = $valid | ConvertFrom-Json
    $secret | Add-Member -NotePropertyName token -NotePropertyValue private
    try {
        Assert-Evidence -Json ($secret | ConvertTo-Json -Depth 5)
        throw "companion_evidence_secret_self_test_failed"
    } catch {
        if ($_.Exception.Message -eq "companion_evidence_secret_self_test_failed") { throw }
    }
    Write-Host "NetHop Companion evidence self-test passed"
}

if ($Path) {
    Assert-Evidence -Json (Get-Content -LiteralPath $Path -Raw)
    Write-Host "NetHop Companion evidence passed: $Path"
}
