param(
    [string] $Token = "e2e-token-0123456789abcdef",
    [int] $Port = 47821
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $root "target\debug\nexdownloadd.exe"
Push-Location $root
try {
    cargo build -p nexdownload-server
    if ($LASTEXITCODE -ne 0) {
        throw "Could not build nexdownloadd."
    }
}
finally {
    Pop-Location
}

$dataDir = Join-Path $env:TEMP ("nexdownload-e2e-" + [guid]::NewGuid().ToString("N"))
$stdout = Join-Path $dataDir "server.stdout.log"
$stderr = Join-Path $dataDir "server.stderr.log"
New-Item -ItemType Directory -Path $dataDir -Force | Out-Null

$base = "http://127.0.0.1:$Port"
$arguments = @(
    "--host", "127.0.0.1",
    "--port", "$Port",
    "--data-dir", $dataDir,
    "--frontend", (Join-Path $root "apps\desktop\dist"),
    "--token", $Token
)

$server = Start-Process -FilePath $binary -ArgumentList $arguments -PassThru `
    -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr

$originalUrl = $env:NEXDOWNLOAD_E2E_URL
$originalApi = $env:NEXDOWNLOAD_E2E_API
$originalToken = $env:NEXDOWNLOAD_E2E_TOKEN

try {
    $ready = $false
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        try {
            $null = Invoke-RestMethod "$base/api/v1/health" -TimeoutSec 2
            $ready = $true
            break
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }
    if (-not $ready) {
        throw "Headless server did not become ready at $base."
    }

    $env:NEXDOWNLOAD_E2E_URL = $base
    $env:NEXDOWNLOAD_E2E_API = $base
    $env:NEXDOWNLOAD_E2E_TOKEN = $Token

    Push-Location $root
    try {
        npm --prefix apps/desktop run test:e2e
        if ($LASTEXITCODE -ne 0) {
            throw "Playwright failed with exit code $LASTEXITCODE."
        }
    }
    finally {
        Pop-Location
    }
}
catch {
    if (Test-Path $stdout) {
        Write-Host "--- server stdout ---"
        Get-Content $stdout
    }
    if (Test-Path $stderr) {
        Write-Host "--- server stderr ---"
        Get-Content $stderr
    }
    throw
}
finally {
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
        $server.WaitForExit(5000) | Out-Null
    }
    $env:NEXDOWNLOAD_E2E_URL = $originalUrl
    $env:NEXDOWNLOAD_E2E_API = $originalApi
    $env:NEXDOWNLOAD_E2E_TOKEN = $originalToken
    Remove-Item $dataDir -Recurse -Force -ErrorAction SilentlyContinue
}
