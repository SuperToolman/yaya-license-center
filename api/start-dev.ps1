$ErrorActionPreference = "Stop"

if (-not $env:YAYA_DESKTOP_LAUNCHER) {
  $launcher = Join-Path $PSScriptRoot '..\..\yaya-low-code\web\src-tauri\target\release\yaya-desktop.exe'
  if (Test-Path -LiteralPath $launcher -PathType Leaf) {
    $env:YAYA_DESKTOP_LAUNCHER = (Resolve-Path -LiteralPath $launcher).Path
  }
}

# Docker Desktop CLI is optional in development. When installed, expose it to
# the API's local-Docker integration; otherwise the UI reports it unavailable.
$dockerCli = Join-Path $env:LOCALAPPDATA "Programs\DockerDesktop\resources\bin"
if (Test-Path -LiteralPath (Join-Path $dockerCli "docker.exe")) {
  $env:YAYA_PLATFORM_DOCKER_CLI = Join-Path $dockerCli "docker.exe"
  if (($env:PATH -split [IO.Path]::PathSeparator) -notcontains $dockerCli) {
    $env:PATH = "$dockerCli$([IO.Path]::PathSeparator)$env:PATH"
  }
}

# BuildKit's client-side registry authentication does not read Windows proxy settings.
# Scope the detected proxy to Docker subprocesses, rather than all API HTTP clients.
if ([string]::IsNullOrWhiteSpace($env:YAYA_PLATFORM_DOCKER_HTTPS_PROXY) -and
    [string]::IsNullOrWhiteSpace($env:HTTPS_PROXY)) {
  $systemProxy = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
  if ($systemProxy.ProxyEnable -eq 1 -and $systemProxy.ProxyServer -match '^[a-zA-Z0-9.\-]+:\d+$') {
    $env:YAYA_PLATFORM_DOCKER_HTTPS_PROXY = "http://$($systemProxy.ProxyServer)"
  }
}

# Only import SMS settings, never print credentials or load arbitrary env keys.
$smsEnvFile = Join-Path $PSScriptRoot '..\deploy\.env'
if (Test-Path -LiteralPath $smsEnvFile) {
  foreach ($line in Get-Content -LiteralPath $smsEnvFile) {
    if ($line -match '^\s*(ALIYUN_ACCESS_KEY_ID|ALIYUN_ACCESS_KEY_SECRET|ALIYUN_SMS_SIGN_NAME|ALIYUN_SMS_LOGIN_TEMPLATE_CODE|ALIYUN_SMS_CODE_PARAM|ALIYUN_SMS_REGION)=(.*)$') {
      $smsName = $Matches[1]
      if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($smsName))) {
        Set-Item -Path "Env:$smsName" -Value $Matches[2].Trim().Trim('"').Trim("'")
      }
    }
  }
}

$secretDirectory = Join-Path $PSScriptRoot "..\secrets"
$privateKey = Join-Path $secretDirectory "private.pem"
$publicKey = Join-Path $secretDirectory "public.pem"
$adminPassword = Join-Path $secretDirectory "admin-password.txt"
foreach ($path in @($privateKey, $publicKey)) {
  if (-not (Test-Path $path)) {
    throw "Missing local development secret: $path. Generate the operation center keys first."
  }
}

$env:LICENSE_SIGNING_PRIVATE_KEY_PEM = Get-Content -Raw $privateKey
$env:LICENSE_SIGNING_PUBLIC_KEY_PEM = Get-Content -Raw $publicKey
if ([string]::IsNullOrWhiteSpace($env:YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD) -and (Test-Path $adminPassword)) {
  $env:YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD = (Get-Content -Raw $adminPassword).Trim()
}
if ([string]::IsNullOrWhiteSpace($env:YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME)) {
  $env:YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME = 'admin'
}

cargo run
