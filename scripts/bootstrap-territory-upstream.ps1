[CmdletBinding()]
param(
    [string]$SourceDirectory = "refer/territory-upstream"
)

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $workspace $SourceDirectory))
$destinationRoot = Join-Path $workspace "data/territories/upstream"
$licenseRoot = Join-Path $workspace "licenses"

$files = [ordered]@{
    "un-m49-country-area-en.csv" = "da52466df41547599ecb7e5efdd9f20a765403cf936bacc49300cb9cf11e5236"
    "cldr-48.2.0-territories-en.json" = "158c1d575308f7e46912edbeda435c8fe2ef5dad280798231f3a432e406b1807"
    "cldr-48.2.0-territories-zh-Hans.json" = "de255a878dc85dbf801353e13e7d6685eccebe238b485818f129a6ad82212d43"
    "cldr-48.2.0-codeMappings.json" = "0d1ef50b92c1140e5847d22d96faf1a9c35543b0dedf8e9a2fcb87e4c51b9ed6"
    "country-flag-icons-1.6.20.tgz" = "0b003c0984b53e12a870f0b9a2ccd9821c588c505cf18d7938dabec3ae669808"
}

foreach ($entry in $files.GetEnumerator()) {
    $source = Join-Path $sourceRoot $entry.Key
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "required upstream file is missing: $($entry.Key)"
    }
    $actual = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entry.Value) {
        throw "upstream digest mismatch: $($entry.Key)"
    }
}

$unicodeLicense = Join-Path $sourceRoot "cldr-48.2.0-LICENSE.txt"
if ((Get-FileHash -LiteralPath $unicodeLicense -Algorithm SHA256).Hash.ToLowerInvariant() -ne "220ba0e1c43b99530d2d5bdb892a99dca0989414f51ab695ecd90163eaa1ec3b") {
    throw "Unicode license digest mismatch"
}

New-Item -ItemType Directory -Force -Path $destinationRoot | Out-Null
New-Item -ItemType Directory -Force -Path $licenseRoot | Out-Null
foreach ($name in $files.Keys) {
    Copy-Item -LiteralPath (Join-Path $sourceRoot $name) -Destination (Join-Path $destinationRoot $name) -Force
}
Copy-Item -LiteralPath $unicodeLicense -Destination (Join-Path $licenseRoot "Unicode-3.0.txt") -Force

$license = (& tar -xOf (Join-Path $sourceRoot "country-flag-icons-1.6.20.tgz") "package/LICENSE" | Out-String).TrimEnd()
if ([string]::IsNullOrWhiteSpace($license) -or $license -notmatch "MIT License") {
    throw "country-flag-icons license is missing or unexpected"
}
[IO.File]::WriteAllText((Join-Path $licenseRoot "country-flag-icons-MIT.txt"), $license + "`n", [Text.UTF8Encoding]::new($false))

Write-Output "territory upstream bootstrap passed"
