# SessionGate Usable Feature Qualification Report

Date: 2026-07-18

Environment: Windows host, Microsoft Edge 150, Docker Compose, Caddy HTTPS,
PostgreSQL 17, Guacamole/guacd 1.6.0, and Hyper-V Windows Server 2025 VM.

## Passed

| Area | Evidence |
|---|---|
| Rust | 13/13 tests; formatting and Clippy warnings-as-errors passed |
| Frontend | Login, portal, management, and RDP JavaScript syntax passed |
| Containers | Caddy, portal, Guacamole, guacd, and PostgreSQL running |
| Health | PostgreSQL and guacd healthy; portal health returned 200 |
| Pages | Login, portal, management, RDP workspace, and shared CSS returned 200 |
| HTTPS | HSTS, strict CSP, and anti-framing header present |
| Invalid login | Incorrect password returned 401 |
| Password-only login | Unenrolled administrator returned 200 |
| Session cookie | `Secure`, `HttpOnly`, and `SameSite=Strict` present |
| Current identity | `/auth/me` returned `admin` |
| Logout | Returned 204; the revoked cookie then returned 401 |
| Anonymous API | Anonymous target request returned 401 |
| Management inventory | 1 destination, 1 policy, 2 bindings, 0 credential references |
| Hyper-V | VM running, heartbeat OK, `172.31.98.16:3389` reachable |
| Direct RDP browser | Edge joined guacd; NLA passed; certificate pin accepted |
| Fullscreen wrapper | Edge joined through `/rdp.html`; NLA passed; no security failure |
| Persistence | 26 sessions, 26 allowed audits, and 13 denied audits observed |

## Implemented but not production-usable

1. Login identities are not yet used by target-list and session-launch handlers;
   those handlers still use the configured lab identity.
2. The portal and management console still expose lab bearer-token fields.
3. Runtime RBAC and CSRF enforcement are not yet connected to every route.
4. User creation, MFA enrollment/reset, role assignment, and session revocation
   do not yet have complete administrator workflows.
5. Credential references are metadata only. No secret upload, encrypted broker,
   hosted provider, or one-time Guacamole redemption is active.
6. A fresh interactive desktop logon was not observed in this automated run.
   Previous visual qualification proved the real Windows desktop and fullscreen
   state, while this run proved tunnel/NLA/certificate behavior.
7. Concurrent multi-user authorization has not been qualified and must not be
   advertised as ready.

## Current safe scope

The deployed system is usable as a single-user isolated RDP lab with a working
password login foundation and management inventory. It is not yet safe to use
as a multi-user production gateway.
