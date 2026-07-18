# SessionGate Implementation Plan

## Phase Overview

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 1 | 4 weeks | Core VPN tunnel (server + Windows client) |
| Phase 2 | 3 weeks | Management portal + authentication |
| Phase 3 | 2 weeks | Enterprise deployment (MSI, GPO, auto-enroll) |
| Phase 4 | 2 weeks | Production hardening + monitoring |

---

## Phase 1: Core VPN Tunnel

**Goal:** Working WireGuard tunnel between Windows client and Linux server.

### Server Side (Linux)

```
Week 1-2:
├── [1] Set up WireGuard kernel module on Linux server
│   ├── Install wireguard-tools
│   ├── Generate server keypair
│   ├── Configure wg0.conf
│   └── Enable IP forwarding + NAT (iptables)
│
├── [2] Gateway Agent (Rust)
│   ├── Read/write wg0.conf programmatically
│   ├── Add/remove peers via `wg set` or netlink
│   ├── Health check endpoint (HTTP /health)
│   └── Systemd service unit
│
└── [3] Test: Manual peer addition
    ├── Generate client keypair
    ├── Add peer to server
    └── Verify tunnel connectivity
```

### Client Side (Windows)

```
Week 2-4:
├── [4] Wintun integration
│   ├── Load wintun.dll
│   ├── Create TUN adapter
│   ├── Read/write packets from ring buffer
│   └── Set IP address and routes
│
├── [5] BoringTun integration
│   ├── Initialize Tunn with static keys
│   ├── Encapsulate outgoing packets
│   ├── Decapsulate incoming packets
│   ├── Handle handshake + keepalive
│   └── Timer tick for key rotation
│
├── [6] Packet loop
│   ├── Wintun RX → BoringTun encrypt → UDP TX
│   ├── UDP RX → BoringTun decrypt → Wintun TX
│   ├── Async tokio runtime
│   └── Graceful shutdown
│
├── [7] Windows Service
│   ├── NT Service registration
│   ├── Start/stop/restart
│   ├── Auto-start on boot
│   └── Session change handling
│
└── [8] Basic CLI
    ├── seekervpn connect --config tunnel.conf
    ├── seekervpn disconnect
    ├── seekervpn status
    └── seekervpn install-service / uninstall-service
```

### Phase 1 Milestone

```
✓ Windows client connects to Linux server via WireGuard tunnel
✓ Bidirectional traffic flows (ping, HTTP, DNS)
✓ Runs as Windows Service with auto-start
✓ Manual key configuration (no portal yet)
```

---

## Phase 2: Management Portal

**Goal:** Web portal for user/device management with DB-based authentication.

### Backend (Rust + Axum)

```
Week 5-6:
├── [9] Database schema (PostgreSQL)
│   ├── users (id, username, password_hash, email, role, mfa_secret, created_at)
│   ├── devices (id, user_id, name, platform, public_key, enrolled_at, last_seen)
│   ├── gateways (id, name, endpoint, public_key, region, status)
│   ├── tunnels (id, device_id, gateway_id, address, allowed_ips, dns)
│   ├── policies (id, name, role, allowed_networks, dns_servers)
│   └── audit_log (id, timestamp, user_id, action, ip, details)
│
├── [10] Auth module
│   ├── Password verification (Argon2 hashing via argon2 crate)
│   ├── TOTP generation + verification (totp-rs crate)
│   ├── JWT session tokens (jsonwebtoken crate)
│   └── Password-less re-auth for enrolled devices
│
├── [11] API endpoints
│   ├── POST /api/v1/auth/login (credentials + TOTP → JWT)
│   ├── POST /api/v1/devices/enroll (register device, return keys + config)
│   ├── GET  /api/v1/devices/:id/config (pull latest config)
│   ├── POST /api/v1/devices/:id/status (client status report)
│   ├── GET  /api/v1/gateways (list available gateways)
│   ├── CRUD /api/v1/users
│   ├── CRUD /api/v1/policies
│   └── GET  /api/v1/audit (audit log query)
│
├── [12] Config generator
│   ├── Build WireGuard conf from DB records
│   ├── Push peer updates to gateways via agent API
│   └── Handle key rotation schedule
│
└── [13] Gateway sync
    ├── Gateway agent polls portal for config changes
    ├── Apply peer add/remove via wg command
    └── Report connected peers + bandwidth stats
```

### Frontend (React)

```
Week 6-7:
├── [14] Login page (username + password + TOTP)
├── [15] Dashboard (connected users, bandwidth, gateway status)
├── [16] User management (create/edit/disable users, assign roles)
├── [17] Device management (enrolled devices, revoke, view status)
├── [18] Policy editor (user role → allowed networks mapping)
├── [19] Gateway status (health, load, peer count)
├── [20] Self-service portal (download client + QR code for enrollment)
└── [21] Audit log viewer (search by user, action, date)
```

### Client Updates

```
Week 7:
├── [22] Enrollment flow in client
│   ├── Login dialog (username + password + TOTP)
│   ├── API call to portal for enrollment
│   ├── Store keys with DPAPI
│   └── Auto-configure tunnel from response
│
├── [23] Config refresh
│   ├── Periodic poll for config changes
│   ├── Apply new routes/DNS without reconnect
│   └── Handle device revocation (disconnect + cleanup)
│
└── [24] Status reporting
    ├── Send connected/disconnected events
    ├── Report bandwidth stats
    └── Device posture checks
```

