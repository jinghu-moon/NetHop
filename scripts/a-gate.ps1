$ErrorActionPreference = "Stop"

Set-Location (Join-Path $PSScriptRoot "..")

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    Write-Host ("> {0} {1}" -f $Command, ($Arguments -join " "))
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "command failed with exit code ${LASTEXITCODE}: $Command $($Arguments -join ' ')"
    }
}

Invoke-Checked "cargo" @("fmt", "--all", "--", "--check")
Invoke-Checked "cargo" @("metadata", "--locked", "--format-version", "1")
Invoke-Checked "cargo" @("tree", "--locked", "-e", "normal,features")
Invoke-Checked "cargo" @("test", "--locked", "--test", "b_contracts")
Invoke-Checked "cargo" @("test", "--locked")
Invoke-Checked "cargo" @("test", "--locked", "--no-default-features", "--features", "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json")
Invoke-Checked "cargo" @("test", "--locked", "--no-default-features", "--features", "parser,experimental-formats")
Invoke-Checked "cargo" @("test", "--locked", "--no-default-features", "--features", "parser,format-uri,format-base64,format-clash-yaml,format-singbox-json,fetch")

Write-Host "A gate passed: workspace, test skeleton, feature isolation, and locked dependency combinations are green."
