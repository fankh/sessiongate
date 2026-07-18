[CmdletBinding()]
param(
    [string]$VmName = 'vpn-rdp-windows-test',
    [string]$PortalUrl = 'http://127.0.0.1:18080',
    [string]$GuacamoleUrl = 'http://127.0.0.1:18081/guacamole/',
    [string]$OutputPath = '',
    [switch]$RequireInteractiveDesktop
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Require-Environment([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "Required environment variable $Name is missing."
    }
    $value
}

function Get-FailureStatus([scriptblock]$Request) {
    try { & $Request | Out-Null; 200 }
    catch {
        if ($null -eq $_.Exception.Response) { throw }
        [int]$_.Exception.Response.StatusCode
    }
}

$token = Require-Environment 'PORTAL_BEARER_TOKEN'
$rdpUsername = Require-Environment 'RDP_TEST_USERNAME'
$rdpPassword = Require-Environment 'RDP_TEST_PASSWORD'
$origin = $PortalUrl.TrimEnd('/')
$authorization = @{ Authorization = "Bearer $token" }
$mutation = @{ Authorization = "Bearer $token"; Origin = $origin }
$credential = [pscredential]::new(
    $rdpUsername,
    (ConvertTo-SecureString $rdpPassword -AsPlainText -Force)
)

$vm = Get-VM -Name $VmName
$guestIp = (Get-VMNetworkAdapter -VMName $VmName).IPAddresses |
    Where-Object { $_ -match '^\d+\.' } | Select-Object -First 1
if (-not $guestIp) { throw 'The VM has no IPv4 address.' }

$guest = Invoke-Command -VMName $VmName -Credential $credential -ScriptBlock {
    foreach ($line in (quser.exe 2>&1)) {
        if ($line -notmatch '\bconsole\b' -and $line -match '\s+(\d+)\s+(Active|Disc)\s+') {
            logoff.exe $Matches[1]
        }
    }
    Start-Sleep -Seconds 1
    [pscustomobject]@{
        RdpEnabled = (Get-ItemProperty `
            'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server').
            fDenyTSConnections -eq 0
        NlaRequired = (Get-ItemProperty `
            'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp').
            UserAuthentication -eq 1
        RdpService = (Get-Service TermService).Status.ToString()
        AuthenticationEvents = (Get-WinEvent -FilterHashtable @{
            LogName = 'Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational'
            Id = 1149
            StartTime = (Get-Date).AddMinutes(-10)
        } -ErrorAction SilentlyContinue).Count
    }
}

$health = curl.exe -s -o NUL -w '%{http_code}' "$origin/healthz"
$guacamoleHealth = curl.exe -s -o NUL -w '%{http_code}' $GuacamoleUrl
$anonymous = curl.exe -s -o NUL -w '%{http_code}' `
    "$origin/api/v1/rdp/targets"
$targets = Invoke-RestMethod "$origin/api/v1/rdp/targets" `
    -Headers $authorization
if ($targets.Count -ne 1) { throw 'Expected exactly one effective target.' }
$target = $targets[0]

$defaultDeny = $true
foreach ($field in @('clipboard_to_browser', 'clipboard_to_remote', 'upload',
    'download', 'printing', 'audio_output', 'microphone', 'recording')) {
    if ($target.policy.$field) { $defaultDeny = $false }
}

$validBody = @{
    target_id = $target.id
    rdp_username = $rdpUsername
    rdp_password = $rdpPassword
} | ConvertTo-Json
$noOrigin = Get-FailureStatus {
    Invoke-RestMethod "$origin/api/v1/rdp/sessions" -Method Post `
        -Headers $authorization -ContentType application/json -Body $validBody
}
$emptyCredentials = Get-FailureStatus {
    $body = @{
        target_id = $target.id; rdp_username = ''; rdp_password = ''
    } | ConvertTo-Json
    Invoke-RestMethod "$origin/api/v1/rdp/sessions" -Method Post `
        -Headers $mutation -ContentType application/json -Body $body
}
$unknownTarget = Get-FailureStatus {
    $body = @{
        target_id = '11111111-1111-4111-8111-111111111111'
        rdp_username = $rdpUsername
        rdp_password = $rdpPassword
    } | ConvertTo-Json
    Invoke-RestMethod "$origin/api/v1/rdp/sessions" -Method Post `
        -Headers $mutation -ContentType application/json -Body $body
}

$launch = Invoke-RestMethod "$origin/api/v1/rdp/sessions" -Method Post `
    -Headers $mutation -ContentType application/json -Body $validBody
$since = (Get-Date).ToUniversalTime().ToString('o')
$profile = Join-Path $env:TEMP "vpn-rdp-integration-$([guid]::NewGuid())"
$debugPort = Get-Random -Minimum 20000 -Maximum 30000
$edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
if (-not (Test-Path -LiteralPath $edge)) { throw 'Microsoft Edge is missing.' }

$stopwatch = [Diagnostics.Stopwatch]::StartNew()
Start-Process $edge -ArgumentList @(
    '--no-first-run', "--user-data-dir=$profile", '--new-window',
    "--remote-debugging-port=$debugPort",
    '--app=about:blank'
) | Out-Null

$debugPage = $null
for ($attempt = 0; $attempt -lt 120; $attempt++) {
    try {
        $pages = Invoke-RestMethod "http://127.0.0.1:$debugPort/json/list"
        $debugPage = @($pages) | Where-Object {
            $_.type -eq 'page' -and $_.url -ne 'edge://permission-request-dialog/'
        } | Select-Object -First 1
        if ($debugPage) { break }
    }
    catch { Start-Sleep -Milliseconds 250 }
}
if (-not $debugPage) {
    Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" |
        Where-Object CommandLine -like "*$profile*" |
        ForEach-Object {
            Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
        }
    throw 'Unable to attach to the Edge test page.'
}
$cdp = [System.Net.WebSockets.ClientWebSocket]::new()
$cdp.ConnectAsync(
    [uri]$debugPage.webSocketDebuggerUrl,
    [Threading.CancellationToken]::None
).GetAwaiter().GetResult()

function Send-Cdp([int]$Id, [string]$Method, [hashtable]$Parameters = $null) {
    $payload = [ordered]@{ id = $Id; method = $Method }
    if ($Parameters) { $payload.params = $Parameters }
    $message = [Text.Encoding]::UTF8.GetBytes(
        ($payload | ConvertTo-Json -Depth 5 -Compress)
    )
    $segment = [ArraySegment[byte]]::new($message)
    $cdp.SendAsync(
        $segment,
        [System.Net.WebSockets.WebSocketMessageType]::Text,
        $true,
        [Threading.CancellationToken]::None
    ).GetAwaiter().GetResult()
}
Send-Cdp 1 'Browser.grantPermissions' @{
    origin = "{0}://{1}" -f ([uri]$GuacamoleUrl).Scheme,
        ([uri]$GuacamoleUrl).Authority
    permissions = @('clipboardReadWrite', 'clipboardSanitizedWrite')
}
Send-Cdp 2 'Page.bringToFront'
Send-Cdp 3 'Page.navigate' @{ url = $launch.guacamole_url }

$rdpLoggedOn = $false
$browserJoined = $false
$logs = ''
for ($attempt = 0; $attempt -lt 240; $attempt++) {
    Start-Sleep -Milliseconds 250
    if ($attempt % 20 -eq 0) { Send-Cdp (4 + $attempt) 'Page.bringToFront' }
    $oldPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $logs = docker logs vpn-solution-guacd-1 --since $since 2>&1 |
        Out-String -Width 4096
    $ErrorActionPreference = $oldPreference
    if ($logs -match 'joined connection') {
        $browserJoined = $true
        if (-not $RequireInteractiveDesktop) { break }
    }
    if ($logs -match 'RDPDR user logged on') { $rdpLoggedOn = $true; break }
    if ($logs -match 'Security negotiation failed|Certificate validation failed') {
        break
    }
}
$loginMilliseconds = $stopwatch.Elapsed.TotalMilliseconds
$cdp.Dispose()

$authenticationEvents = Invoke-Command -VMName $VmName `
    -Credential $credential -ScriptBlock {
        (Get-WinEvent -FilterHashtable @{
            LogName = 'Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational'
            Id = 1149
            StartTime = (Get-Date).AddMinutes(-3)
        } -ErrorAction SilentlyContinue).Count
    }

Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" |
    Where-Object CommandLine -like "*$profile*" |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
if (Test-Path -LiteralPath $profile) {
    Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
}

$database = docker exec vpn-solution-database-1 psql -U vpn -d vpn -Atc `
    "SELECT json_build_object('sessions',(SELECT count(*) FROM rdp_sessions),'allowed',(SELECT count(*) FROM rdp_audit_events WHERE outcome='allowed'),'denied',(SELECT count(*) FROM rdp_audit_events WHERE outcome='denied'));" |
    ConvertFrom-Json

$checks = [ordered]@{
    VmRunning = $vm.State -eq 'Running'
    VmHeartbeat = (Get-VMIntegrationService -VMName $VmName -Name Heartbeat).
        PrimaryStatusDescription -eq 'OK'
    RdpReachable = (Test-NetConnection $guestIp -Port 3389 `
        -WarningAction SilentlyContinue).TcpTestSucceeded
    GuestRdpEnabled = $guest.RdpEnabled
    GuestNlaRequired = $guest.NlaRequired
    GuestRdpService = $guest.RdpService -eq 'Running'
    PortalHealthy = $health -eq '200'
    GuacamoleHealthy = $guacamoleHealth -eq '200'
    AnonymousDenied = $anonymous -eq '401'
    MissingOriginDenied = $noOrigin -eq 403
    EmptyCredentialsDenied = $emptyCredentials -eq 400
    UnknownTargetDenied = $unknownTarget -eq 403
    DefaultDenyPolicy = $defaultDeny
    GuacdNla = $logs -match 'Security mode: NLA'
    CertificateAccepted = $logs -notmatch 'Certificate validation failed'
    SecurityNegotiationPassed = $logs -notmatch 'Security negotiation failed'
    BrowserTunnelJoined = $browserJoined
    SessionPersisted = [int]$database.sessions -gt 0
    AllowedAuditPersisted = [int]$database.allowed -gt 0
    DeniedAuditPersisted = [int]$database.denied -gt 0
}
if ($RequireInteractiveDesktop) {
    $checks.RdpUserLoggedOn = $rdpLoggedOn
    $checks.WindowsAuthenticated = [int]$authenticationEvents -gt `
        [int]$guest.AuthenticationEvents
}
$failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value } |
    ForEach-Object Key)
$result = [pscustomobject]@{
    Timestamp = (Get-Date).ToString('o')
    Passed = $failed.Count -eq 0
    FailedChecks = $failed
    Checks = $checks
    Metrics = @{
        BrowserTunnelMilliseconds = [math]::Round($loginMilliseconds, 1)
        InteractiveDesktopObserved = $rdpLoggedOn
        NewWindowsAuthenticationObserved = [int]$authenticationEvents -gt `
            [int]$guest.AuthenticationEvents
    }
    Target = @{ Id = $target.id; Name = $target.name; GuestIp = $guestIp }
}

if ($OutputPath) {
    $parent = Split-Path -Parent $OutputPath
    if ($parent) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
    $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath
}
$result
if ($failed.Count -gt 0) { throw "Integration test failed: $($failed -join ', ')" }
