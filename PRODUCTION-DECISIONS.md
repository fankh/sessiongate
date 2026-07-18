# SessionGate Production Decisions

Status: accepted for implementation on 2026-07-18.

| Area | Decision |
|---|---|
| Identity | Portal-owned accounts; no external identity dependency |
| Passwords | Argon2id; 14-character minimum; administrator-created accounts |
| MFA | TOTP required for every account; ten single-use recovery codes |
| Roles | User, approver, auditor, and administrator |
| Web edge | Caddy; local CA for testing and public ACME certificate in production |
| Credential broker | Provider-neutral API; encrypted local test provider and hosted production adapter |
| Launch | Random 256-bit ID, 30-second lifetime, atomic one-time redemption |
| Session lifetime | Eight-hour absolute limit and 30-minute idle limit |
| Concurrency | One active session for each user/target pair until capacity is selected |
| Recording | Not provided; recording code and release gates are out of scope |
| Object storage | MinIO for immutable audit archives and protected backups |
| SIEM | Transactional outbox and at-least-once normalized event delivery |
| Current deployment | Docker, browser, and Hyper-V test host; production server selected later |

## Deferred deployment inputs

The implementation remains portable until these environment-specific inputs are
available:

1. Production hostname and server topology.
2. Hosted vault vendor and workload authentication mechanism.
3. Whether Windows targets join Active Directory.
4. SIEM ingestion transport and authentication requirements.
5. Maximum supported concurrent-session target and service-level objectives.
