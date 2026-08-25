# scripts/deploy/apply.ps1
#
# Windows entrypoint for the sdkwork-api-cloud-gateway deployment toolkit.
# Delegates to bash scripts/deploy/apply.sh inside WSL (default: Ubuntu-22.04).
#
# Usage:
#   pwsh scripts/deploy/apply.ps1
#   pwsh scripts/deploy/apply.ps1 -SkipBuild
#   pwsh scripts/deploy/apply.ps1 -Env development,test,production
#   pwsh scripts/deploy/apply.ps1 -WslPassword Helloworld
#
# Security: do NOT hardcode the WSL password. Pass -WslPassword or set
# $env:SDKWORK_SUDO_PASSWORD / $env:WSL_SUDO_PASSWORD for the call only.

[CmdletBinding()]
param(
  [switch]$SkipBuild,
  [switch]$SkipHost,
  [switch]$NoNginx,
  [string[]]$Env = @(),
  [string]$WslPassword = "",
  [string]$WslDistro = "Ubuntu-22.04",
  [string]$WslUser = "sdkwork"
)

$ErrorActionPreference = 'Stop'

$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..' '..')).Path
$drive = $repoWin.Substring(0, 1).ToLower()
$rest = $repoWin.Substring(3) -replace '\\', '/'
$repoWsl = "/mnt/$drive$rest"

$argsList = @()
if ($SkipBuild) { $argsList += '--skip-build' }
if ($SkipHost)  { $argsList += '--skip-host' }
if ($NoNginx)   { $argsList += '--no-nginx' }
if ($Env.Count -gt 0) {
  $argsList += '--env'
  $argsList += ($Env -join ' ')
}

$applyArgs = $argsList -join ' '
$inner = "cd $repoWsl && bash scripts/deploy/apply.sh $applyArgs"

$password = $WslPassword
if (-not $password -and $env:SDKWORK_SUDO_PASSWORD) { $password = $env:SDKWORK_SUDO_PASSWORD }
if (-not $password -and $env:WSL_SUDO_PASSWORD) { $password = $env:WSL_SUDO_PASSWORD }

if ($password -ne '') {
  $env:SDKWORK_SUDO_PASSWORD = $password
  # Forward the Windows env var into WSL (WSLENV /p = path translation off).
  if ($env:WSLENV) {
    if ($env:WSLENV -notmatch 'SDKWORK_SUDO_PASSWORD') {
      $env:WSLENV = "$($env:WSLENV):SDKWORK_SUDO_PASSWORD"
    }
  } else {
    $env:WSLENV = 'SDKWORK_SUDO_PASSWORD'
  }
}

Write-Host "`n==> delegating to WSL ($WslDistro / $WslUser): $repoWsl" -ForegroundColor Cyan
Write-Host "wsl command: $inner"

wsl -d $WslDistro -u $WslUser -- bash -lc $inner
$exit = $LASTEXITCODE

if ($password -ne '') {
  Remove-Item Env:SDKWORK_SUDO_PASSWORD -ErrorAction SilentlyContinue
}

if ($exit -ne 0) {
  throw "WSL deploy failed with exit code $exit"
}

Write-Host "`nDONE. Health: http://127.0.0.1:3910/healthz (dev) :3911 (test) :3913 (prod)"
