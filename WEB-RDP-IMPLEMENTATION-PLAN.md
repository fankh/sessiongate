# SessionGate Browser RDP Implementation Plan

## 1. Outcome and scope

Add browser-based Windows Remote Desktop access to the VPN management portal.
Users authenticate to the existing portal, select an authorized Windows target,
and receive an HTML5 remote desktop without exposing TCP/3389 to the Internet.
Administrators control clipboard direction, file transfer, printing, audio,
microphone, session recording, concurrent sessions, and session duration through
centrally enforced policy.

This plan extends the documented Rust/Axum, React, PostgreSQL, WireGuard, and
TOTP/WebAuthn architecture. The repository currently contains design documents
only, so implementation begins with the portal foundation already described in
`IMPLEMENTATION-PLAN.md`.

## 2. Architecture decision

Use **Apache Guacamole 1.6.x**, pinned to an exact tested patch and container
digest, as the browser-to-RDP data plane:

```text
Browser
  | HTTPS/WSS, portal session, CSRF/origin checks
  v
Reverse proxy / WAF
  |-----------------------> React management portal
  |                              |
  |                              v
  |                         Axum policy API ---- PostgreSQL
  |                              |
  |                    short-lived signed launch assertion
  v                              v
Guacamole web application ---- guacd
                                  |
                         RDP/NLA over private network
                                  |
                         Authorized Windows target
```

Guacamole is preferred over implementing RDP in browser code. Its JavaScript
client transports display and input through the Guacamole protocol, while
`guacd` terminates that protocol and acts as the RDP client. The initial portal
integration uses Guacamole encrypted JSON authentication for short-lived,
single-use connection assertions. A custom Guacamole authentication extension
is a later option if JSON authentication cannot meet revocation or audit needs.

The Guacamole web application and `guacd` run in separate containers and network
segments. Only the web application may reach `guacd`; only `guacd` may reach
approved target TCP/3389 addresses. Neither service is a general VPN gateway.

## 3. Security policy model

Security-sensitive capabilities are default-deny. Portal policy is resolved at
session launch from user, group, device posture, target, and environment rules.
Users cannot supply raw Guacamole/RDP connection parameters.

| Portal policy | Guacamole enforcement | Secure default |
|---|---|---|
| Remote-to-browser clipboard | `disable-copy` | Disabled |
| Browser-to-remote clipboard | `disable-paste` | Disabled |
| File-transfer drive | `enable-drive` | Disabled |
| Download from remote | `disable-download` | Disabled |
| Upload to remote | `disable-upload` | Disabled |
| Virtual PDF printer | `enable-printing` | Disabled |
| Remote audio output | `disable-audio` | Disabled |
| Browser microphone | `enable-audio-input` | Disabled |
| Session recording | `recording-path` and immutable recording name | Required for privileged targets; policy-controlled otherwise |
| Keyboard-event logging | `recording-include-keys` | Disabled because it can capture secrets |
| RDP authentication | `security=nla` | Required |
| Certificate validation | `cert-fingerprints` or trusted PKI | Required; `ignore-cert=false` |

Additional policy fields:

- maximum duration, idle timeout, and absolute expiration;
- maximum concurrent sessions per user and target;
- allowed source CIDRs, countries, schedules, and device-posture levels;
- view-only versus interactive mode;
- screen size, color depth, and bandwidth profile;
- credential source: interactive prompt, managed service credential, or vault;
- approval requirement and ticket/reference ID for privileged targets;
- mandatory recording retention class and legal-hold flag.

Clipboard controls are directional. For example, a support role may paste a
known command into a server while still being unable to copy data out. File
transfer is separate from clipboard and remains disabled unless explicitly
allowed.

## 4. Data model

Add migrations for:

```text
rdp_targets
  id, name, hostname, port, domain, certificate_fingerprint,
  network_zone, credential_ref, enabled, created_at, updated_at

rdp_policies
  id, name, priority, clipboard_in, clipboard_out,
  upload, download, printing, audio_out, microphone,
  recording_mode, max_duration_seconds, idle_timeout_seconds,
  max_concurrent_sessions, require_approval, enabled

rdp_policy_bindings
  id, policy_id, subject_type, subject_id, target_id,
  source_cidrs, schedule, device_posture, priority

rdp_sessions
  id, user_id, device_id, target_id, resolved_policy_snapshot,
  state, launch_jti_hash, source_ip, started_at, ended_at,
  termination_reason, recording_object_key

rdp_approvals
  id, session_id, requester_id, approver_id, ticket_ref,
  state, requested_at, decided_at, expires_at
```

