# SessionGate

**Secure remote desktops, directly in your browser.**

SessionGate is a self-hosted, open-source gateway designed to give users
production-grade, controlled browser access to assigned Windows desktops. RDP
remains private; users enter through an HTTPS portal where identity, target
assignment, session policy, and audit controls are enforced centrally.

> **Release status: pre-production.** The implemented baseline is tested with
> real containers, Microsoft Edge, and a Windows Hyper-V VM. Internet-facing
> production use requires the release gates below. SessionGate is being built
> toward those gates rather than treating the local Compose profile as a
> production deployment.

## Production-readiness target

SessionGate's production target is a least-privilege remote-access service with
a single public HTTPS entry point, private RDP destinations, credential
isolation, enforceable session lifecycle, reliable audit delivery, tested
recovery, and documented capacity limits.

| Capability | Current state | Production gate |
|---|---|---|
| HTTPS edge and private service networks | Implemented | Public hostname and trusted certificate qualified |
| Built-in identity, roles, and TOTP enforcement | Implemented baseline | Complete enrollment, recovery, lockout, and rotation flows |
| Target assignment and default-deny policy | Implemented and tested | Full mutation authorization and regression coverage |
| NLA and RDP certificate pinning | Implemented and VM-tested | Certificate lifecycle procedure approved |
| Credential handling | Temporary credentials and write-only references | One external broker or vault qualified |
| Session lifecycle | Launch and persistence implemented | Idle timeout, maximum duration, revoke, and disconnect enforced |
| Audit and SIEM | Events and transactional outbox contract implemented | Delivery worker, retry, idempotency, and backlog alerts qualified |
| Backup and reliability | Persistent PostgreSQL volume documented | Restore test, rollback procedure, and failure drills passed |
| Scale | Functional and latency tests completed | Concurrent-user limits and resource envelopes published |
| Release governance | Tests and security guidance present | CI, SBOM, signed release, license file, and security review complete |

The authoritative work sequence and exit criteria are maintained in
[ROADMAP.md](ROADMAP.md). A release is production-ready only after every stated
exit gate has objective test evidence.

## Why SessionGate?

- No RDP port exposed to the user's network or the public internet
- No desktop client, browser extension, or external identity service required
- Built-in multi-user login with TOTP enforcement for configured accounts
- Per-user target assignments and server-enforced session policies
- Default-deny clipboard, drive, printer, audio, and device redirection
- NLA enforcement and pinned RDP certificate fingerprints
- Short-lived encrypted Apache Guacamole connection assertions
- PostgreSQL-backed configuration, sessions, and audit events
- Full-screen browser workspace with remote keyboard shortcut handling
- Containerized deployment with a hardened Caddy HTTPS edge

## Architecture

```text
Browser
   │ HTTPS
   ▼
Caddy ──► SessionGate portal ──► PostgreSQL
   │              │
   │              └── signed, short-lived connection assertion
   ▼
Apache Guacamole ──► guacd ──► approved Windows RDP target
```

| Component | Responsibility |
|---|---|
| Caddy | TLS termination, security headers, and reverse proxying |
| Rust/Axum portal | Authentication, authorization, policy, launch, and audit control plane |
| PostgreSQL | Users, destinations, policies, assignments, sessions, and audit data |
| Apache Guacamole | Browser remote-desktop client and assertion validation |
| guacd | Isolated RDP protocol proxy to approved target networks |

See [ARCHITECTURE.md](ARCHITECTURE.md) for trust boundaries and component
details.

## Local evaluation

### Prerequisites

- Docker Engine 24+ with Docker Compose v2, or current Docker Desktop
- An NLA-enabled Windows RDP target reachable from Docker
- The target's SHA-256 RDP certificate fingerprint
- Git

### 1. Configure

```sh
git clone https://github.com/fankh/sessiongate.git
cd sessiongate
cp .env.example .env
```

PowerShell users can run `Copy-Item .env.example .env` instead of `cp`.

Replace every placeholder in `.env`. For the local Compose profile, set:

```dotenv
PORTAL_ALLOWED_ORIGIN=https://localhost:18443
RDP_TARGET_HOST=<target IP or hostname>
RDP_CERTIFICATE_SHA256=<64-character SHA-256 fingerprint>
```

Generate independent random values for the bearer token, 32-hex-character
Guacamole key, and PostgreSQL password. The
[container deployment guide](docs/CONTAINER-DEPLOYMENT.md) provides Linux and
PowerShell generation commands plus optional administrator bootstrap settings.

### 2. Start

```sh
docker compose config --quiet
docker compose up -d --build
docker compose ps
```

The portal waits for PostgreSQL, applies database migrations automatically, and
bootstraps the configured lab access.

### 3. Verify and open

```sh
curl -kfsS https://localhost:18443/healthz
```

