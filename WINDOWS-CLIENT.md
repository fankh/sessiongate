# SessionGate Windows Client Implementation Guide

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────┐
│              Windows Client Application          │
│                                                  │
│  ┌──────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ System   │  │  Enrollment  │  │  Update   │ │
│  │ Tray UI  │  │  & Config    │  │  Module   │ │
│  │ (Tauri)  │  │  Manager     │  │           │ │
│  └────┬─────┘  └──────┬───────┘  └─────┬─────┘ │
│       │               │                │        │
│  ┌────┴───────────────┴────────────────┴─────┐ │
│  │           VPN Service (NT Service)         │ │
│  │                                            │ │
│  │  ┌────────────┐  ┌──────────────────────┐ │ │
│  │  │ BoringTun  │  │   Network Manager    │ │ │
│  │  │ (WireGuard │  │  ┌────────────────┐  │ │ │
│  │  │  Protocol) │  │  │ Split Tunnel   │  │ │ │
│  │  │            │  │  │ Kill Switch    │  │ │ │
│  │  │ Noise IK   │  │  │ DNS Manager   │  │ │ │
│  │  │ ChaCha20   │  │  │ Route Table   │  │ │ │
│  │  │ Curve25519 │  │  └────────────────┘  │ │ │
│  │  └─────┬──────┘  └──────────────────────┘ │ │
│  └────────┼──────────────────────────────────┘ │
│           │                                     │
│  ┌────────┴──────────────────────────────────┐ │
│  │          Wintun Driver (Kernel)            │ │
│  │     Layer 3 TUN Virtual Network Adapter    │ │
│  │              wintun.dll (signed)           │ │
│  └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
         │
         │ UDP 51820 (encrypted WireGuard packets)
         │
    ═════╧═════════════════════════
         │
    VPN Gateway
```

---

## 2. Core Components

### 2.1 Wintun Integration

Wintun is the Layer 3 TUN driver that provides the virtual network adapter on Windows. It ships as a single signed DLL — no separate driver installation needed.

**Rust bindings:**

```rust
// Cargo.toml
[dependencies]
wintun = "0.5"          # Rust wrapper for wintun.dll
boringtun = "0.6"       # WireGuard protocol (Cloudflare)

[target.'cfg(windows)'.dependencies]
windows-service = "0.7" # Windows Service framework
winreg = "0.52"         # Registry access
```

**Wintun adapter lifecycle:**

```rust
use wintun::{Adapter, Session};

// 1. Load wintun.dll (bundled with the application)
let wintun = unsafe { wintun::load_from_path("wintun.dll")? };

// 2. Create or open TUN adapter
let adapter = Adapter::create(
    &wintun,
    "SeekerVPN",           // Adapter name (shown in Network Connections)
    "SeekerVPN Tunnel",    // Tunnel type
    None                   // Optional GUID
)?;

// 3. Set IP address on the adapter
adapter.set_address(Ipv4Addr::new(10, 0, 0, 2))?;
adapter.set_netmask(Ipv4Addr::new(255, 255, 255, 0))?;

// 4. Start session (ring buffer for packet I/O)
let session = adapter.start_session(wintun::MAX_RING_CAPACITY)?; // 64 MB ring

// 5. Read/write packets
loop {
    // Read outgoing packet from Windows network stack
    let packet = session.receive_blocking()?;
    let raw_bytes = packet.bytes();

    // Encrypt with BoringTun and send via UDP socket
    let encrypted = tunnel.encapsulate(raw_bytes, &mut send_buf)?;
    udp_socket.send_to(encrypted, gateway_endpoint)?;

    // Receive encrypted packet from gateway
    let n = udp_socket.recv(&mut recv_buf)?;
    let decrypted = tunnel.decapsulate(None, &recv_buf[..n], &mut dec_buf)?;

    // Write decrypted packet to Windows network stack
    let mut write_packet = session.allocate_send_packet(decrypted.len())?;
    write_packet.bytes_mut().copy_from_slice(decrypted);
    session.send_packet(write_packet);
}
```

### 2.2 BoringTun WireGuard Engine

```rust
use boringtun::noise::{Tunn, TunnResult};
use boringtun::x25519::{PublicKey, StaticSecret};

// Create WireGuard tunnel instance
let private_key = StaticSecret::from(key_bytes);
let peer_public_key = PublicKey::from(peer_key_bytes);

let tunnel = Tunn::new(
    private_key,
    peer_public_key,
    Some(preshared_key),    // Post-quantum PSK
    Some(25),               // Persistent keepalive (seconds)
    0,                      // Tunnel index
    None,                   // Rate limiter
)?;

