[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $workspace "data/territories/source-versions.json"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.schema -ne "nethop-territory-sources-v1" -or @($manifest.sources).Count -ne 5) {
    throw "territory source manifest is invalid"
}
foreach ($source in $manifest.sources) {
    if (-not ([string]$source.url).StartsWith("https://", [StringComparison]::Ordinal) -or
        [string]::IsNullOrWhiteSpace($source.version) -or
        [string]::IsNullOrWhiteSpace($source.license)) {
        throw "territory source provenance is incomplete: $($source.id)"
    }
    $path = Join-Path $workspace $source.path
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $source.sha256) { throw "territory source digest mismatch: $($source.id)" }
}
foreach ($license in @("licenses/Unicode-3.0.txt", "licenses/country-flag-icons-MIT.txt")) {
    if (-not (Test-Path -LiteralPath (Join-Path $workspace $license) -PathType Leaf)) {
        throw "missing territory license: $license"
    }
}

$outputRoot = Join-Path $workspace "out/territory-gate/$([Guid]::NewGuid().ToString('N'))"
& cargo run --quiet --locked -p territory-generator -- $workspace --output-root $outputRoot
if ($LASTEXITCODE -ne 0) { throw "territory regeneration failed" }
$outputs = @(
    "crates/nethop-core/src/generated/territory_registry.rs",
    "crates/nethop-subscription/src/generated/territory_recognition.rs",
    "webui/src/generated/territories.ts"
)
foreach ($relative in $outputs) {
    $expected = Get-Content -LiteralPath (Join-Path $workspace $relative) -Raw
    $actual = Get-Content -LiteralPath (Join-Path $outputRoot $relative) -Raw
    if ($actual -cne $expected) { throw "generated territory output drifted: $relative" }
}
$trackedFlags = Get-ChildItem -LiteralPath (Join-Path $workspace "webui/src/assets/flags") -File -Filter "*.svg"
$generatedFlags = Get-ChildItem -LiteralPath (Join-Path $outputRoot "webui/src/assets/flags") -File -Filter "*.svg"
if ($trackedFlags.Count -ne 249 -or $generatedFlags.Count -ne 249) { throw "territory flag coverage must be exactly 249" }
foreach ($flag in $generatedFlags) {
    $tracked = Join-Path $workspace "webui/src/assets/flags/$($flag.Name)"
    if (-not (Test-Path -LiteralPath $tracked) -or
        (Get-FileHash -LiteralPath $tracked -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $flag.FullName -Algorithm SHA256).Hash) {
        throw "generated flag drifted: $($flag.Name)"
    }
    $svg = Get-Content -LiteralPath $flag.FullName -Raw
    $unsafe = '<script|<foreignObject|\sonload=|\sonclick=|\shref=|xlink:href|url\(http|\ssrc='
    if ($svg.Length -gt 16384 -or $svg -notmatch '^<svg' -or $svg -match $unsafe) {
        throw "unsafe territory flag: $($flag.Name)"
    }
}
Write-Output "territory data gate passed"
