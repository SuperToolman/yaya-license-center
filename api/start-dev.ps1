$ErrorActionPreference = "Stop"

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
