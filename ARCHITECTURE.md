# SessionGate System Architecture

## 1. Why WireGuard

| Factor | WireGuard | OpenVPN | IPSec/IKEv2 |
|--------|-----------|---------|-------------|
| Codebase | ~4,000 LOC | ~100,000 LOC | ~400,000 LOC |
| Throughput | 920–960 Mbps | 650–780 Mbps | 700–850 Mbps |
| CPU usage | 8–15% | 45–60% | 20–30% |
| Latency | 0.2–0.5 ms | 2–5 ms | 1–3 ms |
| Max PPS | 1M+ | ~200K | ~500K |
| Roaming | Built-in | Reconnect | Reconnect |
| Crypto | ChaCha20, Curve25519 | TLS (configurable) | IKEv2 (configurable) |
| License | GPL/MIT | GPL | Varies |

**Decision: WireGuard** — smallest attack surface, highest throughput, lowest latency, built-in roaming for mobile workers.

---

## 2. High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   Management Portal                      │
│              (Rust Axum + React Frontend)                │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│  │ User Mgmt│  │Device Mgmt│ │ Tunnel   │  │Dashboard│ │
│  │ DB Auth  │  │ Enroll   │  │ Config   │  │ Monitor │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │
│                       │                                  │
│              ┌────────┴────────┐                        │
│              │   PostgreSQL    │                        │
│              │  Users/Devices  │                        │
│              │  Tunnels/Keys   │                        │
│              └────────┬────────┘                        │
└───────────────────────┼─────────────────────────────────┘
                        │ REST API / gRPC
          ┌─────────────┼─────────────┐
          │             │             │
    ┌─────┴─────┐ ┌─────┴─────┐ ┌────┴─────┐
    │  VPN GW 1 │ │  VPN GW 2 │ │  VPN GW N│   (Linux servers)
    │ WireGuard │ │ WireGuard │ │ WireGuard│
    │  Kernel   │ │  Kernel   │ │  Kernel  │
    └─────┬─────┘ └─────┬─────┘ └────┬─────┘
          │             │             │
    ══════╧═════════════╧═════════════╧══════   WireGuard Tunnels (UDP 51820)
          │             │             │
    ┌─────┴───┐   ┌─────┴───┐   ┌────┴────┐
    │ Windows │   │ Windows │   │  macOS  │   Clients
    │ Client  │   │ Client  │   │ Client  │
    │BoringTun│   │BoringTun│   │BoringTun│
    │+ Wintun │   │+ Wintun │   │  utun   │
    └─────────┘   └─────────┘   └─────────┘
```

---

## 3. Component Design

### 3.1 VPN Gateway (Linux Server)

```
VPN Gateway
├── WireGuard Kernel Module        # Packet encryption/decryption
│   └── wg0 interface             # Virtual network interface
├── Gateway Agent (Rust)           # Management plane
│   ├── Config Sync               # Pull config from Management Portal
│   ├── Key Rotation              # Auto-rotate peer keys on schedule
│   ├── Health Reporter           # Send metrics to portal
│   └── ACL Enforcer              # Apply per-user network policies
├── iptables/nftables              # Firewall + NAT rules
└── DNS Resolver                   # Split DNS for internal domains
```

**Key design decisions:**
- Use **kernel WireGuard** (not userspace) on Linux gateways for maximum throughput
- Gateway agent runs as systemd service, pulls config via REST API
- Multiple gateways behind DNS round-robin or load balancer for HA
- Each gateway handles 1,000–10,000 concurrent peers easily

### 3.2 Management Portal

```
Management Portal (Rust + React)
├── API Server (Axum)
│   ├── /api/v1/auth               # Login, DB credential verify, TOTP verify
│   ├── /api/v1/users              # CRUD users, assign roles
│   ├── /api/v1/devices            # Register/revoke devices
│   ├── /api/v1/tunnels            # Create/update tunnel configs
│   ├── /api/v1/gateways           # Gateway registration + health
│   ├── /api/v1/policies           # Network access policies
│   └── /api/v1/audit              # Audit log queries
├── Auth Module
│   ├── DB Credentials             # Username/password (Argon2 hashed)
│   ├── TOTP/WebAuthn MFA          # Second factor
│   └── JWT Session Management     # API tokens
├── Key Management
│   ├── X25519 Key Generation      # Per-device keypairs
│   ├── Pre-shared Key (PSK)       # Post-quantum resistance layer
│   └── Key Rotation Scheduler     # Auto-rotate every N days
├── Config Generator
│   ├── Server Config Builder      # Generate wg0.conf for gateways
│   └── Client Config Builder      # Generate peer .conf for clients
├── Database (PostgreSQL)
│   ├── users                      # id, username, password_hash, email, role, mfa_secret
│   ├── devices                    # id, user_id, public_key, platform, enrolled_at
│   ├── tunnels                    # id, device_id, gateway_id, allowed_ips, dns
│   ├── gateways                   # id, endpoint, public_key, region, status
│   ├── policies                   # id, name, allowed_networks, allowed_apps
│   └── audit_log                  # timestamp, user, action, ip, details
└── Frontend (React)
    ├── Dashboard                  # Connected users, bandwidth, alerts
    ├── User Management            # List/add/disable users
    ├── Device Management          # Enrolled devices, revoke
    ├── Gateway Status             # Health, load, connected peers
    └── Self-Service Portal        # User downloads client + config
