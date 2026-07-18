# SessionGate Security Assessment

## 1. Vendors Using Wintun Driver

Wintun is the de facto standard TUN driver for Windows VPN clients. Major vendors using it:

| Vendor | Product | Protocol | Wintun Usage | Notes |
|--------|---------|----------|-------------|-------|
| **WireGuard** | WireGuard for Windows | WireGuard | Native (creator) | Wintun was built for WireGuard |
| **Cloudflare** | WARP (1.1.1.1) | WireGuard (BoringTun) | Yes | Millions of Windows users |
| **Tailscale** | Tailscale | WireGuard | Yes | Mesh VPN, enterprise SSO |
| **Mullvad** | Mullvad VPN | WireGuard | Yes | Privacy-focused, partnered with Tailscale |
| **NordVPN** | NordLynx | WireGuard | Yes | Largest consumer VPN, WireGuard wrapper |
| **OpenVPN** | OpenVPN 2.5+ | OpenVPN | Yes (optional) | Replaced legacy TAP driver |
| **AdGuard** | AdGuard VPN | Custom (WireGuard-based) | Yes | DNS filtering + VPN |
| **VPN.ac** | VPN.ac Client v4.4+ | OpenVPN / WireGuard | Yes | Enterprise VPN |
| **OpenConnect** | OpenConnect | Cisco AnyConnect / Pulse / F5 | Yes | Open-source multi-protocol |
| **Defguard** | Defguard Client | WireGuard | Yes | Enterprise WireGuard + MFA |
| **Firezone** | Firezone | WireGuard | Yes (BoringTun fork) | Open-source Zero Trust |

**Conclusion:** Wintun is battle-tested across billions of device-installs. Using it is the industry standard, not a risk.

---

## 2. Vulnerability Assessment

### 2.1 CVE-2024-3661 — TunnelVision (CVSS 7.6)

**Affects ALL routing-based VPNs** (WireGuard, OpenVPN, IPSec)

| Field | Detail |
|-------|--------|
| Vector | DHCP option 121 route injection on local network |
| Impact | Traffic sent outside VPN tunnel (full bypass) |
| Platforms | Windows, Linux, macOS (NOT Android) |
| Prerequisite | Attacker on same local network (rogue DHCP server) |

**Attack flow:**
```
Attacker (rogue DHCP) → injects option 121 static route →
  OS adds higher-priority route → traffic bypasses VPN tunnel →
  Attacker reads/modifies unencrypted traffic
```

**Our mitigations:**

| Mitigation | Layer | Implementation |
|------------|-------|----------------|
| Kill switch | Client | Windows Firewall rules block all non-VPN traffic |
| DHCP snooping | Network | Switch-level DHCP validation |
| Ignore option 121 | Client | Drop DHCP option 121 when VPN active |
| Network namespace | Server | Linux namespace isolates routing table |
| Full tunnel mode | Policy | Route 0.0.0.0/0 through VPN (no split tunnel bypass) |

### 2.2 WireGuard Protocol — Design Limitations

These are not CVEs but architectural gaps that must be addressed by the management layer:

| Limitation | Risk | Our Mitigation |
|------------|------|----------------|
| **No user authentication** — only public key (device-level) | High — stolen key = full access | Management portal adds DB credentials + TOTP MFA |
| **No access control** — AllowedIPs is static, no user groups | Medium — all peers get same access | Per-user AllowedIPs from DB user role → policy mapping |
| **No key revocation** — no CRL or OCSP equivalent | Medium — can't instantly revoke | Portal pushes peer removal to gateway in real-time |
| **No session expiry** — tunnel lives until key rotates (2 min internal) | Low — persistent access | Portal-enforced re-auth after 8 hours |
| **No logging** — WireGuard logs nothing by default | Medium — no audit trail | Gateway agent logs all peer connections to audit DB |
| **Static IP allocation** — no DHCP, IPs assigned per-peer | Low — IP management at scale | Portal manages IP pool, auto-assigns from subnet |
| **UDP only** — blocked by some corporate firewalls | Medium — connectivity issues | Obfuscation layer (optional) or TCP fallback proxy |
| **No NAT traversal** — needs direct UDP path or port forwarding | Low — most NATs handle UDP | Persistent keepalive (25s) keeps NAT mappings alive |

