[CmdletBinding()]
param(
    [string]$ReferenceRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) "refer"),
    [string]$OutputRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) "crates/nethop-subscription/tests/fixtures/territory")
)

$ErrorActionPreference = "Stop"

function Read-ProxyNames([string]$Path) {
    $insideProxies = $false
    $names = [Collections.Generic.List[string]]::new()
    foreach ($line in Get-Content -LiteralPath $Path) {
        if ($line -match '^proxies:\s*$') {
            $insideProxies = $true
            continue
        }
        if ($insideProxies -and $line -match '^proxy-groups:\s*$') { break }
        if (-not $insideProxies) { continue }

        $value = $null
        if ($line -match '^\s*-\s+name:\s*(.+?)\s*$') {
            $value = $Matches[1]
        }
        elseif ($line -match '^\s*-\s*\{\s*name:\s*(?:''([^'']*)''|"([^"]*)"|([^,}]+))') {
            $value = @($Matches[1], $Matches[2], $Matches[3]) | Where-Object { $_ } | Select-Object -First 1
        }
        if ($null -eq $value) { continue }
        $value = $value.Trim()
        if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        $names.Add($value)
    }
    return $names
}

function Expected-Territory([string]$Sample, [string]$Name) {
    if ($Sample -eq "glados") {
        if ($Name -match '^(US|TW|JP|SG)-') { return $Matches[1] }
        return $null
    }
    if ($Sample -eq "fsllist") {
        if ($Name -match '^([A-Z]{2})-') { return $Matches[1] }
        throw "fsllist node lacks an alpha-2 prefix: $Name"
    }
    $mapping = [ordered]@{
        "日本" = "JP"; "新加坡" = "SG"; "香港" = "HK"; "韩国" = "KR";
        "印度" = "IN"; "台湾" = "TW"; "美国" = "US"; "加拿大" = "CA";
        "法国" = "FR"; "德国" = "DE"; "英国" = "GB"; "越南" = "VN";
        "俄罗斯" = "RU"; "乌克兰" = "UA"; "土耳其" = "TR"; "尼日利亚" = "NG"
    }
    foreach ($entry in $mapping.GetEnumerator()) {
        if ($Name.StartsWith($entry.Key, [StringComparison]::Ordinal)) { return $entry.Value }
    }
    return $null
}

$sources = @(
    @{ id = "glados"; file = "glados-facility.com_khi1215215@163.yaml"; expected = 56 },
    @{ id = "magic-ring"; file = "魔戒.yaml"; expected = 44 },
    @{ id = "fsllist"; file = "fsllist.yaml"; expected = 51 }
)
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
foreach ($source in $sources) {
    $path = Join-Path $ReferenceRoot $source.file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "missing private reference sample: $($source.id)" }
    $names = @(Read-ProxyNames $path)
    if ($names.Count -ne $source.expected) { throw "unexpected proxy count for $($source.id): $($names.Count)" }
    $nodes = foreach ($name in $names) {
        $expected = Expected-Territory $source.id $name
        [ordered]@{
            name = $name
            expected_territory_code = $expected
            information_node = ($source.id -eq "magic-ring" -and $null -eq $expected)
        }
    }
    $fixture = [ordered]@{
        schema = "nethop-territory-name-fixture-v1"
        sample_id = $source.id
        source_sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        format = "clash_yaml"
        nodes = @($nodes)
    }
    $output = Join-Path $OutputRoot "$($source.id).json"
    [IO.File]::WriteAllText($output, (($fixture | ConvertTo-Json -Depth 5) + "`n"), [Text.UTF8Encoding]::new($false))
}

Write-Output "territory fixtures generated without connection parameters"