// Encapsulate: plaintext IP packet → encrypted WireGuard packet
match tunnel.encapsulate(plaintext, &mut encrypted_buf) {
    TunnResult::WriteToNetwork(data) => {
        udp_socket.send_to(data, gateway)?;
    }
    TunnResult::Done => {} // Nothing to send
    TunnResult::Err(e) => log::error!("Encrypt error: {e:?}"),
}

// Decapsulate: encrypted WireGuard packet → plaintext IP packet
match tunnel.decapsulate(None, &ciphertext, &mut plaintext_buf) {
    TunnResult::WriteToTunInterface(data) => {
        tun_session.send_packet(data); // Inject into Windows network stack
    }
    TunnResult::WriteToNetwork(data) => {
        udp_socket.send_to(data, gateway)?; // Handshake response
    }
    TunnResult::Done => {}
    TunnResult::Err(e) => log::error!("Decrypt error: {e:?}"),
}
```

### 2.3 Windows Service

The VPN engine runs as an NT Service for auto-start and privilege management:

```rust
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

const SERVICE_NAME: &str = "SeekerVPN";
const SERVICE_DISPLAY: &str = "Seeker VPN Service";

define_windows_service!(ffi_service_main, service_main);

fn service_main(arguments: Vec<OsString>) {
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                // Graceful shutdown: close tunnel, remove routes
                tunnel_handle.shutdown();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::SessionChange(info) => {
                // Handle user login/logout, lock/unlock
                handle_session_change(info);
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    // Report "Running"
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SESSION_CHANGE,
        ..Default::default()
    })?;

    // Main VPN loop
    run_vpn_tunnel(status_handle);
}

fn main() -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}
```

**Service registration (install.rs):**

```rust
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType};

fn install_service() -> Result<()> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CREATE_SERVICE,
    )?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![],
        account_name: None,     // LocalSystem
        account_password: None,
    };

    manager.create_service(&service_info, ServiceAccess::full_access())?;
    Ok(())
}
```

---

## 3. Key Storage (DPAPI)

Windows Data Protection API encrypts the private key so only the Local System account can decrypt it:

```rust
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    CRYPTPROTECT_LOCAL_MACHINE,
};

fn encrypt_key(private_key: &[u8]) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: private_key.len() as u32,
        pbData: private_key.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &input,
            w!("SeekerVPN Private Key"),  // Description
            None,                          // Optional entropy
            None,                          // Reserved
            None,                          // Prompt struct
            CRYPTPROTECT_LOCAL_MACHINE,    // Machine-level protection
            &mut output,
        )?;
    }

    let encrypted = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    Ok(encrypted)
}

fn decrypt_key(encrypted: &[u8]) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_LOCAL_MACHINE,
            &mut output,
        )?;
    }

    let decrypted = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    Ok(decrypted)
}
```

**Key file locations:**

```
C:\ProgramData\SeekerVPN\
├── config.json.dpapi        # Encrypted tunnel config (gateway, allowed IPs, DNS)
├── private.key.dpapi         # Encrypted X25519 private key
├── psk.key.dpapi             # Encrypted pre-shared key
└── wintun.dll                # Signed TUN driver
```

---

## 4. Network Management

### 4.1 Split Tunneling

```rust
use std::process::Command;

fn configure_split_tunnel(
    tun_adapter_index: u32,
    corporate_networks: &[&str],   // e.g., ["10.0.0.0/8", "172.16.0.0/12"]
    dns_servers: &[&str],          // e.g., ["10.0.0.53"]
) -> Result<()> {
    // Add routes for corporate networks through the VPN tunnel
    for network in corporate_networks {
        Command::new("route")
            .args(["add", network, "10.0.0.1", "if", &tun_adapter_index.to_string()])
            .output()?;
    }

    // Set DNS for the VPN adapter only (split DNS)
    for dns in dns_servers {
        Command::new("netsh")
            .args([
                "interface", "ip", "add", "dns",
                &format!("name={tun_adapter_index}"),
                dns,
            ])
            .output()?;
    }

    // Set DNS suffix for NRPT (Name Resolution Policy Table)
    // This routes *.internal.company.com queries to corporate DNS
    set_nrpt_rule("internal.company.com", dns_servers)?;

    Ok(())
}
```

### 4.2 Kill Switch

Block all non-VPN traffic when the tunnel drops unexpectedly:

```rust
use std::process::Command;

