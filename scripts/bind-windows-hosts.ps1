#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Bind SDKWork API gateway domains to 127.0.0.1 in the Windows hosts file.

.DESCRIPTION
  WSL2 forwards localhost to Windows 127.0.0.1, so nginx on WSL port 80
  is reachable from Windows browsers via these hostnames.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/bind-windows-hosts.ps1
#>
$ErrorActionPreference = 'Stop'

$hostsPath = "$env:SystemRoot\System32\drivers\etc\hosts"
$markerBegin = '# === SDKWORK API GATEWAY DOMAINS BEGIN ==='
$markerEnd = '# === SDKWORK API GATEWAY DOMAINS END ==='

$domainBlock = @(
    $markerBegin
    '# SDKWork API Cloud Gateway (WSL nginx :80 -> container 391x)'
    '127.0.0.1 api-dev.sdkwork.com api-dev.birdcoder.com api-dev.dtupay.com'
    '127.0.0.1 api-test.sdkwork.com api-test.birdcoder.com api-test.dtupay.com'
    '127.0.0.1 api-staging.sdkwork.com api-staging.birdcoder.com api-staging.dtupay.com'
    '127.0.0.1 api.sdkwork.com api.birdcoder.com api.dtupay.com'
    $markerEnd
) -join "`r`n"

$content = Get-Content -Path $hostsPath -Raw -Encoding UTF8
if ($content -match [regex]::Escape($markerBegin)) {
    $pattern = "(?s)$([regex]::Escape($markerBegin)).*?$([regex]::Escape($markerEnd))"
    $content = [regex]::Replace($content, $pattern, $domainBlock)
} else {
    $content = $content.TrimEnd() + "`r`n`r`n" + $domainBlock + "`r`n"
}

Set-Content -Path $hostsPath -Value $content -Encoding UTF8 -NoNewline
Write-Host "Updated Windows hosts: api-dev/test/staging/prod for sdkwork.com, birdcoder.com, dtupay.com"

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