### Phase 2 Milestone

```
✓ Admin manages users/devices/policies via web portal
✓ Users authenticate with DB credentials + TOTP
✓ Client auto-enrolls and receives config from portal
✓ Key rotation works end-to-end
✓ Audit log captures all actions
```

---

## Phase 3: Enterprise Deployment

**Goal:** Silent deployment to Windows fleet via Group Policy.

```
Week 8-9:
├── [25] MSI installer (WiX)
│   ├── Bundle seekervpn.exe + wintun.dll
│   ├── Register Windows Service
│   ├── Write PORTAL_URL to registry
│   ├── Support DO_NOT_LAUNCH property
│   └── Upgrade + uninstall support
│
├── [26] Group Policy deployment
│   ├── Test GPO software installation
│   ├── Document target machine groups
│   ├── Verify silent install + auto-start
│   └── Test upgrade path (v1.0 → v1.1)
│
├── [27] Zero-touch enrollment
│   ├── Pre-shared enrollment token from portal
│   ├── Auto-enroll on first service start
│   ├── No user interaction until MFA prompt
│   └── Pre-provision config via portal API
│
├── [28] Network features
│   ├── Split tunneling (route corporate only)
│   ├── Kill switch (block non-VPN on disconnect)
│   ├── DNS leak prevention (NRPT rules)
│   └── Auto-reconnect on network change
│
└── [29] System tray UI (Tauri)
    ├── Connect/disconnect button
    ├── Gateway selection (closest/manual)
    ├── Connection status + stats
    ├── Settings (auto-connect, kill switch toggle)
    └── "Open portal" link
```

### Phase 3 Milestone

```
✓ MSI deploys silently via Group Policy to 100+ machines
✓ Zero-touch: machines auto-enroll and connect
✓ Split tunneling + kill switch working
✓ System tray UI for end users
```

---

## Phase 4: Production Hardening

**Goal:** Production-ready with monitoring, HA, and security hardening.

```
Week 10-11:
├── [30] High availability
│   ├── Multiple gateways (Seoul + Busan)
│   ├── Client failover between gateways
│   ├── Health check + auto-failover
│   └── PostgreSQL replication for portal
│
├── [31] Monitoring
│   ├── Prometheus metrics from gateway agent
│   │   ├── wireguard_peers_connected
│   │   ├── wireguard_bytes_rx / wireguard_bytes_tx
│   │   ├── wireguard_handshakes_total
│   │   └── wireguard_last_handshake_seconds
│   ├── Grafana dashboard
│   ├── Alert rules (gateway down, peer count spike, bandwidth anomaly)
│   └── Client-side log shipping (optional)
│
├── [32] Security hardening
│   ├── Portal: rate limiting, CSRF, CSP headers
│   ├── API: input validation, JWT rotation
│   ├── Gateway: nftables hardening, sysctl tuning
│   ├── Client: certificate pinning, binary signing
│   └── Penetration test checklist
│
├── [33] Performance tuning
│   ├── Gateway: UDP buffer sizes, NAPI tuning
│   ├── Client: Wintun ring buffer sizing
│   ├── MTU optimization (1420 default, test path MTU)
│   └── Benchmark: throughput, latency, concurrent peers
│
└── [34] Documentation
    ├── Admin guide (install, configure, troubleshoot)
    ├── User guide (connect, enroll, FAQ)
    ├── API reference (OpenAPI/Swagger)
    └── Runbook (incident response, key compromise)
```

### Phase 4 Milestone

```
✓ HA gateway failover tested
✓ Grafana dashboard showing real-time VPN metrics
✓ Security audit complete
✓ >500 Mbps per client, 10K concurrent peers per gateway
✓ Documentation complete
```

---

## Technology Stack Summary

| Component | Crate / Tool | Version |
|-----------|-------------|---------|
| VPN Protocol | boringtun | 0.6 |
| Windows TUN | wintun | 0.5 |
| Async Runtime | tokio | 1.x |
| HTTP Server | axum | 0.7 |
| HTTP Client | reqwest | 0.12 |
| Database | sqlx + PostgreSQL | 0.8 |
| Password hashing | argon2 | 0.5 |
| TOTP | totp-rs | 5.x |
| JWT | jsonwebtoken | 9.x |
| Serialization | serde + serde_json | 1.x |
| CLI | clap | 4.x |
| Logging | tracing | 0.1 |
| Windows Service | windows-service | 0.7 |
| Windows API | windows-rs | 0.58 |
| Desktop UI | tauri | 2.x |
| Installer | WiX Toolset | 4.x |
| Frontend | React + TypeScript | 18.x |
| Monitoring | Prometheus + Grafana | - |

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Wintun driver signing | High — unsigned drivers blocked on Win 11 | Use official signed wintun.dll from wintun.net |
| BoringTun Windows stability | Medium — less tested than Linux | Extensive testing, fallback to wireguard-go |
| DPAPI key recovery | Low — keys tied to machine account | Document backup procedure, portal can re-issue keys |
| GPO deployment failures | Medium — MSI compatibility | Test on Win 10/11 variants, SCCM as backup |
| DB connection failure | Low — auth unavailable | Connection pooling (sqlx), retry logic |
| Gateway overload | Medium — performance degradation | Auto-scaling, load balancing, rate limiting |