fn enable_kill_switch(gateway_endpoint: &str, tun_adapter_index: u32) -> Result<()> {
    // Windows Firewall rules: block all except VPN and DHCP
    let rules = [
        // Allow VPN tunnel traffic (UDP to gateway)
        format!(
            "netsh advfirewall firewall add rule name=\"SeekerVPN-Allow-Tunnel\" \
             dir=out action=allow protocol=udp remoteip={gateway_endpoint} remoteport=51820"
        ),
        // Allow traffic through TUN adapter only
        format!(
            "netsh advfirewall firewall add rule name=\"SeekerVPN-Allow-TUN\" \
             dir=out action=allow interface=\"SeekerVPN\""
        ),
        // Allow DHCP
        "netsh advfirewall firewall add rule name=\"SeekerVPN-Allow-DHCP\" \
         dir=out action=allow protocol=udp localport=68 remoteport=67".to_string(),
        // Allow loopback
        "netsh advfirewall firewall add rule name=\"SeekerVPN-Allow-Loopback\" \
         dir=out action=allow remoteip=127.0.0.0/8".to_string(),
        // Block everything else
        "netsh advfirewall firewall add rule name=\"SeekerVPN-Block-All\" \
         dir=out action=block".to_string(),
    ];

    for rule in &rules {
        Command::new("cmd").args(["/C", rule]).output()?;
    }
    Ok(())
}

fn disable_kill_switch() -> Result<()> {
    // Remove all SeekerVPN firewall rules
    Command::new("cmd")
        .args(["/C", "netsh advfirewall firewall delete rule name=all dir=out \
               | findstr SeekerVPN"])
        .output()?;
    // More reliable: delete by name prefix
    for suffix in ["Allow-Tunnel", "Allow-TUN", "Allow-DHCP", "Allow-Loopback", "Block-All"] {
        Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule",
                   &format!("name=SeekerVPN-{suffix}")])
            .output()?;
    }
    Ok(())
}
```

### 4.3 Auto-Connect on Network Change

```rust
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use tokio::sync::watch;

async fn monitor_network_changes(reconnect_tx: watch::Sender<bool>) {
    // Monitor Windows network connectivity changes
    loop {
        let event = wait_for_network_event().await;

        match event {
            NetworkEvent::Connected(interface) => {
                log::info!("Network connected: {interface}");
                // Trigger VPN reconnect
                reconnect_tx.send(true).ok();
            }
            NetworkEvent::Disconnected => {
                log::warn!("Network disconnected, waiting...");
            }
            NetworkEvent::AddressChanged => {
                log::info!("IP address changed, reconnecting VPN");
                reconnect_tx.send(true).ok();
            }
        }
    }
}
```

---

## 5. Enrollment Flow

### 5.1 First-Time Setup

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EnrollRequest {
    username: String,
    password: String,
    totp_code: String,
    device_name: String,
    platform: String,
    os_version: String,
    hostname: String,
}

#[derive(Deserialize)]
struct EnrollResponse {
    device_id: String,
    private_key: String,           // X25519 private key (base64)
    preshared_key: String,         // PSK (base64)
    tunnel_config: TunnelConfig,
}

#[derive(Deserialize)]
struct TunnelConfig {
    address: String,               // e.g., "10.0.0.42/32"
    dns: Vec<String>,              // e.g., ["10.0.0.53"]
    allowed_ips: Vec<String>,      // e.g., ["10.0.0.0/8"]
    gateway_endpoint: String,      // e.g., "vpn.company.com:51820"
    gateway_public_key: String,    // Gateway's X25519 public key
    persistent_keepalive: u16,     // 25 seconds
}

async fn enroll_device(portal_url: &str, username: &str, password: &str, totp: &str) -> Result<()> {
    let client = Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    let hostname = hostname::get()?.to_string_lossy().to_string();
    let os_info = os_info::get();

    let response: EnrollResponse = client
        .post(format!("{portal_url}/api/v1/devices/enroll"))
        .json(&EnrollRequest {
            username: username.to_string(),
            password: password.to_string(),
            totp_code: totp.to_string(),
            device_name: hostname.clone(),
            platform: "windows".to_string(),
            os_version: format!("{} {}", os_info.os_type(), os_info.version()),
            hostname,
        })
        .send()
        .await?
        .json()
        .await?;

    // Encrypt and store keys with DPAPI
    let encrypted_key = encrypt_key(
        &base64::decode(&response.private_key)?
    )?;
    std::fs::write("C:\\ProgramData\\SeekerVPN\\private.key.dpapi", &encrypted_key)?;

    let encrypted_psk = encrypt_key(
        &base64::decode(&response.preshared_key)?
    )?;
    std::fs::write("C:\\ProgramData\\SeekerVPN\\psk.key.dpapi", &encrypted_psk)?;

    // Store tunnel config (encrypted)
    let config_json = serde_json::to_vec(&response.tunnel_config)?;
    let encrypted_config = encrypt_key(&config_json)?;
    std::fs::write("C:\\ProgramData\\SeekerVPN\\config.json.dpapi", &encrypted_config)?;

    log::info!("Device enrolled successfully: {}", response.device_id);
    Ok(())
}
```