### 2.3 BoringTun (Cloudflare) — Userspace Risks

| Risk | Severity | Detail |
|------|----------|--------|
| Userspace vs kernel | Low | Slightly more attack surface than kernel module; but same crypto |
| No known CVEs | — | No CVEs filed as of March 2026 |
| Deployed at scale | — | Millions of devices (Cloudflare WARP iOS/Android/Windows/macOS) |
| Rust memory safety | — | No buffer overflow class of bugs |
| Dependency supply chain | Low | Minimal dependencies, auditable |

### 2.4 Wintun Driver — Kernel Risks

| Risk | Severity | Detail |
|------|----------|--------|
| Kernel driver attack surface | Low | Minimal TUN driver (~2K LOC), well-audited |
| No known CVEs | — | No CVEs filed against wintun |
| Driver signing | — | Microsoft-signed, no bypass needed |
| Shared driver conflicts | Low | OpenVPN and WireGuard can conflict on same machine |
| Windows version compat | Low | Supports Windows 7/8/8.1/10/11 |

**Note:** OpenVPN's `ovpn-dco-win` driver had CVE-2025-50054 (buffer overflow), but that's a different driver, not wintun.

### 2.5 DPAPI Key Storage

| Risk | Severity | Detail |
|------|----------|--------|
| SYSTEM-level key access | Medium | Any process as LocalSystem can decrypt `CRYPTPROTECT_LOCAL_MACHINE` keys |
| Offline attack | Medium | If disk is not encrypted, keys extractable from offline disk |
| Local admin access | Medium | Local admins can impersonate LocalSystem |

**Mitigations:**

```
1. Use user-level DPAPI (CRYPTPROTECT_LOCAL_MACHINE = false)
   → Key tied to logged-in user, not machine
   → Requires user to be logged in for VPN to work

2. Require BitLocker / disk encryption
   → Prevents offline key extraction
   → Client posture check verifies disk_encrypted = true

3. Additional entropy parameter in CryptProtectData
   → Adds app-specific secret to encryption
   → Attacker needs both DPAPI master key + entropy

4. TPM binding (Windows 10/11)
   → Store entropy in TPM sealed to PCR measurements
   → Hardware-bound, non-extractable
```

### 2.6 Management Portal Attack Surface

| Vector | Risk | Mitigation |
|--------|------|------------|
| API authentication bypass | High | JWT with short expiry (8h), refresh token rotation |
| Credential stuffing | Medium | Bcrypt/Argon2 password hashing, rate limiting |
| SQL injection | High | sqlx with parameterized queries (Rust) |
| XSS | Medium | React auto-escaping, CSP headers |
| CSRF | Medium | SameSite cookies, CSRF tokens |
| Brute force login | Medium | Rate limiting (10 attempts/min), account lockout |
| Privilege escalation | High | Role-based access, principle of least privilege |
| API key leakage | Medium | Keys in DPAPI, never logged, rotatable |

---

## 3. Threat Model

### 3.1 Attack Scenarios

| # | Scenario | Attacker | Impact | Mitigation |
|---|----------|----------|--------|------------|
| T1 | Stolen device with VPN keys | Physical access | Full VPN access until revoked | DPAPI + BitLocker, remote device revocation via portal |
| T2 | Rogue DHCP on public WiFi | Same network | Traffic bypass (TunnelVision) | Kill switch, ignore DHCP opt 121 |
| T3 | Compromised credentials | Remote | VPN access if no MFA | TOTP MFA required for all users |
| T4 | Insider threat (admin) | Portal admin | Full infrastructure access | Audit logging, dual-admin approval for policy changes |
| T5 | Gateway compromise | Remote/APT | Decrypt all peer traffic | Gateway hardening, network segmentation, key rotation |
| T6 | Supply chain (malicious wintun.dll) | Build pipeline | Kernel-level code execution | Hash verification, signed DLL from wintun.net only |
| T7 | Man-in-the-middle during roaming | Active network | Endpoint IP hijack (traffic stays encrypted) | Persistent keepalive re-establishes quickly |
| T8 | Key extraction from memory | Local privilege escalation | Private key stolen | DPAPI, minimize key lifetime in memory, secure zeroing |

