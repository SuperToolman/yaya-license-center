$ErrorActionPreference = "Stop"

$projectRoot = $PSScriptRoot
$apiDirectory = Join-Path $projectRoot "api"
$webDirectory = Join-Path $projectRoot "web"
$apiScript = Join-Path $apiDirectory "start-dev.ps1"
$apiHealthUrl = "http://127.0.0.1:8779/healthz"

if (-not (Test-Path -LiteralPath $apiScript)) {
  throw "API start script was not found: $apiScript"
}

if (Get-NetTCPConnection -LocalPort 8779 -State Listen -ErrorAction SilentlyContinue) {
  Write-Host "API is already running on port 8779."
} else {
  Write-Host "Starting API..."
  Start-Process -FilePath "powershell.exe" -ArgumentList @(
    "-ExecutionPolicy", "Bypass", "-File", $apiScript
  ) -WorkingDirectory $apiDirectory
}

$apiReady = $false
for ($attempt = 1; $attempt -le 30; $attempt++) {
  try {
    Invoke-WebRequest -UseBasicParsing -Uri $apiHealthUrl -TimeoutSec 1 | Out-Null
    $apiReady = $true
    break
  } catch {
    Start-Sleep -Seconds 1
  }
}

if (-not $apiReady) {
  throw "API did not start within 30 seconds. Check the api logs and local secrets."
}

if (Get-NetTCPConnection -LocalPort 8778 -State Listen -ErrorAction SilentlyContinue) {
  Write-Host "Web is already running at http://127.0.0.1:8778"
  exit 0
}

Write-Host "API is ready. Starting web..."
Start-Process -FilePath "pnpm.cmd" -ArgumentList @("dev") -WorkingDirectory $webDirectory
Write-Host "License Center is starting at http://127.0.0.1:8778"
