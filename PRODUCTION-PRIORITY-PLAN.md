# SessionGate Production Priority Plan

## Objective

Move the working browser-to-Windows RDP lab into a production-qualified remote
access service without weakening NLA, certificate validation, policy defaults,
or auditability.

The current implementation proves the basic data path:

```text
Browser -> portal -> Guacamole -> guacd -> Windows VM
```

Production readiness is currently estimated at **60%**. This percentage
represents completed security and operational gates, not lines of code.

## Priority model

- **P0:** A security or control-plane requirement that blocks any production use.
- **P1:** Required for a reliable supported release, after P0 gates pass.
- **P2:** Scale, usability, and operational maturity work.
- **P3:** Optional integrations and optimization.

No P1 feature should delay remediation of an open P0 gate.

## Phase 0 — Preserve the working baseline

Priority: **P0**
Estimated readiness contribution: **5%**
Target cumulative readiness: **60%**

Deliverables:

1. [Implemented] Convert the real-VM smoke procedure into an automated
   integration harness.
2. [Implemented] Pin container images by registry digest and record software
   versions.
3. [Implemented] Add CI gates for Rust tests, Clippy, Compose validation,
   migration smoke tests, and assertion compatibility tests.
4. [Implemented] Preserve the Hyper-V clean checkpoint and create repeatable VM
   reset scripts.
5. Remove temporary shared credentials and rotate all lab secrets.

Exit criteria:

- A clean checkout can reproduce the current successful NLA desktop connection.
- A failed certificate pin, missing origin, invalid token, and unbound target all
  fail closed in CI or the isolated integration environment.
- No test credential or launch assertion appears in Git, logs, reports, or the
  database.

## Phase 1 — Production identity and HTTPS

Priority: **P0**
Estimated readiness contribution: **10%**
Target cumulative readiness: **70%**

Deliverables:

1. Replace the shared bearer token with OIDC Authorization Code + PKCE.
2. Require MFA through the identity provider.
3. Use secure, `HttpOnly`, `SameSite=Strict` portal sessions with rotation.
4. Implement role separation for users, approvers, auditors, and administrators.
5. Add explicit CSRF tokens in addition to origin validation.
6. Terminate TLS at a production reverse proxy with trusted certificates.
7. Enforce HTTPS redirects, HSTS, secure cookies, CSP, frame policy, and request
   size/rate limits.
8. Bind user identity server-side; never accept an asserted subject from the
   browser.

Exit criteria:

- Every portal and Guacamole request is encrypted in transit.
- MFA is required for remote access.
- Authorization tests prove that user, approver, auditor, and administrator
  permissions cannot be crossed.
- Session fixation, CSRF, origin spoofing, and bearer replay tests fail closed.

## Phase 2 — Credential broker and one-time launches

Priority: **P0**
Estimated readiness contribution: **10%**
Target cumulative readiness: **80%**

Deliverables:

1. Replace browser-entered Windows passwords with a vault or credential broker.
2. Retrieve credentials only after identity, device, policy, and approval checks.
3. Use per-session or short-lived Windows credentials where the target platform
   supports them.
4. Build a Guacamole authentication extension that redeems a random one-time
   launch ID from the portal over an authenticated back channel.
5. Keep Windows credentials and connection parameters out of URLs and browser
   memory.
6. Atomically mark launch IDs used before returning connection material.
7. Audit vault access, launch redemption, denial, expiry, and reuse attempts.

Exit criteria:

- Replaying a launch URL or ID fails after first use.
- The browser never receives the Windows password.
- A database, application-log, proxy-log, browser-history, and crash-dump review
  finds no reusable credential.
- Vault failure and broker timeout deny access without falling back to static
  credentials.

## Phase 3 — Enforceable session lifecycle

Priority: **P0**
Estimated readiness contribution: **8%**
Target cumulative readiness: **88%**

Deliverables:

1. Add authenticated callbacks from the Guacamole extension for connecting,
   active, disconnected, failed, and terminated states.
2. Enforce absolute session duration in the gateway, not only in the portal UI.
3. Add portal-initiated termination with acknowledgement from guacd/Guacamole.
4. Reconcile orphaned sessions after process restart or network partition.
5. Enforce per-user and per-target concurrent-session limits.
6. Record start, authentication, first frame, disconnect, and termination reason.
7. Add idle timeout separately from absolute maximum duration.