Open <https://localhost:18443/login.html>. Caddy uses an internal certificate
for the local profile, so the browser may show a trust warning. Do not expose
the local diagnostic ports or use the development certificate for a public
deployment.

To follow startup logs:

```sh
docker compose logs -f --tail=100 portal caddy guacamole guacd database
```

To stop the stack without deleting persistent data:

```sh
docker compose down
```

## Production deployment

The checked-in Compose profile deliberately publishes services on loopback and
uses Caddy's internal certificate for `localhost`. Before deploying on a hosted
server:

1. Configure a public DNS name and publicly trusted HTTPS certificate.
2. Publish only TCP 443; keep portal, Guacamole, guacd, PostgreSQL, and RDP
   diagnostic/control ports private.
3. Store independent secrets outside the repository and remove bootstrap
   credentials after first use.
4. Restrict guacd egress to explicitly approved Windows targets.
5. Configure privileged-user TOTP and operational account recovery.
6. Connect and monitor the transactional audit outbox with the operator's SIEM.
7. Run backup restoration and the complete browser/VM regression procedure.
8. Record capacity limits, incident procedures, and rollback approval.

Follow the [container deployment guide](docs/CONTAINER-DEPLOYMENT.md) for the
hosted Caddy configuration, persistence, backup, restore, upgrade, and rollback
procedures. The guide is an operational starting point; the gates in
[ROADMAP.md](ROADMAP.md) determine release approval.

## Configuration reference

| Variable | Required | Purpose |
|---|---:|---|
| `PORTAL_BEARER_TOKEN` | Yes | Lab API token; use at least 32 random characters |
| `PORTAL_USER` | Yes | Subject receiving the bootstrapped lab assignment |
| `PORTAL_ALLOWED_ORIGIN` | Yes | Exact browser origin accepted for state-changing requests |
| `GUACAMOLE_JSON_SECRET_KEY` | Yes | Shared 32-hex-character Guacamole assertion key |
| `POSTGRES_PASSWORD` | Yes | PostgreSQL application password |
| `RDP_TARGET_HOST` | Yes | Approved Windows target IP address or hostname |
| `RDP_CERTIFICATE_SHA256` | Yes | Pinned 64-character RDP certificate fingerprint |
| `RDP_DOMAIN` | No | Windows domain supplied for the lab target |
| `PORTAL_BOOTSTRAP_USERNAME` | No | Initial management user |
| `PORTAL_BOOTSTRAP_PASSWORD` | With username | Initial management-user password |
| `PORTAL_BOOTSTRAP_TOTP_HEX` | No | Initial TOTP secret; omit to allow login before MFA enrollment |

Never commit `.env`; it is excluded by `.gitignore`. See
[CONTAINER-DEPLOYMENT.md](docs/CONTAINER-DEPLOYMENT.md) for hosted TLS,
persistence, backup, restore, upgrade, and troubleshooting procedures.

## Security model

SessionGate is designed around explicit assignment and default-deny policy:

1. A user authenticates to the portal.
2. The portal returns only destinations assigned to that identity.
3. A launch request is evaluated against target and policy constraints.
4. The portal creates a short-lived encrypted Guacamole assertion.
5. Guacamole and guacd connect to the pinned, NLA-enabled RDP destination.
6. Session lifecycle and security events are persisted for audit delivery.

Browser-supplied hostnames, ports, policy flags, and credentials are not treated
as authorization. Review [SECURITY.md](SECURITY.md) before deployment and report
security issues through a private maintainer channel rather than a public issue.

## Development and testing

Run the Rust checks:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The integration guide covers containers, a real Edge browser, Hyper-V,
certificate pinning, NLA, policy controls, audit evidence, and performance:

- [Testing guide](docs/TESTING.md)
- [Consolidated test results](docs/TEST-RESULTS.md)

## Documentation

### Operate SessionGate

- [Container deployment](docs/CONTAINER-DEPLOYMENT.md)
- [Management API](docs/MANAGEMENT-API.md)
- [SIEM integration](docs/SIEM-INTEGRATION.md)
- [Security model](SECURITY.md)
- [Product roadmap and readiness](ROADMAP.md)

### Design and implementation

- [System architecture](ARCHITECTURE.md)
- [Product roadmap and readiness](ROADMAP.md)
- [Management API contract](docs/MANAGEMENT-API.md)

### Test evidence

- [Testing guide](docs/TESTING.md)
- [Consolidated test results](docs/TEST-RESULTS.md)

## Contributing

Issues and focused pull requests are welcome. For behavior changes:

1. Explain the security and compatibility impact.
2. Add or update tests.
3. Run `cargo test --workspace` and Clippy.
4. Update the relevant architecture, API, or deployment documentation.

Avoid committing credentials, `.env`, generated reports, VM disks, recordings,
or private infrastructure details.

## License

SessionGate source metadata declares the Apache License 2.0. A repository-level
license file should be included before the first formal release.
