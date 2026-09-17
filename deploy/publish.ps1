[CmdletBinding()]
param(
  [string]$ServerIp,
  [string]$SshUser,
  [securestring]$SshPassword,
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_.-]*$')] [string]$ContainerName,
  [ValidateRange(1, 65535)] [int]$SshPort = 22,
  [ValidateRange(1, 65535)] [int]$WebPort = 8778,
  [ValidateRange(1, 65535)] [int]$ApiPort = 8779,
  [string]$WebOrigin,
  [ValidatePattern('^/[A-Za-z0-9._/-]+$')] [string]$RemoteDir = '/opt/yaya-operation-center-service',
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
if ($PSBoundParameters.ContainsKey('WebOrigin')) {
  $WebOrigin = $WebOrigin.Trim().TrimEnd('/')
  if ($WebOrigin -notmatch '^https?://[A-Za-z0-9.-]+(?::[0-9]{1,5})?$') { throw 'WebOrigin must be an http(s) origin without a path.' }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$secretsDir = Join-Path $repoRoot 'secrets'
$adminPasswordFile = Join-Path $secretsDir 'admin-password.txt'
if ($Initialize) {
  foreach ($file in @('private.pem', 'public.pem')) {
    if (-not (Test-Path -LiteralPath (Join-Path $secretsDir $file))) { throw "-Initialize requires: $secretsDir\\$file" }
  }
  if (-not (Test-Path -LiteralPath $adminPasswordFile)) {
    $legacyAdminTokenFile = Join-Path $secretsDir 'admin-token.txt'
    if (-not (Test-Path -LiteralPath $legacyAdminTokenFile)) { throw "-Initialize requires: $adminPasswordFile" }
    $adminPasswordFile = $legacyAdminTokenFile
    Write-Warning 'Using legacy secrets/admin-token.txt as the initial admin password for this migration.'
  }
}
foreach ($command in @('ssh.exe', 'scp.exe', 'tar.exe')) {
  if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Required command was not found: $command" }
}

$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$archive = Join-Path ([IO.Path]::GetTempPath()) "yaya-operation-center-$stamp-$PID.tar.gz"
$askPass = Join-Path ([IO.Path]::GetTempPath()) "yaya-operation-center-askpass-$PID.cmd"
$remoteArchive = "/tmp/yaya-operation-center-$stamp-$PID.tar.gz"
$target = "$SshUser@$ServerIp"
$composeProjectName = 'yaya-operation-center'
$sshOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-p', "$SshPort")
$scpOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-P', "$SshPort")
$excludes = @('--exclude=.git', '--exclude=node_modules', '--exclude=web/node_modules', '--exclude=web/.next', '--exclude=api/target', '--exclude=api/.yaya-operation-center.sqlite3', '--exclude=api/.license-center.sqlite3', '--exclude=api/*.log', '--exclude=secrets', '--exclude=deploy/.env', '--exclude=deploy/secrets')
$passwordBstr = [IntPtr]::Zero

try {
  $passwordBstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SshPassword)
  $env:YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordBstr)
  [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr)
  $passwordBstr = [IntPtr]::Zero
  [IO.File]::WriteAllText($askPass, "@echo off`r`necho %YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD%`r`n", [Text.Encoding]::ASCII)
  $env:SSH_ASKPASS = $askPass
  $env:SSH_ASKPASS_REQUIRE = 'force'
  $env:DISPLAY = 'yaya-operation-center-publish'

  Write-Host 'Creating source package...'
  Invoke-Native 'tar.exe' (@('-czf', $archive) + $excludes + @('-C', $repoRoot, '.'))
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "mkdir -p '$RemoteDir/deploy/secrets'"))

  if ($Initialize) {
    Write-Host 'Uploading signing keys and initial administrator password...'
    Invoke-Native 'scp.exe' ($scpOptions + @("$secretsDir\\private.pem", "${target}:$RemoteDir/deploy/secrets/private.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @("$secretsDir\\public.pem", "${target}:$RemoteDir/deploy/secrets/public.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @($adminPasswordFile, "${target}:$RemoteDir/deploy/secrets/admin-password.txt"))
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
    "web_origin_host='$ServerIp'",
    "web_origin='$WebOrigin'",
    "initialize='$($Initialize.IsPresent.ToString().ToLowerInvariant())'",
    "compose_project_name='$composeProjectName'",
    'test -f "$remote_dir/deploy/secrets/private.pem" || { echo "Missing signing private key. Run with -Initialize once." >&2; exit 2; }',
    'test -f "$remote_dir/deploy/secrets/public.pem" || { echo "Missing signing public key. Run with -Initialize once." >&2; exit 2; }',
    'test -f "$remote_dir/deploy/secrets/admin-password.txt" || { echo "Missing initial administrator password. Run with -Initialize once." >&2; exit 2; }',
    'existing_project=""',
    'if docker container inspect "$container_name" >/dev/null 2>&1; then existing_service=$(docker inspect -f ''{{ index .Config.Labels "com.docker.compose.service" }}'' "$container_name"); existing_project=$(docker inspect -f ''{{ index .Config.Labels "com.docker.compose.project" }}'' "$container_name"); { [ "$existing_service" = "operation-center" ] || [ "$existing_service" = "license-center" ]; } || { echo "Container name $container_name is already used by $existing_service; refusing to replace it." >&2; exit 4; }; fi',
    'find "$remote_dir" -mindepth 1 -maxdepth 1 ! -name deploy -exec rm -rf -- {} +',
    'find "$remote_dir/deploy" -mindepth 1 -maxdepth 1 ! -name .env ! -name secrets -exec rm -rf -- {} +',
    'tar -xzf "$archive" -C "$remote_dir"',
    'rm -f "$archive"',
    'env_file="$remote_dir/deploy/.env"',
    'if [ ! -f "$env_file" ]; then cp "$remote_dir/deploy/.env.example" "$env_file"; fi',
    'set_env() { key="$1"; value="$2"; temp_file=$(mktemp); awk -v key="$key" -v value="$value" ''BEGIN { found = 0 } index($0, key "=") == 1 { print key "=" value; found = 1; next } { print } END { if (!found) print key "=" value }'' "$env_file" > "$temp_file" && mv "$temp_file" "$env_file"; }',
    'set_env CONTAINER_NAME "$container_name"',
    'set_env WEB_PORT "$web_port"',
    'set_env API_PORT "$api_port"',
    'if [ "$initialize" = true ] && [ -z "$web_origin" ]; then web_origin="http://$web_origin_host:$web_port"; fi',
    'if [ -n "$web_origin" ]; then set_env WEB_ORIGIN "$web_origin"; fi',
    'for host_port in "$web_port" "$api_port"; do owner=$(docker ps --filter "publish=$host_port" --format ''{{.Names}}'' | grep -vx "$container_name" || true); [ -z "$owner" ] || { echo "Host port $host_port is already used by: $owner" >&2; exit 5; }; done',
    'BUILDKIT_PROGRESS=plain docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" build operation-center',
    'if [ -n "$existing_project" ] && [ "$existing_project" != "$compose_project_name" ]; then docker rm -f "$container_name"; fi',
    'docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" up -d --no-build',
    'attempt=0',
    'until docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" exec -T operation-center curl -fsS http://127.0.0.1:8779/healthz >/dev/null; do attempt=$((attempt + 1)); [ "$attempt" -lt 30 ] || { echo "Operation center health check timed out." >&2; docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" logs --tail 150 >&2 || true; exit 3; }; sleep 2; done',
    'docker compose --project-name "$compose_project_name" --project-directory "$remote_dir/deploy" -f "$remote_dir/deploy/compose.yaml" --env-file "$remote_dir/deploy/.env" ps'
  ) -join "`n"
  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($remoteScript))
  Write-Host 'Building and starting Yaya Operation Center...'
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "echo $encoded | base64 -d | sh"))
  Write-Host "Publish completed: http://${ServerIp}:$WebPort"
}
finally {
  if ($passwordBstr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr) }
  Remove-Item Env:YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD -ErrorAction SilentlyContinue
  Remove-Item Env:SSH_ASKPASS -ErrorAction SilentlyContinue
  Remove-Item Env:SSH_ASKPASS_REQUIRE -ErrorAction SilentlyContinue
  Remove-Item Env:DISPLAY -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $askPass -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $archive -Force -ErrorAction SilentlyContinue
}
