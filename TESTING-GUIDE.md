# SessionGate Testing Guide

This guide validates the SessionGate browser RDP control plane, Apache Guacamole gateway,
PostgreSQL persistence, and a Windows guest running on Hyper-V. It separates
component tests from a successful interactive desktop test so partial results
are not reported as end-to-end success.

## Test environment

The current local lab uses:

- Hyper-V VM: `vpn-rdp-windows-test`
- Guest OS: Windows Server 2025 Standard Evaluation (Desktop Experience)
- Guest name: `RDPTEST`
- Guest IPv4 address at the time of writing: `172.31.98.16`
- RDP port: `3389`
- NLA: required
- Portal: `http://127.0.0.1:18080`
- Guacamole: `http://127.0.0.1:18081/guacamole/`

Treat the address as dynamic. Always query Hyper-V before running a test.

## Security rules

- Use only an isolated test VM and temporary credentials.
- Never commit `.env`, passwords, ISO files, VHDX files, launch assertions, or
  database volumes.
- Do not disable NLA or certificate validation to make a test pass.
- Do not publish portal, Guacamole, PostgreSQL, guacd, or RDP ports beyond the
  intended lab interfaces.
- Remove unattended-install media and cached credentials after installation.
- A successful TCP connection is not evidence of a successful RDP login.

## 1. Verify the Windows VM

Run from an elevated PowerShell session:

```powershell
$name = 'vpn-rdp-windows-test'
$vm = Get-VM -Name $name
$adapter = Get-VMNetworkAdapter -VMName $name
$ip = $adapter.IPAddresses |
  Where-Object { $_ -match '^\d+\.' } |
  Select-Object -First 1

[pscustomobject]@{
  State = $vm.State
  Status = $vm.Status
  Heartbeat = (Get-VMIntegrationService -VMName $name -Name Heartbeat).
    PrimaryStatusDescription
  IPv4 = $ip
  RdpReachable = (Test-NetConnection $ip -Port 3389).
    TcpTestSucceeded
}
```

Expected result:

- VM state is `Running`.
- Status is `Operating normally`.
- Heartbeat is `OK`.
- An IPv4 address is present.
- Port 3389 is reachable.

## 2. Verify Windows RDP policy

Use PowerShell Direct so this test does not depend on guest networking:

```powershell
$credential = Get-Credential 'RDPTEST\Administrator'

Invoke-Command -VMName vpn-rdp-windows-test -Credential $credential {
  $terminalServer = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'
  $rdpTcp = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp'

  [pscustomobject]@{
    RdpEnabled = $terminalServer.fDenyTSConnections -eq 0
    NlaRequired = $rdpTcp.UserAuthentication -eq 1
    Service = (Get-Service TermService).Status
    FirewallRules = (Get-NetFirewallRule -DisplayGroup 'Remote Desktop' |
      Where-Object Enabled -eq True).Count
  }
}
```

All four checks must pass. Record the RDP certificate fingerprint separately:

```powershell
Invoke-Command -VMName vpn-rdp-windows-test -Credential $credential {
  $certificate = Get-ChildItem 'Cert:\LocalMachine\Remote Desktop' |
    Select-Object -First 1
  $sha256 = [Security.Cryptography.SHA256]::Create()
  try {
    ($sha256.ComputeHash($certificate.RawData) |
      ForEach-Object ToString x2) -join ''
  } finally {
    $sha256.Dispose()
  }
}
```

## 3. Configure the container lab

Copy the example environment file and replace every placeholder:

```powershell
Copy-Item .env.example .env
```

Required values include:

```dotenv
PORTAL_BEARER_TOKEN=<at-least-32-random-characters>
PORTAL_USER=rdp-test-user
PORTAL_ALLOWED_ORIGIN=http://127.0.0.1:18080
GUACAMOLE_JSON_SECRET_KEY=<exactly-32-hexadecimal-digits>
POSTGRES_PASSWORD=<long-random-password>
RDP_TARGET_HOST=<current-guest-ipv4>
RDP_CERTIFICATE_SHA256=<current-guest-certificate-sha256>
RDP_DOMAIN=RDPTEST
```

The checked-in Compose ports are 8080 and 8081. If those ports are occupied,
use a local uncommitted override or `docker compose run` with 18080 and 18081.
The configured `PORTAL_ALLOWED_ORIGIN` and `GUACAMOLE_PUBLIC_URL` must match the
published test ports exactly.

