# SessionGate

**Secure remote desktops, directly in your browser.**

SessionGate is an open-source, zero-trust remote access gateway that gives
authorized users controlled browser access to assigned Windows desktops without
exposing RDP to the public network.

## Summary

| Component | Technology | Purpose |
|-----------|-----------|---------|
| HTTPS edge | Caddy | TLS termination and security headers |
| Access portal | Rust + Axum | Authentication, authorization, policy, and audit control plane |
| Browser desktop | Apache Guacamole + guacd | Isolated browser-to-RDP session transport |
| Persistence | PostgreSQL | Users, targets, policies, assignments, sessions, and audit events |
| Authentication | Password + optional TOTP MFA | Built-in, self-hosted user authentication |
| Deployment | Docker Compose | Reproducible self-hosted container stack |

## Documents

| File | Description |
|------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | System architecture and component design |
| [WINDOWS-CLIENT.md](WINDOWS-CLIENT.md) | Windows client implementation and deployment |
| [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md) | Development roadmap and milestones |
| [WEB-RDP-IMPLEMENTATION-PLAN.md](WEB-RDP-IMPLEMENTATION-PLAN.md) | Browser RDP architecture, policy controls, security gates, and delivery plan |
| [IMPLEMENTATION-STATUS.md](IMPLEMENTATION-STATUS.md) | Implemented browser-RDP vertical slice and remaining production gaps |
| [TESTING-GUIDE.md](TESTING-GUIDE.md) | Reproducible container, Hyper-V, RDP, policy, audit, and performance test procedure |
| [TEST-REPORT-2026-07-18.md](TEST-REPORT-2026-07-18.md) | Real Edge, Guacamole, PostgreSQL, and Hyper-V VM test evidence and metrics |
| [TEST-REPORT-POST-FIX-2026-07-18.md](TEST-REPORT-POST-FIX-2026-07-18.md) | Post-fix NLA, certificate pin, Windows authentication, and desktop evidence |
| [FULLSCREEN-TEST-REPORT-2026-07-18.md](FULLSCREEN-TEST-REPORT-2026-07-18.md) | Real Edge/Hyper-V fullscreen, keyboard-lock, iframe, NLA, and manual activation evidence |
| [USABLE-FEATURE-REPORT-2026-07-18.md](USABLE-FEATURE-REPORT-2026-07-18.md) | Full deployed feature qualification with explicit usable and incomplete boundaries |
| [PRODUCTION-PRIORITY-PLAN.md](PRODUCTION-PRIORITY-PLAN.md) | P0-P3 production roadmap, readiness percentages, dependencies, and exit gates |
| [PRODUCTION-DECISIONS.md](PRODUCTION-DECISIONS.md) | Accepted identity, MFA, TLS, broker, lifecycle, storage, and SIEM decisions |
| [docs/SIEM-INTEGRATION.md](docs/SIEM-INTEGRATION.md) | Vendor-neutral security-event envelope and reliable delivery contract |
| [docs/MANAGEMENT-API.md](docs/MANAGEMENT-API.md) | Authoritative identity, administration, credential, approval, and session API contract |
| [docs/CONTAINER-DEPLOYMENT.md](docs/CONTAINER-DEPLOYMENT.md) | Docker Compose deployment, TLS, secrets, validation, backup, upgrades, and server hosting |
| [SECURITY.md](SECURITY.md) | Vulnerability assessment, threat model, vendor landscape |

## SessionGate quick start

The SessionGate control plane is implemented in `portal/`. It generates
short-lived Apache Guacamole connection assertions from server-enforced,
default-deny policy and persists configuration, sessions, and audit events in
PostgreSQL.

```powershell
Copy-Item .env.example .env
# Replace every placeholder in .env with a real lab value.
docker compose config
docker compose up --build
```

Open `https://localhost:18443`, accept the local Caddy development certificate,
enter the configured lab bearer token, load the
approved target, enter temporary Windows credentials, and review the effective
controls before connecting. The password is cleared from the UI after launch
and is included only in the encrypted, 30-second Guacamole assertion. Use this
lab flow only on loopback. See [the container deployment guide](docs/CONTAINER-DEPLOYMENT.md)
for server TLS, secret handling, persistence, backup, and upgrade instructions.

Mutating API calls require both the bearer token and an `Origin` matching
`PORTAL_ALLOWED_ORIGIN`. The lab target is bootstrapped as enabled and bound to
`PORTAL_USER` with every redirection denied. Additional objects can be created
through `POST /api/v1/admin/rdp/targets`, `/policies`, and `/bindings`.

Do not expose the lab ports beyond loopback. See
`WEB-RDP-IMPLEMENTATION-PLAN.md` and `IMPLEMENTATION-STATUS.md` before any
production deployment.

## References

- [WireGuard Protocol](https://www.wireguard.com/)
- [BoringTun (Cloudflare)](https://github.com/cloudflare/boringtun)
- [Wintun Driver](https://www.wintun.net/)
- [WireGuard Windows Enterprise Docs](https://github.com/WireGuard/wireguard-windows/blob/master/docs/enterprise.md)
- [Defguard](https://github.com/DefGuard/defguard)
- [WG-Portal](https://github.com/h44z/wg-portal)
