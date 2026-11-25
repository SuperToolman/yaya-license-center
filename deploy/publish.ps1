[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)] [string]$ServerIp,
  [string]$SshUser = 'root',
  [securestring]$SshPassword,
  [string]$SshKeyPath,
  [ValidateRange(1, 65535)] [int]$SshPort = 22,
  [ValidateRange(1, 65535)] [int]$WebPort = 8778,
  [ValidateRange(1, 65535)] [int]$ApiPort = 8779,
  [string]$WebOrigin,
  [string]$LicenseApiUrl,
  [string]$DesktopLauncherPath,
  [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9_.-]*$')] [string]$MigrateContainer,
  [ValidatePattern('^/(opt|srv)/[A-Za-z0-9][A-Za-z0-9._/-]*$')]
  [string]$RemoteDir = '/opt/yaya-operation-center-service',
  [switch]$Initialize
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Native {
  param([string]$File, [string[]]$Arguments)
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE." }
}

function Normalize-Origin {
  param([string]$Value, [string]$Name)
  if ([string]::IsNullOrWhiteSpace($Value)) { return '' }
  $uri = $null
  if (-not [Uri]::TryCreate($Value.Trim(), [UriKind]::Absolute, [ref]$uri) -or
      $uri.Scheme -notin @('http', 'https') -or $uri.UserInfo -or
      $uri.AbsolutePath -ne '/' -or $uri.Query -or $uri.Fragment) {
    throw "$Name must be an HTTP(S) origin without credentials, a path, query or fragment."
  }
  $normalized = $uri.GetLeftPart([UriPartial]::Authority)
  if ($normalized -notmatch '^https?://[A-Za-z0-9.\-\[\]:]+$') { throw "$Name contains unsupported host characters; use an IP or ASCII domain." }
  return $normalized
}