```

### 3.3 Windows Client

```
Windows Client (Rust)
├── VPN Engine
│   ├── BoringTun (Rust)           # WireGuard protocol implementation
│   │   ├── Noise IK handshake    # Key exchange (Curve25519)
│   │   ├── ChaCha20-Poly1305     # Packet encryption
│   │   └── Cookie/MAC            # DoS protection
│   └── Wintun Driver             # Layer 3 TUN adapter (kernel)
│       └── wintun.dll            # Signed driver, no install needed
├── System Tray UI (egui/tauri)
│   ├── Connect/Disconnect        # One-click VPN toggle
│   ├── Gateway Selection         # Choose closest/fastest gateway
│   ├── Status Display            # Connected, IP, bandwidth
│   └── Settings                  # Auto-connect, kill switch
├── Service Layer
│   ├── Windows Service           # Runs as NT Service (LocalSystem)
│   ├── Auto-Connect              # Connect on network change
│   ├── Kill Switch               # Block traffic when VPN drops
│   └── Split Tunneling           # Route only corporate traffic
├── Enrollment
│   ├── Device Registration       # Generate keypair, register with portal
│   ├── Config Download           # Pull tunnel config via API
│   ├── DPAPI Encryption          # Encrypt keys at rest (Windows)
│   └── Certificate Pinning       # Pin portal TLS cert
└── Update Module
    ├── Auto-Update Check          # Poll portal for new versions
    └── MSI Silent Update          # Background upgrade
```

See [WINDOWS-CLIENT.md](WINDOWS-CLIENT.md) for detailed Windows implementation.

---

## 4. Authentication Flow

```
User Login Flow:

  Windows Client          Management Portal         PostgreSQL DB
       │                        │                        │
       │  1. Login Request      │                        │
       │  (username, password)  │                        │
       │───────────────────────>│                        │
       │                        │  2. Verify password    │
       │                        │  (Argon2 hash check)   │
       │                        │───────────────────────>│
       │                        │  3. User record +      │
       │                        │     role/policy        │
       │                        │<───────────────────────│
       │  4. MFA Challenge      │                        │
       │  (TOTP required)       │                        │
       │<───────────────────────│                        │
       │  5. TOTP Code          │                        │
       │───────────────────────>│                        │
       │  6. JWT + VPN Config   │                        │
       │  (allowed_ips, dns,    │                        │
       │   gateway endpoint,    │                        │
       │   peer public key)     │                        │
       │<───────────────────────│                        │
       │                        │                        │
       │  7. WireGuard Handshake│                        │
       │═══════════════════════>│  VPN Gateway           │
       │  8. Tunnel Active      │                        │
       │<═══════════════════════│                        │
```

**Device Enrollment Flow:**

```
1. Admin creates user in portal (username, password, role)
2. User logs into self-service portal with credentials + MFA
3. Portal generates X25519 keypair for the device
4. Private key delivered to client via HTTPS (one-time download)
5. Client stores private key encrypted with DPAPI (Windows) or Keychain (macOS)
6. Portal pushes public key to assigned gateway(s)
7. WireGuard tunnel is ready
```

---

## 5. Network Architecture

### Split Tunneling

```
Windows Client
│
├── Corporate Traffic (10.0.0.0/8, internal.company.com)
│   └──> WireGuard Tunnel ──> VPN Gateway ──> Corporate Network
│
└── Internet Traffic (0.0.0.0/0)
    └──> Direct Internet (no VPN)
