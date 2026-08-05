[CmdletBinding()]
param(
  [string]$ServerIp,
  [string]$SshUser,
  [securestring]$SshPassword,
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_.-]*$')] [string]$ContainerName,
  [ValidateRange(1, 65535)] [int]$SshPort = 22,
  [ValidateRange(1, 65535)] [int]$WebPort = 8778,
  [ValidateRange(1, 65535)] [int]$ApiPort = 8779,
  [ValidatePattern('^/[A-Za-z0-9._/-]+$')] [string]$RemoteDir = '/opt/yaya-license-center-service',
  [switch]$Initialize
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Native {
  param([string]$File, [string[]]$Arguments)
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE." }
}

function Read-Required {
  param([string]$Value, [string]$Prompt)
  if ([string]::IsNullOrWhiteSpace($Value)) { return (Read-Host $Prompt).Trim() }
  return $Value.Trim()
}

$ServerIp = Read-Required $ServerIp 'Server IP'
$SshUser = Read-Required $SshUser 'SSH user'
$ContainerName = Read-Required $ContainerName 'Container name'
if (-not $SshPassword) { $SshPassword = Read-Host 'SSH password' -AsSecureString }

$parsedIp = $null
if (-not [Net.IPAddress]::TryParse($ServerIp, [ref]$parsedIp)) { throw 'ServerIp must be an IP address.' }
if ($SshUser -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') { throw 'SshUser contains unsupported characters.' }
if ($ContainerName -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') { throw 'ContainerName contains unsupported characters.' }

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$secretsDir = Join-Path $repoRoot 'secrets'
if ($Initialize) {
  foreach ($file in @('private.pem', 'public.pem', 'admin-token.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $secretsDir $file))) { throw "-Initialize requires: $secretsDir\\$file" }
  }
}
foreach ($command in @('ssh.exe', 'scp.exe', 'tar.exe')) {
  if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Required command was not found: $command" }
}

$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$archive = Join-Path ([IO.Path]::GetTempPath()) "yaya-license-center-$stamp-$PID.tar.gz"
$askPass = Join-Path ([IO.Path]::GetTempPath()) "yaya-license-center-askpass-$PID.cmd"
$remoteArchive = "/tmp/yaya-license-center-$stamp-$PID.tar.gz"
$target = "$SshUser@$ServerIp"
$composeProjectName = 'yaya-license-center'
$sshOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-p', "$SshPort")
$scpOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-P', "$SshPort")
$excludes = @('--exclude=.git', '--exclude=node_modules', '--exclude=web/node_modules', '--exclude=web/.next', '--exclude=api/target', '--exclude=api/.license-center.sqlite3', '--exclude=api/*.log', '--exclude=secrets', '--exclude=deploy/.env', '--exclude=deploy/secrets')
$passwordBstr = [IntPtr]::Zero

try {
  $passwordBstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SshPassword)
  $env:YAYA_LICENSE_CENTER_DEPLOY_SSH_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordBstr)
  [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr)
  $passwordBstr = [IntPtr]::Zero
  [IO.File]::WriteAllText($askPass, "@echo off`r`necho %YAYA_LICENSE_CENTER_DEPLOY_SSH_PASSWORD%`r`n", [Text.Encoding]::ASCII)
  $env:SSH_ASKPASS = $askPass
  $env:SSH_ASKPASS_REQUIRE = 'force'
  $env:DISPLAY = 'yaya-license-center-publish'

  Write-Host 'Creating source package...'
  Invoke-Native 'tar.exe' (@('-czf', $archive) + $excludes + @('-C', $repoRoot, '.'))
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "mkdir -p '$RemoteDir/deploy/secrets'"))

  if ($Initialize) {
    Write-Host 'Uploading signing keys and administrator token...'
    Invoke-Native 'scp.exe' ($scpOptions + @("$secretsDir\\private.pem", "${target}:$RemoteDir/deploy/secrets/private.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @("$secretsDir\\public.pem", "${target}:$RemoteDir/deploy/secrets/public.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @("$secretsDir\\admin-token.txt", "${target}:$RemoteDir/deploy/secrets/admin-token.txt"))
  }

  Write-Host 'Uploading source package...'
  Invoke-Native 'scp.exe' ($scpOptions + @($archive, "${target}:$remoteArchive"))
  $remoteScript = @(
    'set -eu',
    "remote_dir='$RemoteDir'",
    "archive='$remoteArchive'",
    "container_name='$ContainerName'",
    "web_port='$WebPort'",
    "api_port='$ApiPort'",
    "compose_project_name='$composeProjectName'",
    'test -f "$remote_dir/deploy/secrets/private.pem" || { echo "Missing signing private key. Run with -Initialize once." >&2; exit 2; }',
    'test -f "$remote_dir/deploy/secrets/public.pem" || { echo "Missing signing public key. Run with -Initialize once." >&2; exit 2; }',
    'test -f "$remote_dir/deploy/secrets/admin-token.txt" || { echo "Missing administrator token. Run with -Initialize once." >&2; exit 2; }',
    'printf "CONTAINER_NAME=%s\\nWEB_PORT=%s\\nAPI_PORT=%s\\n" "$container_name" "$web_port" "$api_port" > "$remote_dir/deploy/.env"',
    'if docker container inspect "$container_name" >/dev/null 2>&1; then existing_service=$(docker inspect -f ''{{ index .Config.Labels "com.docker.compose.service" }}'' "$container_name"); existing_project=$(docker inspect -f ''{{ index .Config.Labels "com.docker.compose.project" }}'' "$container_name"); [ "$existing_service" = "license-center" ] || { echo "Container name $container_name is already used by $existing_service; refusing to replace it." >&2; exit 4; }; if [ "$existing_project" != "$compose_project_name" ]; then docker rm -f "$container_name"; fi; fi',
    'tar -xzf "$archive" -C "$remote_dir"',
    'rm -f "$archive"',
    'BUILDKIT_PROGRESS=plain docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" up -d --build',
    'attempt=0',
    'until docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" exec -T license-center curl -fsS http://127.0.0.1:8779/healthz >/dev/null; do attempt=$((attempt + 1)); [ "$attempt" -lt 30 ] || { echo "License center health check timed out." >&2; exit 3; }; sleep 2; done',
    'docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" ps'
  ) -join "`n"
  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($remoteScript))
  Write-Host 'Building and starting license center...'
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "echo $encoded | base64 -d | sh"))
  Write-Host "Publish completed: http://${ServerIp}:$WebPort"
}
finally {
  if ($passwordBstr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr) }
  Remove-Item Env:YAYA_LICENSE_CENTER_DEPLOY_SSH_PASSWORD -ErrorAction SilentlyContinue
  Remove-Item Env:SSH_ASKPASS -ErrorAction SilentlyContinue
  Remove-Item Env:SSH_ASKPASS_REQUIRE -ErrorAction SilentlyContinue
  Remove-Item Env:DISPLAY -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $askPass -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
}