Never store Windows passwords in these tables. Store only a reference to a
secret manager. Encrypt target metadata that is operationally sensitive and
record policy snapshots so later policy changes do not rewrite audit history.

## 5. API and session flow

### Administrative APIs

- `CRUD /api/v1/rdp/targets`
- `POST /api/v1/rdp/targets/:id/test` using a bounded server-side probe
- `CRUD /api/v1/rdp/policies`
- `CRUD /api/v1/rdp/policy-bindings`
- `GET /api/v1/rdp/sessions` and `GET /api/v1/rdp/sessions/:id`
- `POST /api/v1/rdp/sessions/:id/terminate`
- `POST /api/v1/rdp/approvals/:id/approve|deny`

### User flow

1. User authenticates to the portal with MFA.
2. Portal lists only targets for which a policy resolves to allow.
3. `POST /api/v1/rdp/sessions` validates CSRF, origin, role, device posture,
   schedule, approvals, concurrency, and target health.
4. The API creates a pending session and a 30–60 second, single-use launch
   assertion containing only server-derived Guacamole parameters.
5. The browser navigates to the embedded Guacamole client over HTTPS/WSS.
6. Guacamole validates the assertion and asks `guacd` to connect to the exact
   approved target with resolved policy parameters.
7. Connect, disconnect, policy decision, approval, timeout, and administrator
   termination events are written to the audit log.
8. Session revocation closes the WebSocket/tunnel and the guacd connection.

Launch assertions contain `aud`, `iss`, user ID, session ID, target ID, policy
snapshot hash, `iat`, `nbf`, `exp`, and random `jti`. Store only a hash of `jti`,
consume it atomically, and reject replay. Do not place RDP passwords in browser
storage, URLs, portal logs, or audit details.

## 6. Frontend plan

Add React routes and components for:

- `/remote-desktops`: authorized target cards and connection status;
- `/remote-desktops/session/:id`: full-screen client with reconnect and explicit
  disconnect controls;
- `/admin/rdp/targets`: target inventory, certificate pin, zone, and health;
- `/admin/rdp/policies`: directional clipboard and device-redirection controls
  with secure defaults and an effective-policy preview;
- `/admin/rdp/sessions`: active session monitoring and termination;
- `/admin/rdp/approvals`: privileged-session approval queue;
- `/audit`: filters for RDP launch, connect, disconnect, denial, and transfer
  policy events.

The client must show the effective restrictions before connection. Hide disabled
clipboard/upload/download UI, but treat UI hiding only as usability; enforcement
is server-side in the generated Guacamole configuration.

## 7. Delivery phases

### Phase 0 — threat model and proof of concept

- Pin Guacamole web, guacd, PostgreSQL driver, and FreeRDP-compatible images by
  digest; generate an SBOM and scan them.
- Deploy one disposable Windows evaluation VM reachable only from guacd.
- Prove NLA, certificate validation, WebSocket proxying, and all directional
  clipboard/file/audio/printing settings.
- Capture packet traces showing that browsers never connect to TCP/3389.

Exit: a repeatable local Compose lab and a completed control matrix with no
Internet-exposed RDP port.

### Phase 1 — portal foundation and target inventory

- Implement the Axum/React/PostgreSQL portal foundation and existing MFA/RBAC
  plan if not already present.
- Add target, policy, binding, session, approval, and audit migrations.
- Implement strict target validation: address allowlist, no user-controlled DNS
  rebinding, fixed port policy, certificate fingerprint, and network zone.
- Add admin CRUD and effective-policy evaluation with deterministic priority and
  deny-overrides semantics.

Exit: admins can define targets and preview resolved policy; no RDP session can
yet launch.

### Phase 2 — secure session launch

- Deploy isolated Guacamole web and guacd services.
- Implement single-use encrypted/signed launch assertions and key rotation.
- Add secret-manager integration and prefer interactive NLA credential prompts
  for the first release.
- Implement browser client route, session lifecycle, timeouts, concurrency
  limits, and administrator termination.
- Enforce clipboard and all redirection controls from immutable policy snapshots.

Exit: an MFA-authenticated user can launch one authorized browser RDP session,
and every default-deny control has an automated end-to-end test.