### 3.2 Trust Boundaries

```
┌─────────────────────────────────────────────┐
│              TRUSTED ZONE                    │
│                                              │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐ │
│  │Management│  │  VPN     │  │ Corporate │ │
│  │ Portal   │  │ Gateway  │  │ Network   │ │
│  └────┬─────┘  └────┬─────┘  └───────────┘ │
│       │              │                       │
└───────┼──────────────┼───────────────────────┘
        │              │
  ══════╪══════════════╪══════  TRUST BOUNDARY (WireGuard encryption)
        │              │
┌───────┼──────────────┼───────────────────────┐
│       │         UNTRUSTED                    │
│  ┌────┴─────┐  ┌────┴─────┐                 │
│  │ HTTPS    │  │ WireGuard│                  │
│  │ (TLS 1.3)│  │ (UDP)   │  ← Internet     │
│  └────┬─────┘  └────┬─────┘                 │
│       │              │                       │
│  ┌────┴──────────────┴─────┐                 │
│  │     Windows Client      │                 │
│  │  (partially trusted)    │                 │
│  └─────────────────────────┘                 │
│                                              │
│  ┌─────────────────────────┐                 │
│  │  Local Network (WiFi)   │  ← TunnelVision│
│  │  (untrusted)            │     attack here │
│  └─────────────────────────┘                 │
└──────────────────────────────────────────────┘
```

---

## 4. Security Checklist

### Pre-Deployment

- [ ] Verify wintun.dll hash matches official release from wintun.net
- [ ] Enable BitLocker requirement in device posture policy
- [ ] Configure DHCP snooping on all network switches
- [ ] Set up kill switch as default-on in client config
- [ ] Enable MFA for all VPN users (no exceptions)
- [ ] Configure key rotation schedule (90 days device keys, 30 days PSK)
- [ ] Set JWT token expiry to 8 hours
- [ ] Enable audit logging on portal and gateways
- [ ] Rate limit portal API (10 req/min for auth endpoints)
- [ ] Pin portal TLS certificate in client

### Ongoing Operations

- [ ] Monitor for failed MFA attempts (alert on > 5/hour per user)
- [ ] Review audit logs weekly for anomalous access patterns
- [ ] Rotate gateway keys quarterly
- [ ] Update BoringTun and wintun.dll when new versions release
- [ ] Run penetration test annually
- [ ] Review user role → policy mappings monthly
- [ ] Verify device posture compliance (OS patched, AV active, disk encrypted)
- [ ] Test kill switch effectiveness quarterly
- [ ] Test gateway failover quarterly

---

## 5. Comparison: Our Stack vs Competitors

| Security Feature | Our Solution | Tailscale | Defguard | OpenVPN |
|-----------------|-------------|-----------|----------|---------|
| Protocol | WireGuard | WireGuard | WireGuard | OpenVPN/WireGuard |
| User auth | DB credentials + TOTP | SSO (Google/MS/Okta) | LDAP + WebAuthn | RADIUS/LDAP |
| MFA | TOTP | SSO provider MFA | Native 2FA | Plugin |
| Key storage (Win) | DPAPI | DPAPI | DPAPI | File-based |
| Kill switch | Yes | No (mesh VPN) | Yes | Yes |
| Split tunnel | Yes | Yes (ACLs) | Yes | Yes |
| Audit log | Yes | Yes | Yes | syslog |
| Device posture | Yes | Yes (premium) | No | No |
| Zero-trust ACL | DB role-based | Tailnet ACLs | LDAP groups | RADIUS attributes |
| Self-hosted | Yes | No (SaaS control plane) | Yes | Yes |
| Open source | Yes (MIT) | Partial (client only) | Yes (Apache 2.0) | Yes (GPL) |
