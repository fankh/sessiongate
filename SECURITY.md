# SessionGate Security

## Reporting a vulnerability

Do not disclose vulnerabilities through a public issue. Contact the repository
owner privately with the affected version, reproduction steps, impact, and any
suggested mitigation. Avoid including real credentials, hostnames, recordings,
or customer data.

## Security properties

SessionGate is designed to provide:

- no public exposure of RDP or database ports;
- explicit user-to-target assignment;
- server-selected destinations and connection options;
- default-deny session redirection;
- NLA enforcement and SHA-256 RDP certificate pinning;
- short-lived encrypted Guacamole launch assertions;
- salted password hashing and hashed bearer tokens;
- optional TOTP enforcement per configured account;
- origin and CSRF validation for state-changing requests;
- role boundaries for administrators, auditors, and users;
- write-only credential references;
- durable audit events and transactional SIEM delivery;
- hardened containers with dropped capabilities, read-only filesystems, and
  `no-new-privileges` where supported.

These properties depend on correct deployment. They do not make an exposed
diagnostic port, weak administrator credential, untrusted RDP certificate, or
unrestricted guacd network safe.

## Trust boundaries

| Boundary | Required control |
|---|---|
| Browser to Caddy | Trusted HTTPS certificate, secure cookies, security headers |
| Caddy to portal/Guacamole | Private container network; no public direct ports |
| Portal to PostgreSQL | Internal network and independent database credentials |
| Portal to Guacamole | Independent high-entropy shared assertion key |
| Guacamole to guacd | Internal control network |
| guacd to Windows | Restricted egress, NLA, approved target, pinned certificate |
| Audit outbox to SIEM | Authenticated TLS, retry, idempotency, monitored backlog |

## Deployment requirements

Before exposing SessionGate beyond localhost:

1. Use a public hostname and a publicly trusted TLS certificate.
2. Publish only TCP 443; block 18080, 18081, 5432, 4822, and direct 3389.
3. Generate independent secrets for the database, bearer token, administrator,
   and Guacamole assertion encryption.
4. Remove bootstrap credentials from the runtime environment after first use.
5. Enable TOTP for privileged users and enforce strong password policy.
6. Restrict guacd egress to approved target subnets or addresses.
7. Validate every RDP certificate change through an administrative process.
8. Back up and restore-test PostgreSQL and protected deployment secrets.
9. Forward audit events to the existing SIEM and alert on delivery backlog.
10. Patch pinned container images through a reviewed upgrade process.

See [docs/CONTAINER-DEPLOYMENT.md](docs/CONTAINER-DEPLOYMENT.md) for commands and
[docs/SIEM-INTEGRATION.md](docs/SIEM-INTEGRATION.md) for the event contract.

## Known boundaries

The current release does not claim multi-host high availability, a fully
qualified external credential broker, formal scale certification, or completed
internet-facing production approval. Recording is intentionally not part of the
required product scope. Current gaps and exit gates are tracked in
[ROADMAP.md](ROADMAP.md).

## Maintainer verification

For each release:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
docker compose config --quiet
```

Also execute the real-browser and Windows VM procedure in
[docs/TESTING.md](docs/TESTING.md), verify default-deny policy, inspect audit
events, and test backup restoration.
