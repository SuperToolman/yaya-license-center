$ErrorActionPreference = "Stop"

$projectRoot = $PSScriptRoot
$apiDirectory = Join-Path $projectRoot "api"
$webDirectory = Join-Path $projectRoot "web"
$apiScript = Join-Path $apiDirectory "start-dev.ps1"
$apiHealthUrl = "http://127.0.0.1:8779/healthz"
$apiLogDirectory = Join-Path $apiDirectory "runtime\dev"
$apiLog = Join-Path $apiLogDirectory "api.log"
$apiErrorLog = Join-Path $apiLogDirectory "api-error.log"

function Get-ListeningProcessIds {
  param([int]$Port)

  return @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty OwningProcess -Unique)
}

function Stop-RunningService {
  param(
    [int]$Port,
    [string]$ServiceName
  )

  $processIds = Get-ListeningProcessIds -Port $Port
  foreach ($processId in $processIds) {
    $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if (-not $process) {
      continue
    }
    Write-Host "Stopping existing $ServiceName process tree on port $Port ($($process.ProcessName), PID $processId)..."
    & taskkill.exe /PID $processId /T /F 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
      throw "Failed to stop existing $ServiceName process tree (PID $processId)."
    }
  }

  if ($processIds.Count -eq 0) {
    return
  }

  $deadline = (Get-Date).AddSeconds(10)
  while ((Get-Date) -lt $deadline -and (Get-ListeningProcessIds -Port $Port).Count -gt 0) {
    Start-Sleep -Milliseconds 250
  }
  if ((Get-ListeningProcessIds -Port $Port).Count -gt 0) {
    throw "$ServiceName port $Port is still occupied after stopping its process tree."
  }
}

function Stop-ProcessTree {
  param(
    [int]$ProcessId,
    [string]$ServiceName
  )

  $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
  if (-not $process) {
    return
  }
  Write-Host "Stopping $ServiceName process tree (PID $ProcessId)..."
  & taskkill.exe /PID $ProcessId /T /F 2>$null | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to stop $ServiceName process tree (PID $ProcessId)."
  }
}

if (-not (Test-Path -LiteralPath $apiScript)) {
  throw "API start script was not found: $apiScript"
}

Stop-RunningService -Port 8779 -ServiceName "API"
Stop-RunningService -Port 8778 -ServiceName "Web"

New-Item -ItemType Directory -Force -Path $apiLogDirectory | Out-Null
Remove-Item -LiteralPath $apiLog, $apiErrorLog -Force -ErrorAction SilentlyContinue
Write-Host "Starting API..."
$apiProcess = Start-Process -FilePath "powershell.exe" -ArgumentList @(
  "-ExecutionPolicy", "Bypass", "-File", $apiScript
) -WorkingDirectory $apiDirectory -WindowStyle Hidden -RedirectStandardOutput $apiLog -RedirectStandardError $apiErrorLog -PassThru

$apiReady = $false
for ($attempt = 1; $attempt -le 30; $attempt++) {
  try {
    Invoke-WebRequest -UseBasicParsing -Uri $apiHealthUrl -TimeoutSec 1 | Out-Null
    $apiReady = $true
    break
  } catch {
    if ($apiProcess.HasExited) {
      $details = if (Test-Path -LiteralPath $apiErrorLog) { Get-Content -LiteralPath $apiErrorLog -Raw } else { "" }
      throw "API failed to start. $details"
    }
    Start-Sleep -Seconds 1
  }
}

if (-not $apiReady) {
  throw "API did not start within 30 seconds. Check the api logs and local secrets."
}

Write-Host "API is ready. Starting web..."
try {
  Push-Location $webDirectory
  try {
    & pnpm.cmd dev
    if ($LASTEXITCODE -ne 0) {
      throw "Web exited with code $LASTEXITCODE."
    }
  } finally {
    Pop-Location
  }
} finally {
  Stop-ProcessTree -ProcessId $apiProcess.Id -ServiceName "API"
}
