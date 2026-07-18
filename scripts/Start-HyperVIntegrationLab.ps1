[CmdletBinding()]
param([int]$PortalPort = 18080, [int]$GuacamolePort = 18081)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$required = @('PORTAL_BEARER_TOKEN', 'PORTAL_USER',
    'GUACAMOLE_JSON_SECRET_KEY', 'POSTGRES_PASSWORD', 'RDP_TARGET_HOST',
    'RDP_CERTIFICATE_SHA256')
foreach ($name in $required) {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
        throw "Required environment variable $name is missing."
    }
}

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    docker compose config --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Compose validation failed.' }

    foreach ($service in @('portal', 'guacamole')) {
        $containers = docker ps -aq `
            --filter label=com.docker.compose.project=vpn-solution `
            --filter "label=com.docker.compose.service=$service"
        if ($containers) { docker rm -f $containers | Out-Null }
    }

    docker compose up -d database guacd
    if ($LASTEXITCODE -ne 0) { throw 'Failed to start database and guacd.' }

    docker compose run -d --no-deps `
        -p "127.0.0.1:${GuacamolePort}:8080" guacamole | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to start Guacamole.' }

    $origin = "http://127.0.0.1:$PortalPort"
    $guacamoleUrl = "http://127.0.0.1:$GuacamolePort/guacamole/"
    docker compose run -d --no-deps `
        -p "127.0.0.1:${PortalPort}:8080" `
        -e "PORTAL_ALLOWED_ORIGIN=$origin" `
        -e "GUACAMOLE_PUBLIC_URL=$guacamoleUrl" portal | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to start the portal.' }

    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        $portalStatus = curl.exe -s -o NUL -w '%{http_code}' "$origin/healthz"
        $guacamoleStatus = curl.exe -s -o NUL -w '%{http_code}' $guacamoleUrl
        if ($portalStatus -eq '200' -and $guacamoleStatus -eq '200') {
            $ready = $true
            break
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $ready) { throw 'Integration lab did not become ready.' }

    [pscustomobject]@{
        Portal = $origin
        Guacamole = $guacamoleUrl
        PortalStatus = [int]$portalStatus
        GuacamoleStatus = [int]$guacamoleStatus
    }
}
finally { Pop-Location }