### 5.2 Config Refresh

The client periodically checks the portal for updated configs (new gateway, changed ACLs):

```rust
async fn refresh_config(portal_url: &str, device_id: &str, jwt: &str) -> Result<Option<TunnelConfig>> {
    let client = Client::new();
    let response = client
        .get(format!("{portal_url}/api/v1/devices/{device_id}/config"))
        .bearer_auth(jwt)
        .send()
        .await?;

    match response.status() {
        StatusCode::OK => {
            let config: TunnelConfig = response.json().await?;
            Ok(Some(config))
        }
        StatusCode::NOT_MODIFIED => Ok(None),  // No changes
        StatusCode::UNAUTHORIZED => {
            log::warn!("Device revoked or token expired");
            Err(anyhow!("Device revoked"))
        }
        _ => Err(anyhow!("Config refresh failed: {}", response.status())),
    }
}
```

---

## 6. Enterprise Deployment

### 6.1 MSI Installer

Build with WiX Toolset for Group Policy deployment:

```xml
<!-- Product.wxs -->
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*"
           Name="Seeker VPN Client"
           Manufacturer="Seekers Lab"
           Version="1.0.0"
           UpgradeCode="YOUR-GUID-HERE">

    <Package InstallerVersion="500" Compressed="yes" InstallScope="perMachine" />
    <MajorUpgrade DowngradeErrorMessage="A newer version is already installed." />

    <Property Id="PORTAL_URL" Value="https://vpn.company.com" />
    <Property Id="DO_NOT_LAUNCH" Value="0" />

    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLFOLDER" Name="SeekerVPN">
          <Component Id="MainExecutable" Guid="*">
            <File Source="seekervpn.exe" KeyPath="yes" />
            <File Source="wintun.dll" />
            <ServiceInstall
              Id="SeekerVPNService"
              Name="SeekerVPN"
              DisplayName="Seeker VPN Service"
              Description="Seeker VPN WireGuard tunnel service"
              Type="ownProcess"
              Start="auto"
              ErrorControl="normal"
              Arguments="--service" />
            <ServiceControl
              Id="StartService"
              Name="SeekerVPN"
              Start="install"
              Stop="both"
              Remove="uninstall"
              Wait="yes" />
          </Component>
        </Directory>
      </Directory>
      <Directory Id="CommonAppDataFolder">
        <Directory Id="SeekerVPNData" Name="SeekerVPN">
          <Component Id="DataDir" Guid="*">
            <CreateFolder />
          </Component>
        </Directory>
      </Directory>
    </Directory>

    <Feature Id="Complete" Title="Seeker VPN" Level="1">
      <ComponentRef Id="MainExecutable" />
      <ComponentRef Id="DataDir" />
    </Feature>

    <!-- Write portal URL to registry for the service to read -->
    <Component Id="RegistryEntries" Directory="INSTALLFOLDER" Guid="*">
      <RegistryKey Root="HKLM" Key="SOFTWARE\SeekerVPN">
        <RegistryValue Name="PortalURL" Type="string" Value="[PORTAL_URL]" />
        <RegistryValue Name="Version" Type="string" Value="1.0.0" />
        <RegistryValue Name="InstallPath" Type="string" Value="[INSTALLFOLDER]" />
      </RegistryKey>
    </Component>

  </Product>
</Wix>
```

### 6.2 Group Policy Deployment

```
1. Place MSI on network share: \\dc\shares\software\SeekerVPN-1.0.0.msi

2. Group Policy Object:
   Computer Configuration
   └── Policies
       └── Software Settings
           └── Software Installation
               └── New Package: \\dc\shares\software\SeekerVPN-1.0.0.msi
                   Assignment: Assigned (auto-install at boot)

3. Custom properties:
   msiexec /i SeekerVPN-1.0.0.msi
     PORTAL_URL="https://vpn.company.com"
     DO_NOT_LAUNCH=1
     /qn                    # Silent install
     /l*v install.log       # Verbose logging
```

### 6.3 SCCM / Intune Deployment

