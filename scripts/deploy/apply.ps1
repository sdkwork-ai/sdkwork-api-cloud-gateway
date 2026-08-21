# scripts/deploy/apply.ps1
#
# Windows entrypoint for the sdkwork-api-cloud-gateway deployment toolkit.
#
# The gateway's nginx reverse proxy + containers live inside WSL Ubuntu 22.04, so on
# Windows we translate the repository path and delegate to the bash toolkit running in
# the "Ubuntu" WSL distribution. This keeps ONE implementation (bash) and gives Windows
# users a native one-liner — satisfying the multi-OS requirement without duplication.
#
# Usage (run from an elevated PowerShell, or just double-click-free in a normal shell):
#   pwsh scripts/deploy/apply.ps1                       # full cycle, all environments
#   pwsh scripts/deploy/apply.ps1 -SkipBuild             # reuse existing image
#   pwsh scripts/deploy/apply.ps1 -Env development,test  # selected environments
#   pwsh scripts/deploy/apply.ps1 -WslPassword Helloworld # non-interactive sudo for nginx/cert install
#
# Security: do NOT hardcode the WSL password here. Pass it via -WslPassword or set
# $env:WSL_SUDO_PASSWORD beforehand. It is forwarded to the bash script as an environment
# variable only for the duration of the WSL call (not written to disk).

[CmdletBinding()]
param(
  [switch]$SkipBuild,
  [switch]$NoNginx,
  [string[]]$Env = @(),
  [string]$WslPassword = "",
  [string]$WslDistro = "Ubuntu"
)

$ErrorActionPreference = 'Stop'

# Resolve the gateway repo root from this script's location (scripts/deploy -> repo root).
$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..' '..')).Path
# Convert  E:\sdkwork-space\...  ->  /mnt/e/sdkwork-space/...
$drive = $repoWin.Substring(0, 1).ToLower()
$rest = $repoWin.Substring(3) -replace '\\', '/'
$repoWsl = "/mnt/$drive$rest"

$argsList = @()
if ($SkipBuild) { $argsList += '--skip-build' }
if ($NoNginx)   { $argsList += '--no-nginx' }
if ($Env.Count -gt 0) { $argsList += '--env'; $argsList += ($Env -join ' ') }

$applyArgs = $argsList -join ' '
$inner = "cd $repoWsl && bash scripts/deploy/apply.sh $applyArgs"

if ($WslPassword -ne '') {
  $env:SDKWORK_SUDO_PASSWORD = $WslPassword
}

log_step_host() { Write-Host "`n==> $($args -join ' ')" -ForegroundColor Cyan }
log_step_host "delegating to WSL ($WslDistro): $repoWsl"
Write-Host "wsl command: $inner"

# -u ubuntu: run as the configured WSL user (docker and the Node toolchain are on its PATH).
# Root-only steps (certs/nginx) escalate via sudo inside apply.sh, reading
# SDKWORK_SUDO_PASSWORD when provided (non-interactive), else prompting.
wsl -d $WslDistro -u ubuntu -- bash -lc $inner

if ($WslPassword -ne '') { Remove-Item Env:SDKWORK_SUDO_PASSWORD -ErrorAction SilentlyContinue }
Write-Host "`nDONE. On Windows, also bind hosts: powershell -ExecutionPolicy Bypass -File scripts/bind-windows-hosts.ps1"
