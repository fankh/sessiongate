# SessionGate Implementation Status

## Implemented

- Rust/Axum browser portal service with a static responsive interface.
- PostgreSQL-backed target, policy, user binding, session, and audit storage,
  with automatic migrations and a fail-closed bootstrap policy.
- Authenticated administration APIs for creating targets, policies, and user
  bindings; disabled objects and unbound targets are never launchable.
- Bearer authentication boundary for the lab API.
- Default-deny directional clipboard, upload, download, printing, audio, and
  microphone policy.
- NLA-only RDP configuration with mandatory SHA-256 certificate pin.
- Apache Guacamole 1.6 JSON authentication payload generation using HMAC-SHA256
  followed by AES-128-CBC as required by Guacamole.
- Launch assertions limited to 30 seconds by the API and 60 seconds by the
  library contract.
- Deterministic effective-policy selection by binding priority, policy priority,
  and stable binding ID, with one visible result per target.
- Origin enforcement on every mutating API request.
- Ephemeral browser-to-portal RDP credential handoff. Credentials are validated,
  included only in the encrypted 30-second Guacamole assertion, cleared from
  the browser input, and never persisted or logged by the portal.
- FreeRDP certificate-store state redirected to an ephemeral tmpfs while guacd's
  root filesystem remains read-only.
- SHA-256 certificate pins normalized to FreeRDP's required colon-separated
  fingerprint format.
- Isolated Guacamole/guacd Compose topology with loopback-only published ports,
  read-only filesystems, dropped capabilities, and a dedicated guacd control
  network.
- Unit tests for secure defaults, directional controls, validation, and encrypted
  assertion block alignment.

## Intentionally incomplete

- The shared lab bearer boundary must be replaced by the planned portal MFA/JWT
  session and separate administrator authorization before production use.
- Guacamole encrypted JSON assertions can be replayed until their short expiry.
  Strict one-time launch requires the planned custom Guacamole authentication
  extension.
- Administration currently exposes create operations only; update, disable,
  delete, approval, and operator UI workflows remain pending.
- Browser-entered credentials are a lab flow; production vault or credential-
  broker integration and TLS termination are pending.
- Recording storage, approvals, session termination, SIEM export, and production
  reverse-proxy TLS are pending.
- Container images use exact versions but not immutable digests yet.

This is a runnable security-focused vertical slice, not a production-ready remote
access service.
