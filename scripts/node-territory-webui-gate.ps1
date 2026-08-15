[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$workspace = Split-Path -Parent $PSScriptRoot
$webui = Join-Path $workspace "webui"
Push-Location $webui
try {
    npm run typecheck
    if ($LASTEXITCODE -ne 0) { throw "WebUI typecheck failed" }
    npm run test:unit
    if ($LASTEXITCODE -ne 0) { throw "WebUI model contracts failed" }
    npm run test:browser
    if ($LASTEXITCODE -ne 0) { throw "WebUI node component contracts failed" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "WebUI build failed" }
    npm run check:security
    if ($LASTEXITCODE -ne 0) { throw "WebUI security contracts failed" }
    npm run check:bundle
    if ($LASTEXITCODE -ne 0) { throw "WebUI bundle contracts failed" }
    npm run report:release
    if ($LASTEXITCODE -ne 0) { throw "WebUI release artifacts failed" }
}
finally { Pop-Location }

$sbom = Get-Content -LiteralPath (Join-Path $workspace "artifacts/webui/webui-sbom.cdx.json") -Raw | ConvertFrom-Json
$vendored = @($sbom.components | Where-Object { @($_.properties | Where-Object { $_.name -eq "nethop:vendored" -and $_.value -eq "true" }).Count -eq 1 })
if ($vendored.Count -ne 2 -or $vendored.name -notcontains "country-flag-icons" -or $vendored.name -notcontains "Unicode CLDR territory data") {
    throw "territory SBOM components are incomplete"
}

$flags = Get-ChildItem -LiteralPath (Join-Path $workspace "webui/src/assets/flags") -File -Filter "*.svg"
$archive = Join-Path $workspace "out/territory-gate/flags-$([Guid]::NewGuid().ToString('N')).zip"
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $archive) | Out-Null
Compress-Archive -LiteralPath $flags.FullName -DestinationPath $archive -CompressionLevel Optimal
$zipBytes = (Get-Item -LiteralPath $archive).Length
if ($flags.Count -ne 249 -or $zipBytes -gt 102400) { throw "flag asset budget exceeded: $zipBytes bytes" }
$nodeChunks = @(Get-ChildItem -LiteralPath (Join-Path $workspace "module/webroot/assets") -File -Filter "NodesView-*.js")
if ($nodeChunks.Count -ne 1) { throw "expected exactly one NodesView chunk" }
$nodeChunk = $nodeChunks[0]
$nodeSource = Get-Content -LiteralPath $nodeChunk.FullName -Raw
if ($nodeSource -match 'data:image/svg\+xml') { throw "flag SVGs were inlined into the NodesView chunk" }
Write-Output "node territory WebUI gate passed: flags_zip_bytes=$zipBytes"