$parsedIp = $null
if (-not [Net.IPAddress]::TryParse($ServerIp, [ref]$parsedIp)) { throw 'ServerIp must be an IP address.' }
if ($SshUser -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') { throw 'Invalid SSH user.' }
if ($RemoteDir -match '(^|/)\.{1,2}(/|$)|//|/$') { throw 'RemoteDir must be a canonical application directory.' }
if ($WebPort -eq $ApiPort) { throw 'WebPort and ApiPort must differ.' }
$WebOrigin = Normalize-Origin $WebOrigin 'WebOrigin'
$LicenseApiUrl = Normalize-Origin $LicenseApiUrl 'LicenseApiUrl'
$originHost = if ($parsedIp.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetworkV6) { "[$ServerIp]" } else { $ServerIp }
$defaultOrigin = "http://${originHost}:$WebPort"
$defaultLicenseApi = "http://${originHost}:$ApiPort"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$desktopManifest = Join-Path (Split-Path -Parent $repoRoot) 'yaya-low-code/web/src-tauri/Cargo.toml'
if (-not $DesktopLauncherPath -and (Test-Path -LiteralPath $desktopManifest -PathType Leaf)) {
  Write-Host 'Building the Windows desktop launcher...'
  $desktopTargetDir = Join-Path (Split-Path -Parent $desktopManifest) 'target'
  $desktopBuildArgs = @('build', '--locked', '--release', '--manifest-path', $desktopManifest, '--target-dir', $desktopTargetDir)
  $desktopBinary = 'release/yaya-desktop.exe'
  if ((& rustc -vV) -notcontains 'host: x86_64-pc-windows-msvc') {
    $desktopBuildArgs += @('--target', 'x86_64-pc-windows-msvc')
    $desktopBinary = 'x86_64-pc-windows-msvc/release/yaya-desktop.exe'
  }
  Invoke-Native 'cargo' $desktopBuildArgs
  $DesktopLauncherPath = Join-Path $desktopTargetDir $desktopBinary
}
if ($DesktopLauncherPath) {
  $DesktopLauncherPath = (Resolve-Path -LiteralPath $DesktopLauncherPath).Path
}
$secretsDir = Join-Path $repoRoot 'secrets'
$adminPasswordFile = Join-Path $secretsDir 'admin-password.txt'
if ($Initialize) {
  foreach ($name in @('private.pem', 'public.pem')) {
    if (-not (Test-Path -LiteralPath (Join-Path $secretsDir $name) -PathType Leaf)) { throw "Missing secrets/$name." }
  }
  if (-not (Test-Path -LiteralPath $adminPasswordFile -PathType Leaf)) {
    $adminPasswordFile = Join-Path $secretsDir 'admin-token.txt'
    if (-not (Test-Path -LiteralPath $adminPasswordFile -PathType Leaf)) { throw 'Missing secrets/admin-password.txt.' }
  }
}
foreach ($command in @('ssh.exe', 'scp.exe', 'tar.exe')) {
  if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Required command not found: $command" }
}
$versionMatch = [regex]::Match((Get-Content (Join-Path $repoRoot 'api/Cargo.toml') -Raw), '(?m)^version\s*=\s*"([A-Za-z0-9_.-]+)"')
if (-not $versionMatch.Success) { throw 'Cannot determine API release version.' }
$releaseId = "$($versionMatch.Groups[1].Value)-$([DateTime]::UtcNow.ToString('yyyyMMddHHmmss'))-$([Guid]::NewGuid().ToString('N').Substring(0, 8))"
$archive = Join-Path ([IO.Path]::GetTempPath()) "yaya-operation-center-$releaseId.tar.gz"
$askPass = Join-Path ([IO.Path]::GetTempPath()) "yaya-operation-center-askpass-$PID.cmd"
$stage = "/tmp/yaya-operation-center-$releaseId"
$target = "$SshUser@$ServerIp"
$scpTarget = "$SshUser@$originHost"
$sshOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=4', '-p', "$SshPort")
$scpOptions = @('-o', 'StrictHostKeyChecking=accept-new', '-o', 'ConnectTimeout=15', '-P', "$SshPort")
if ($SshKeyPath) {
  $SshKeyPath = (Resolve-Path -LiteralPath $SshKeyPath).Path
  $sshOptions += @('-i', $SshKeyPath, '-o', 'IdentitiesOnly=yes')
  $scpOptions += @('-i', $SshKeyPath, '-o', 'IdentitiesOnly=yes')
}
$excludes = @(
  '--exclude=.git', '--exclude=node_modules', '--exclude=target', '--exclude=target-filter-fix', '--exclude=.next*',
  '--exclude=test-results*', '--exclude=playwright-report', '--exclude=__pycache__',
  '--exclude=runtime', '--exclude=avatars', '--exclude=platform-packages',
  '--exclude=*.sqlite*', '--exclude=*.log', '--exclude=*.build-worker.lock',
  '--exclude=.platform-environment.key', '--exclude=secrets', '--exclude=.env', '--exclude=.env.*',
  '--exclude=*.pem', '--exclude=*.tsbuildinfo', '--exclude=.patches'
)
$savedEnvironment = @{}
foreach ($name in @('SSH_ASKPASS', 'SSH_ASKPASS_REQUIRE', 'DISPLAY', 'YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD')) {
  $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$passwordBstr = [IntPtr]::Zero
try {
  # Without -SshPassword use normal OpenSSH key/agent/interactive authentication.
  if ($SshPassword) {
    $passwordBstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($SshPassword)
    $env:YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordBstr)
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr)
    $passwordBstr = [IntPtr]::Zero
    # The password is read by PowerShell, never expanded as cmd.exe code.
    $helper = '@echo off' + "`r`n" + 'powershell.exe -NoProfile -NonInteractive -Command "[Console]::Out.WriteLine([Environment]::GetEnvironmentVariable(''YAYA_OPERATION_CENTER_DEPLOY_SSH_PASSWORD''))"' + "`r`n"
    [IO.File]::WriteAllText($askPass, $helper, [Text.Encoding]::ASCII)
    $env:SSH_ASKPASS = $askPass
    $env:SSH_ASKPASS_REQUIRE = 'force'
    $env:DISPLAY = 'yaya-operation-center-publish'
  }
  Write-Host "Packaging native host release $releaseId..."
  $sources = @('api', 'web', 'host-monitor', 'crates', 'deploy', 'pnpm-lock.yaml', 'pnpm-workspace.yaml')
  Invoke-Native 'tar.exe' (@('-czf', $archive) + $excludes + @('-C', $repoRoot) + $sources)
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "umask 077; mkdir '$stage'; mkdir '$stage/source' '$stage/secrets'"))
  Invoke-Native 'scp.exe' ($scpOptions + @($archive, "${scpTarget}:$stage/source.tar.gz"))
  if ($DesktopLauncherPath) {
    Invoke-Native 'scp.exe' ($scpOptions + @($DesktopLauncherPath, "${scpTarget}:$stage/yaya-desktop.exe"))
  }
  if ($Initialize) {
    Invoke-Native 'scp.exe' ($scpOptions + @((Join-Path $secretsDir 'private.pem'), "${scpTarget}:$stage/secrets/private.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @((Join-Path $secretsDir 'public.pem'), "${scpTarget}:$stage/secrets/public.pem"))
    Invoke-Native 'scp.exe' ($scpOptions + @($adminPasswordFile, "${scpTarget}:$stage/secrets/admin-password.txt"))
  }
  $initializeValue = $Initialize.IsPresent.ToString().ToLowerInvariant()
  $remoteScript = @"
set -eu
tar -xzf '$stage/source.tar.gz' -C '$stage/source'
if [ -f /opt/yaya-native-runtime/environment.sh ]; then . /opt/yaya-native-runtime/environment.sh; fi
if [ `"`$(id -u)`" -eq 0 ]; then
  bash '$stage/source/deploy/host/install.sh' '$RemoteDir' '$releaseId' '$WebPort' '$ApiPort' '$WebOrigin' '$LicenseApiUrl' '$defaultOrigin' '$defaultLicenseApi' '$initializeValue' '$stage' '$MigrateContainer'
else
  sudo -n --preserve-env=PATH,RUSTUP_HOME,CARGO_HOME bash '$stage/source/deploy/host/install.sh' '$RemoteDir' '$releaseId' '$WebPort' '$ApiPort' '$WebOrigin' '$LicenseApiUrl' '$defaultOrigin' '$defaultLicenseApi' '$initializeValue' '$stage' '$MigrateContainer'
fi
"@
  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($remoteScript))
  Write-Host 'Building native binaries and installing the systemd services on the host...'
  Invoke-Native 'ssh.exe' ($sshOptions + @($target, "echo $encoded | base64 -d | bash"))
  Write-Host 'Host publication completed. See the URL and service status reported by the installer.'
}
finally {
  if ($passwordBstr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordBstr) }
  foreach ($name in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
  foreach ($path in @($askPass, $archive)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
  }
}