Exit criteria:

- A session is forcibly disconnected at the configured deadline within an
  accepted tolerance.
- Database state matches the real gateway state after normal exit, browser
  crash, container restart, VM restart, and network interruption.
- Concurrent-session limits remain correct under race and retry tests.
- Administrators can terminate a session and receive an auditable confirmation.

## Phase 4 — Behavioral policy qualification

Priority: **P1**
Estimated readiness contribution: **5%**
Target cumulative readiness: **93%**

Deliverables:

1. Build two policies: all redirections denied and individually enabled controls.
2. Test clipboard independently in browser-to-remote and remote-to-browser
   directions.
3. Test upload and download independently with known files and hashes.
4. Test printing, audio output, and microphone capture.
5. Verify denied operations leave no partial files, clipboard data, print jobs,
   audio streams, or unexpected audit gaps.
6. Test policy changes during launch races and confirm the server-side snapshot
   is the policy actually enforced.

Exit criteria:

- Every control has a positive and negative real-desktop test.
- Directional controls are proven independent.
- Browser UI labels, assertion parameters, guacd behavior, Windows behavior,
  and audit records agree for every case.

## Phase 5 — Recording and audit lifecycle

Priority: **P1**
Estimated readiness contribution: **3%**
Target cumulative readiness: **96%**

Deliverables:

1. Store recordings in encrypted object storage outside the guacd container.
2. Use immutable object identifiers derived from session IDs, not user input.
3. Verify recording creation, finalization, playback, retention, and deletion.
4. Protect playback with the same authorization and audit model as live access.
5. Export normalized events to the SIEM with retry and back-pressure handling.
6. Define retention and legal-hold policies.

Exit criteria:

- Recording-required sessions fail closed if recording cannot start.
- A recording can be traced to one session and its policy snapshot.
- Unauthorized playback and deletion attempts fail and generate audit events.

## Phase 6 — Reliability, performance, and scale

Priority: **P2**
Estimated readiness contribution: **3%**
Target cumulative readiness: **99%**

Deliverables:

1. Measure launch API p50/p95/p99 and first-frame latency.
2. Run sustained desktop workloads for idle, office use, scrolling, and video.
3. Test 1, 5, 10, and the planned maximum concurrent sessions.
4. Measure portal, Guacamole, guacd, PostgreSQL, browser, VM, and network use.
5. Add readiness/liveness probes, graceful shutdown, backup/restore, and disaster
   recovery tests.
6. Test dependency loss, container restart, database failover, slow targets, and
   expired certificates.
7. Define capacity limits and alerts from measured results.

Exit criteria:

- The service meets documented latency, concurrency, recovery, and resource
  budgets for the target deployment size.
- Load shedding denies new work without breaking existing sessions.
- Backup restoration includes policies, bindings, sessions, and audit integrity.

## Phase 7 — Release and operational approval

Priority: **P2**
Estimated readiness contribution: **1%**
Target cumulative readiness: **100%**

Deliverables:

1. Threat-model review and independent security assessment.
2. Dependency, image, secret, and infrastructure scans with documented triage.
3. Operator runbooks for deployment, rotation, incident response, restore, and
   emergency session termination.
4. Supported-platform matrix and upgrade/rollback procedure.
5. Final production-readiness review with named owners and accepted residual risk.

Exit criteria:

- No unresolved critical or high-severity finding.
- Operations can deploy, observe, rotate, terminate, recover, and roll back the
  service using the documented procedures.
- Security, operations, and product owners approve release evidence.

## Recommended execution order

```text
Baseline automation
  -> OIDC/MFA + HTTPS
  -> Vault + one-time Guacamole extension
  -> Enforced lifecycle + callbacks
  -> Desktop policy qualification
  -> Recording/audit lifecycle
  -> Reliability/performance/scale
  -> Security and operational release approval
```

## Immediate implementation backlog

The next work should be executed in this order:

1. Add integration-test automation for the current Hyper-V/Edge success path.
2. Select the production OIDC provider and TLS termination architecture.
3. Define the one-time launch redemption API and Guacamole extension contract.
4. Define session-state and callback schemas before implementing termination.
5. Select the credential vault and Windows credential issuance strategy.

These five decisions unblock all remaining P0 work and prevent policy or UI work
from being built on a temporary authentication model.
