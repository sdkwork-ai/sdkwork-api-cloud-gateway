#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Bind SDKWork API gateway domains to 127.0.0.1 in the Windows hosts file.

.DESCRIPTION
  WSL2 forwards localhost to Windows 127.0.0.1, so the WSL-hosted edge (the
  sdkwork-webserver reverse proxy on port 80) is reachable from Windows
  browsers via these hostnames.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File bin/host/bind-windows-hosts.ps1
#>
$ErrorActionPreference = 'Stop'

# Registered public base-domain family (ENVIRONMENT_SPEC §5.1.0.1).
# MUST stay aligned with the canonical deployment registry
# (scripts/deployment-environments.mjs ENVIRONMENTS + the expectedHosts matrix
# asserted in scripts/deployment-contract.test.mjs) and with
# etc/sdkwork.deployment.config.json cloudApiBaseUrl.
$baseDomains = @(
    'sdkwork.com', 'birdcoder.com', 'dtupay.com', 'noaper.com',
    'sdkwork.cn', 'birdcoder.cn', 'dtupay.cn', 'noaper.cn',
    'skubc.com', 'skubc.cn',
    'zowalk.com', 'zowalk.cn',
    'offer86.com', 'offer86.cn',
    '86offer.com', '86offer.cn'
)

$hostsPath = "$env:SystemRoot\System32\drivers\etc\hosts"
$markerBegin = '# === SDKWORK API GATEWAY DOMAINS BEGIN ==='
$markerEnd = '# === SDKWORK API GATEWAY DOMAINS END ==='

# api-<suffix>.<base-domain> per environment; production uses the bare "api".
$envSuffixes = @('api-dev', 'api-test', 'api-staging', 'api-demo', 'api')

$domainBlock = @( $markerBegin )
$domainBlock += '# SDKWork API Cloud Gateway (WSL webserver edge :80 -> container 391x)'
foreach ($suffix in $envSuffixes) {
    $names = foreach ($d in $baseDomains) { "$suffix.$d" }
    $domainBlock += "127.0.0.1 $names"
}
$domainBlock += $markerEnd
$domainBlock = $domainBlock -join "`r`n"

$content = Get-Content -Path $hostsPath -Raw -Encoding UTF8
if ($content -match [regex]::Escape($markerBegin)) {
    $pattern = "(?s)$([regex]::Escape($markerBegin)).*?$([regex]::Escape($markerEnd))"
    $content = [regex]::Replace($content, $pattern, $domainBlock)
} else {
    $content = $content.TrimEnd() + "`r`n`r`n" + $domainBlock + "`r`n"
}

Set-Content -Path $hostsPath -Value $content -Encoding UTF8 -NoNewline
Write-Host "Updated Windows hosts: api-dev/test/staging/demo/prod for $($baseDomains.Count) registered base domains"

$checks = @(
    'http://api-dev.sdkwork.com/healthz',
    'http://api-test.birdcoder.com/healthz',
    'http://api-staging.dtupay.com/healthz'
)
foreach ($url in $checks) {
    try {
        $resp = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 10
        Write-Host "OK $url -> $($resp.StatusCode)"
    } catch {
        Write-Warning "FAIL $url -> $($_.Exception.Message)"
    }
}
