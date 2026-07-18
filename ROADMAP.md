# SessionGate Roadmap

This document consolidates the former implementation status, browser-RDP plan,
production decisions, and priority plan. It describes current capability rather
than promising a release date.

## Implemented baseline

- Rust/Axum portal and static browser UI
- PostgreSQL migrations and persisted control-plane state
- Built-in users, password hashing, server-side sessions, and optional TOTP
- Administrator, auditor, and user authorization boundaries
- RDP destinations, policies, assignments, and credential references
- Default-deny redirection controls
- NLA and RDP certificate fingerprint enforcement
- Encrypted, short-lived Guacamole assertions
- Caddy HTTPS edge and hardened Docker Compose topology
- Multi-user login, target selection, and full-screen remote workspace
- Audit records and vendor-neutral SIEM outbox contract
- Real Edge, Guacamole, PostgreSQL, and Hyper-V integration procedure

## Accepted product decisions

| Area | Decision |
|---|---|
| Identity | Built-in account/password/TOTP support without an external identity dependency |
| Local MFA | Login is allowed without OTP only when an account has no TOTP secret |
| TLS | Caddy at the public edge |
| Session duration | Maximum eight hours, with shorter policy values allowed |
| Recording | Not required for the product baseline |
| Object storage | MinIO-compatible storage if future artifacts require it |
| SIEM | Transactional outbox to the operator's existing SIEM |
| Qualification | Local containers plus a real Edge browser and Windows Hyper-V VM |

## Release priorities

### P0 — Safe public deployment

- replace bootstrap/lab-token administration with complete authenticated UI flows;
- add account lifecycle, password rotation, lockout, recovery, and TOTP enrollment;
- complete CSRF and authorization coverage for every management mutation;
- publish a supported public-hostname Caddy profile and secret-management path;
- add automated CI for tests, linting, container build, and dependency review.

### P1 — Credential and session lifecycle

- integrate and qualify one external credential provider;
- ensure credentials are injected without disclosure to browser users;
- enforce server-side idle, maximum-duration, revoke, and disconnect behavior;
- reconcile portal state with Guacamole/guacd connection state;
- add administrator session visibility and termination controls.

### P2 — Reliability and observability

- deliver the SIEM outbox worker with retry, idempotency, and backlog alerts;
- add structured metrics for authentication, launch, latency, and failure reasons;
- test PostgreSQL backup, restore, and migration rollback procedures;
- qualify concurrent users, reconnect behavior, and resource limits;
- define supported browser, Windows, PostgreSQL, and container versions.

### P3 — Release governance

- establish private vulnerability reporting and response targets;
- add an Apache-2.0 repository license file and contributor guidance;
- produce a signed versioned release and software bill of materials;
- complete an independent security review and remediate release blockers.

## Production exit gates

A production release requires all P0 controls, a qualified credential path,
enforceable session termination, reliable audit export, backup restoration,
documented capacity limits, and a complete real-browser/VM regression pass.
