# SessionGate Architecture

## Purpose

SessionGate brokers narrowly authorized browser sessions to private Windows RDP
destinations. It is not a network VPN: users receive a remote display for an
assigned target, not general connectivity to the target network.

## Runtime topology

```text
Untrusted browser
       │ HTTPS
       ▼
  Caddy edge
       ├──────────────► Rust/Axum portal ─────► PostgreSQL
       │                        │
       │                        └─ encrypted 30-second assertion
       ▼
Apache Guacamole ──────────────► guacd ───────► Windows RDP target
```

| Component | Trust role |
|---|---|
| Caddy | Public TLS boundary, response headers, request-size limits, routing |
| Portal | Identity, authorization, policy resolution, assertion issuance, audit |
| PostgreSQL | Durable control-plane and audit state |
| Guacamole | Validates launch assertions and serves the browser client |
| guacd | Data-plane proxy with access to approved RDP networks |
| Windows target | NLA-authenticated destination with a pinned certificate |

Only Caddy is intended to be public. PostgreSQL and the guacd control network
are internal. Direct portal and Guacamole ports are bound to loopback for local
diagnostics and must not be published on a hosted server.

## Identity and authorization

SessionGate stores salted password hashes and server-side session records.
Accounts configured with a TOTP secret must provide a valid code. Authorization
is evaluated from persisted targets, policies, and user/role bindings; a browser
cannot nominate an arbitrary destination or elevate a redirection control.

The portal supports these management boundaries:

- administrators manage configuration and access assignments;
- auditors receive read-only visibility;
- users see and launch only assigned destinations.

## Session launch

1. The browser authenticates and requests its assigned destinations.
2. The user selects an assigned target and submits temporary Windows credentials.
3. The portal resolves the highest-priority applicable binding and policy.
4. The portal validates the enabled target, network zone, certificate pin, and
   session constraints.
5. It creates an encrypted Apache Guacamole JSON assertion that expires after
   30 seconds and contains only server-selected connection parameters.
6. Guacamole validates the assertion and asks guacd to establish NLA-protected
   RDP to the approved target.
7. Session state and security events are persisted for audit delivery.

Credentials are never returned by management APIs. The browser clears the
temporary Windows password after launch. Production deployments should replace
browser-entered credentials with a supported vault or credential broker.

## Policy model

Redirection controls are independent and default to deny:

- clipboard copy from remote;
- clipboard paste to remote;
- drive and file transfer;
- printer redirection;
- audio input and output;
- device and smart-card redirection;
- recording.

The portal maps only effective server-side policy into the launch assertion.
Unknown targets, malformed assertions, expired assertions, and missing policy
fail closed.

## Data model

PostgreSQL migrations define users, authentication sessions, RDP targets,
credential references, policies, bindings, remote sessions, audit events, and
the SIEM outbox. Migrations run automatically when the portal starts.

Credential references contain provider metadata, never readable secret values.
Audit delivery uses a transactional outbox so a security event is committed in
the same transaction as the state change it describes.

## Network boundaries

Compose separates four paths:

- `frontend`: Caddy, portal, and Guacamole HTTP traffic;
- `database_control`: portal-to-PostgreSQL traffic only;
- `guacd_control`: Guacamole-to-guacd protocol traffic only;
- `rdp_egress`: guacd access to approved destination networks.

Production firewall policy should restrict `rdp_egress` to explicitly managed
RDP targets and block lateral or general internet access.

## Availability and scale

The current Compose profile is a single-host deployment. PostgreSQL provides
durable state, but automated failover, multi-replica coordination, external
secret brokering, and formal load qualification remain release work. See
[ROADMAP.md](ROADMAP.md) for current readiness boundaries.