```

### Full Tunnel

```
Windows Client
│
└── All Traffic (0.0.0.0/0)
    └──> WireGuard Tunnel ──> VPN Gateway ──> Internet / Corporate
```

### Policy-Based Routing

| User Role | Allowed Networks | DNS | Gateway |
|-----------|-----------------|-----|---------|
| admin | 10.0.0.0/8, 172.16.0.0/12 | internal.dns | gw-hq |
| developer | 10.10.0.0/16 (dev VLAN) | dev.dns | gw-dev |
| sales | 10.20.0.0/16 (CRM only) | crm.dns | gw-closest |
| contractor | 10.30.0.0/24 (isolated) | public DNS | gw-dmz |

---

## 6. Security Design

### Cryptographic Stack (WireGuard)

| Layer | Algorithm | Purpose |
|-------|-----------|---------|
| Key Exchange | Curve25519 (ECDH) | Peer authentication + key agreement |
| Symmetric Encryption | ChaCha20-Poly1305 | Packet encryption + integrity |
| Hashing | BLAKE2s | Key derivation, MAC |
| Pre-shared Key | 256-bit symmetric | Post-quantum resistance layer |

### Key Lifecycle

```
Key Rotation Schedule:
├── Device Keys (X25519)     Rotate every 90 days (configurable)
├── Pre-shared Keys          Rotate every 30 days
├── Session Keys             Rotate every 2 minutes (WireGuard built-in)
└── JWT Tokens               Expire after 8 hours
```

### Zero-Trust Principles

1. **No implicit trust** — Every device must be enrolled and authenticated
2. **Least privilege** — User role determines allowed networks
3. **MFA required** — TOTP or WebAuthn for all users
4. **Device posture** — Client reports OS version, AV status, patch level
5. **Continuous verification** — Re-auth on network change or after timeout
6. **Audit everything** — All connections logged with user, IP, duration

---

## 7. High Availability

```
                    DNS Round Robin / Load Balancer
                    ┌───────────────────────┐
                    │    vpn.company.com     │
                    └───────┬───────┬───────┘
                            │       │
                    ┌───────┴──┐ ┌──┴───────┐
                    │  GW-01   │ │  GW-02   │    Active-Active
                    │ Seoul    │ │ Busan    │
                    │ wg0:     │ │ wg0:     │
                    │ 51820/udp│ │ 51820/udp│
                    └──────────┘ └──────────┘
                         │            │
                    ┌────┴────────────┴────┐
                    │   Shared PostgreSQL   │    Config + state sync
                    │   (or per-GW sync)    │
                    └──────────────────────┘
```

- Clients receive multiple gateway endpoints; failover is automatic (WireGuard roaming)
- Each gateway independently pulls config from portal
- No shared state between gateways (WireGuard is stateless per-peer)
- PostgreSQL for portal is replicated separately

---

## 8. Technology Stack

| Component | Technology | Justification |
|-----------|-----------|---------------|
| VPN Protocol | WireGuard | Performance, simplicity, audit-friendly |
| Server VPN | Linux kernel module (wireguard-tools) | Maximum throughput on Linux |
| Client VPN Engine | BoringTun (Rust, Cloudflare) | Userspace, cross-platform, proven at scale |
| Windows TUN | Wintun (signed driver) | Official WireGuard TUN adapter, MIT license |
| API Server | Rust + Axum | Same stack as OpenOrb, high performance |
| Frontend | React + TypeScript | Standard enterprise dashboard |
| Database | PostgreSQL | Users, credentials, audit logs, config store |
| Auth | DB credentials (Argon2) + TOTP | Self-contained, no external dependency |
| Client UI | Tauri (Rust + WebView) | Native Windows app, small footprint (~5 MB) |
| Installer | WiX / MSI | Group Policy deployment |
| Key Storage | Windows DPAPI | OS-level encryption at rest |

---

## 9. Performance Targets

| Metric | Target |
|--------|--------|
| Handshake latency | < 100 ms |
| Throughput (per client) | > 500 Mbps |
| Throughput (per gateway) | > 10 Gbps |
| Concurrent peers per GW | 10,000 |
| Key rotation | < 50 ms downtime |
| Reconnect after roaming | < 1 second |
| Client binary size | < 15 MB (MSI) |
| Memory usage (client) | < 30 MB |
