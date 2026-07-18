[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$VmName = 'vpn-rdp-windows-test',
    [string]$CheckpointName = 'clean-windows-server-2025-rdp',
    [int]$ReadyTimeoutSeconds = 180
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$vm = Get-VM -Name $VmName -ErrorAction Stop
$checkpoint = Get-VMSnapshot -VM $vm -Name $CheckpointName -ErrorAction Stop
if (-not $PSCmdlet.ShouldProcess($VmName, "restore checkpoint $CheckpointName")) {
    return
}

if ($vm.State -ne 'Off') {
    Stop-VM -VM $vm -TurnOff -Force
}
Restore-VMSnapshot -VMSnapshot $checkpoint -Confirm:$false
Start-VM -VM $vm | Out-Null

$deadline = (Get-Date).AddSeconds($ReadyTimeoutSeconds)
do {
    Start-Sleep -Seconds 2
    $heartbeat = (Get-VMIntegrationService -VMName $VmName -Name Heartbeat).
        PrimaryStatusDescription
    $guestIp = (Get-VMNetworkAdapter -VMName $VmName).IPAddresses |
        Where-Object { $_ -match '^\d+\.' } | Select-Object -First 1
    $rdpReady = $guestIp -and (Test-NetConnection $guestIp -Port 3389 `
        -WarningAction SilentlyContinue).TcpTestSucceeded
} until (($heartbeat -eq 'OK' -and $rdpReady) -or (Get-Date) -ge $deadline)

if ($heartbeat -ne 'OK' -or -not $rdpReady) {
    throw "VM $VmName did not become heartbeat/RDP ready within $ReadyTimeoutSeconds seconds."
}

[pscustomobject]@{
    VmName = $VmName
    Checkpoint = $CheckpointName
    State = (Get-VM -Name $VmName).State.ToString()
    Heartbeat = $heartbeat
    GuestIp = $guestIp
    RdpReady = $rdpReady
}