```powershell
# SCCM Application Detection Rule
$reg = Get-ItemProperty "HKLM:\SOFTWARE\SeekerVPN" -ErrorAction SilentlyContinue
if ($reg.Version -ge "1.0.0") { Write-Host "Installed" }

# Intune Win32 App
# Install command:
msiexec /i SeekerVPN-1.0.0.msi PORTAL_URL="https://vpn.company.com" /qn

# Uninstall command:
msiexec /x {PRODUCT-GUID} /qn

# Detection rule:
# File exists: C:\Program Files\SeekerVPN\seekervpn.exe
```

### 6.4 Zero-Touch Enrollment

The client can auto-enroll after MSI installation using a pre-shared enrollment token:

```
Boot → GPO installs MSI → Service starts →
  Service reads PORTAL_URL + ENROLLMENT_TOKEN from registry →
  Service registers device with portal using enrollment token →
  Portal returns VPN config for this machine →
  Tunnel connects automatically →
  User logs in → MFA prompt → Full access
```

---

## 7. Client Configuration File

The client generates a standard WireGuard config internally:

```ini
# Generated by SeekerVPN client (stored encrypted via DPAPI)
[Interface]
PrivateKey = <base64 X25519 private key>
Address = 10.0.0.42/32
DNS = 10.0.0.53, 10.0.0.54
MTU = 1420

[Peer]
PublicKey = <gateway public key base64>
PresharedKey = <PSK base64>
AllowedIPs = 10.0.0.0/8, 172.16.0.0/12
Endpoint = vpn.company.com:51820
PersistentKeepalive = 25
```

This config is never stored in plaintext — it's encrypted with DPAPI immediately after generation.

---

## 8. Monitoring & Diagnostics

### Client-Side Logging

```
C:\ProgramData\SeekerVPN\logs\
├── service.log           # VPN service events
├── tunnel.log            # WireGuard handshake + packet stats
└── enrollment.log        # Device enrollment events
```

### Status Reporting to Portal

```rust
#[derive(Serialize)]
struct ClientStatus {
    device_id: String,
    connected: bool,
    gateway_endpoint: String,
    tunnel_ip: String,
    uptime_seconds: u64,
    tx_bytes: u64,
    rx_bytes: u64,
    last_handshake: DateTime<Utc>,
    os_version: String,
    client_version: String,
    // Device posture
    antivirus_enabled: bool,
    firewall_enabled: bool,
    disk_encrypted: bool,
    os_up_to_date: bool,
}
```

### Diagnostic Commands

```powershell
# Check service status
sc query SeekerVPN

# View logs
Get-Content C:\ProgramData\SeekerVPN\logs\service.log -Tail 50

# Check TUN adapter
Get-NetAdapter | Where-Object { $_.InterfaceDescription -like "*SeekerVPN*" }

# Check routes
route print | findstr "10.0.0"

# Test connectivity through tunnel
Test-NetConnection -ComputerName 10.0.0.1 -Port 443
```

---

## 9. Rust Project Structure

```
seekervpn-client/
├── Cargo.toml
├── build.rs                    # Embed wintun.dll, set Windows manifest
├── installer/
│   ├── Product.wxs             # WiX MSI definition
│   └── build.ps1               # MSI build script
├── src/
│   ├── main.rs                 # Entry point (CLI or service dispatcher)
│   ├── service.rs              # Windows Service implementation
│   ├── tunnel.rs               # BoringTun + Wintun integration
│   ├── config.rs               # Config load/save (DPAPI encrypted)
│   ├── enrollment.rs           # Device enrollment + key generation
│   ├── network.rs              # Routes, DNS, split tunnel, kill switch
│   ├── monitor.rs              # Network change detection
│   ├── updater.rs              # Auto-update check + MSI upgrade
│   ├── tray.rs                 # System tray UI (Tauri/egui)
│   └── posture.rs              # Device posture checks (AV, FW, encryption)
├── assets/
│   ├── wintun.dll              # Signed Wintun driver
│   ├── icon.ico                # Tray icon
│   └── logo.png                # UI logo
└── tests/
    ├── tunnel_test.rs           # WireGuard handshake tests
    └── config_test.rs           # Config encrypt/decrypt tests
```

**Cargo.toml:**

```toml
[package]
name = "seekervpn-client"
version = "1.0.0"
edition = "2021"

[dependencies]
boringtun = "0.6"
wintun = "0.5"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
hostname = "0.4"
os_info = "3"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Security_Cryptography",
    "Win32_NetworkManagement_Ndis",
    "Win32_Networking_WinSock",
]}
windows-service = "0.7"
winreg = "0.52"
tauri = { version = "2", features = ["tray-icon"] }

[profile.release]
lto = true
strip = true
codegen-units = 1
```
