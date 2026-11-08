$ErrorActionPreference = "Stop"

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock] $Action,
        [Parameter(Mandatory = $true)]
        [string] $Name
    )
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Invoke-Step { cargo fmt --all -- --check } "Rust format check"
    Invoke-Step { cargo clippy --all-targets -- -D warnings } "Clippy"
    Invoke-Step { cargo test } "Rust tests"
    Invoke-Step { npm --prefix apps/desktop run build } "Frontend build"
    Invoke-Step { npm --prefix apps/desktop test } "Frontend tests"
    Invoke-Step { cargo check -p nexdownload-desktop } "Tauri desktop check"
    Write-Host "All checks passed." -ForegroundColor Green
}
finally {
    Pop-Location
}