### Phase 3 — recording, approvals, and operations

- Store recordings in encrypted object storage with per-session object keys,
  retention, integrity hashes, restricted playback, and deletion workflows.
- Add privileged-target approval and ticket-reference workflows.
- Add Prometheus metrics for active sessions, launch failures, latency, guacd
  saturation, and target failures without high-cardinality user labels.
- Add audit export to the SIEM and alerts for repeated denials, replay attempts,
  unusual source locations, and disabled-control violations.
- Add rolling deployment, connection draining, backup/restore, and disaster
  recovery procedures.

Exit: operations can monitor, terminate, investigate, and recover sessions while
recording access is independently authorized and audited.

### Phase 4 — production qualification

- Load-test browser rendering and input latency at expected concurrency.
- Test Windows Server 2019/2022/2025 and Windows 10/11 targets with NLA.
- Test Chrome, Edge, Firefox, and Safari at supported versions.
- Run penetration tests covering SSRF, assertion replay, header spoofing,
  WebSocket origin bypass, CSRF, XSS, credential leakage, container escape,
  drive-path traversal, recording exposure, and authorization races.
- Exercise guacd/web failure, portal restart, target disconnect, policy changes,
  revocation, and recording-storage outage.

Exit: all security gates pass, performance objectives are recorded, rollback is
tested, and residual risks are accepted by the service owner.

## 8. Required tests

### Policy tests

- Every control defaults to deny when absent, null, invalid, or conflicting.
- `disable-copy` blocks remote-to-browser content while remote-internal copy still
  works; `disable-paste` independently blocks browser-to-remote content.
- Drive, upload, download, printer, audio, and microphone controls are each
  tested independently and in combinations.
- A user cannot alter target address, port, username, security mode, certificate,
  drive path, recording path, or resolved policy in browser requests.

### Security tests

- Launch assertions reject expiry, replay, wrong audience, altered ciphertext,
  rotated keys, and sessions already terminated.
- WebSocket upgrades require the authenticated cookie, expected Origin, active
  session, and matching session/user identifiers.
- RDP certificates fail closed; `ignore-cert=true`, legacy `security=rdp`, and
  public/non-approved target addresses are rejected by configuration validation.
- Clipboard and file contents never appear in portal logs, traces, metrics, or
  audit records.
- Recording paths and drive paths are server-generated opaque identifiers and
  cannot traverse directories or collide across tenants.

### Performance tests

- Measure launch latency, input latency, frame rate, bandwidth, CPU, and memory at
  1, 10, 50, 100, and planned-maximum concurrent sessions.
- Define initial objectives after Phase 0 measurement; do not claim the existing
  VPN gateway throughput targets apply to graphical sessions.

## 9. Deployment and network controls

- Expose only HTTPS/WSS through the reverse proxy; block direct access to
  Guacamole, guacd, PostgreSQL, object storage, and RDP targets.
- Place guacd in a dedicated egress segment with nftables rules generated from
  approved target inventory. Do not grant unrestricted corporate-network egress.
- Use TLS between reverse proxy and Guacamole and mTLS or a private authenticated
  channel between Guacamole and guacd where supported by the selected deployment.
- Run containers rootless/read-only where possible, drop Linux capabilities,
  apply seccomp/AppArmor, use ephemeral per-session drive directories, and set
  CPU/memory/PID/file-size limits.
- Keep browser RDP access separate from endpoint WireGuard enrollment: a user may
  be authorized for one, both, or neither.

## 10. Explicit non-goals for the first release

- General-purpose VNC, SSH, Telnet, or Kubernetes console access.
- Browser-initiated arbitrary host/port connections.
- USB, smart-card, local disk, or arbitrary device passthrough.
- Shared-session links or anonymous access.
- Keystroke logging.
- Replacing Windows authorization, NLA, endpoint hardening, or EDR.

## 11. Primary references

- [Apache Guacamole architecture](https://guacamole.apache.org/doc/gug/guacamole-architecture.html)
- [Apache Guacamole RDP and common connection parameters](https://guacamole.apache.org/doc/gug/configuring-guacamole.html)
- [Apache Guacamole encrypted JSON authentication](https://guacamole.apache.org/doc/gug/json-auth.html)
- [Apache Guacamole extension API](https://guacamole.apache.org/doc/gug/guacamole-ext.html)