## 4. Static and unit validation

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
docker compose config --quiet
docker compose build portal
```

Expected result: every command exits with status zero. The current unit suite
covers default-deny redirection, independent directional controls, target
validation, assertion lifetimes, encryption, and HMAC integrity.

## 5. Start and inspect containers

```powershell
docker compose up -d database guacd guacamole portal
docker compose ps
```

Verify both HTTP services:

```powershell
curl.exe -s -o NUL -w '%{http_code}' http://127.0.0.1:8080/healthz
curl.exe -s -o NUL -w '%{http_code}' http://127.0.0.1:8081/guacamole/
```

Both should return `200`. The portal health endpoint also checks PostgreSQL.

## 6. Test authentication and origin enforcement

Set the token only in the current PowerShell process:

```powershell
$token = '<PORTAL_BEARER_TOKEN>'
$authorization = @{ Authorization = "Bearer $token" }
$mutation = @{
  Authorization = "Bearer $token"
  Origin = 'http://127.0.0.1:8080'
}
```

Verify that an anonymous inventory request returns `401`:

```powershell
curl.exe -s -o NUL -w '%{http_code}' http://127.0.0.1:8080/api/v1/rdp/targets
```

Verify that a valid token returns the bound target:

```powershell
$targets = Invoke-RestMethod http://127.0.0.1:8080/api/v1/rdp/targets -Headers $authorization
$targets | ConvertTo-Json -Depth 5
```

Use a launch body containing `target_id`, `rdp_username`, and `rdp_password`.
Send it without `Origin`; it must return `403`. Send the same body with
`$mutation`; it should return `201` and a Guacamole URL containing a short-lived
encrypted `data` parameter. Verify that neither credentials nor the assertion
appear in application logs or PostgreSQL.

## 7. Verify default-deny policy

Inspect the target returned by the API. The bootstrapped policy must report
`false` for:

- `clipboard_to_browser`
- `clipboard_to_remote`
- `upload`
- `download`
- `printing`
- `audio_output`
- `microphone`
- `recording`

Create a disabled target through the administration API and verify it does not
appear in the user inventory. Attempt to launch a random target UUID and verify
the result is `403` with `access denied by default`.

## 8. Verify persistence and audit records

```powershell
docker compose exec database psql -U vpn -d vpn -c @'
SELECT id, user_id, target_id, state, source_ip, created_at
FROM rdp_sessions
ORDER BY created_at DESC
LIMIT 10;

SELECT actor_id, action, outcome, target_id, session_id, details, created_at
FROM rdp_audit_events
ORDER BY created_at DESC
LIMIT 20;
'@
```

Confirm that allowed launches create both a session and an `allowed` audit
event. Unknown targets must create a `denied` audit event without creating an
active session.

## 9. Browser and Guacamole test

Open the portal, enter the bearer token, load targets, and select **Connect
securely**. Inspect guacd concurrently:

```powershell
docker compose logs --follow --tail 100 guacd
```

A full pass requires all of the following:

1. Guacamole accepts the encrypted JSON assertion.
2. The browser creates a Guacamole tunnel.
3. guacd selects `Security mode: NLA`.
4. The pinned certificate is accepted.
5. Windows records a successful authentication event 1149.
6. A usable Windows desktop is displayed.
7. Clipboard, drive, printing, audio, and microphone behavior matches policy.

The 2026-07-18 post-fix run passed steps 1 through 6 after providing an
ephemeral writable FreeRDP home, formatting the certificate pin correctly, and
adding the lab credential handoff. Step 7 still requires explicit behavioral
testing for each enabled and disabled redirection.

## 10. Verify Windows authentication events

```powershell
Invoke-Command -VMName vpn-rdp-windows-test -Credential $credential {
  Get-WinEvent -FilterHashtable @{
    LogName = 'Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational'
    StartTime = (Get-Date).AddMinutes(-10)
  } | Select-Object TimeCreated, Id, Message
}
```

Event 261 proves only that the listener received a connection. Event 1149 is
required to prove successful user authentication.

## 11. Performance test after desktop access works

Do not benchmark the current failed negotiation path. After a desktop session
works, record:

- Portal launch API latency, p50/p95/p99
- Time from launch click to first desktop frame
- guacd CPU and memory during idle, video playback, and window movement
- Browser CPU, memory, and received bytes
- Network latency and throughput between guacd and the VM
- Five concurrent-session resource use
- Reconnect and session-expiry behavior

Container measurements:

```powershell
docker stats --no-stream
docker compose logs --since 10m portal guacamole guacd
```

Store measurements with the date, host specification, VM specification,
container image versions, policy, test duration, and pass/fail conclusion.

The repeatable harness runs the deterministic browser-tunnel gate by default:

```powershell
.\scripts\Reset-HyperVIntegrationVm.ps1
.\scripts\Start-HyperVIntegrationLab.ps1
.\scripts\Test-HyperVBrowserRdp.ps1 -OutputPath .\reports\integration-latest.json
```

On an interactive Windows desktop, add `-RequireInteractiveDesktop` to require
a fresh Windows event 1149 and `RDPDR user logged on`. The strict mode exits
nonzero if browser permission UI or desktop-session state blocks automation.

## 12. Cleanup

Remove container test state:

```powershell
docker compose down --volumes --remove-orphans
```

Stop the VM without deleting it:

```powershell
Stop-VM -Name vpn-rdp-windows-test
```

The clean baseline checkpoint is named `clean-windows-server-2025-rdp`.
Restoring it discards guest changes after the checkpoint and should therefore
be performed only when that data loss is intended.

## Pass criteria

The solution is end-to-end functional only when every section through browser
policy enforcement passes. Container health, successful JSON authentication,
an open TCP port, or an RDP listener event alone are partial results.
