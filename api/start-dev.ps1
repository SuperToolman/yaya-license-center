$ErrorActionPreference = "Stop"

$secretDirectory = Join-Path $PSScriptRoot "..\secrets"
$privateKey = Join-Path $secretDirectory "private.pem"
$publicKey = Join-Path $secretDirectory "public.pem"
$adminToken = Join-Path $secretDirectory "admin-token.txt"

foreach ($path in @($privateKey, $publicKey, $adminToken)) {
  if (-not (Test-Path $path)) {
    throw "Missing local development secret: $path. Generate the license center keys first."
  }
}

$env:LICENSE_SIGNING_PRIVATE_KEY_PEM = Get-Content -Raw $privateKey
$env:LICENSE_SIGNING_PUBLIC_KEY_PEM = Get-Content -Raw $publicKey
$env:LICENSE_CENTER_ADMIN_TOKEN = (Get-Content -Raw $adminToken).Trim()

cargo run
