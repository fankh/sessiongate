# SessionGate Management API Contract

Base path: `/api/v1`. JSON is required unless stated otherwise. Mutations require
an authenticated server-side session, the `Origin` header, and `X-CSRF-Token`.
Authorization is enforced by the centralized permission matrix in
`portal/src/management.rs`.

## Identity

| Method | Path | Permission | Purpose |
|---|---|---|---|
| POST | `/auth/login` | Public | Verify username/password and create MFA challenge |
| POST | `/auth/mfa` | Challenge | Verify TOTP or one recovery code and create session |
| POST | `/auth/logout` | Authenticated | Revoke current session |
| GET | `/auth/me` | Authenticated | Return identity, roles and CSRF token |
| POST | `/auth/password` | Authenticated | Change own password and revoke other sessions |

## Administration

| Resource | Collection path | Required permission |
|---|---|---|
| Users | `/admin/users` | `manage_users` |
| Destinations | `/admin/destinations` | `manage_destinations` |
| Credential references | `/admin/credentials` | `manage_credentials` |
| Policies | `/admin/policies` | `manage_policies` |
| Assignments | `/admin/assignments` | `manage_policies` |
| Active sessions | `/admin/sessions` | `terminate_any_session` for termination |
| System health | `/admin/health` | `view_system_health` |

Collection resources use `GET` and `POST`. Individual resources use `GET`,
`PATCH`, and `DELETE` at `/{id}`. Disabling is preferred over deletion when an
audit or session record refers to the resource. Optimistic updates use the
`updated_at` value supplied as `If-Unmodified-Since`.

## Credential non-disclosure

Credential-reference responses contain only:

```json
{
  "id": "uuid",
  "name": "Windows administrators",
  "provider": "local_encrypted",
  "external_ref": "local/windows-admin",
  "status": "healthy",
  "last_rotated_at": "2026-07-18T08:00:00Z"
}
```

Secret material is accepted only by `PUT /admin/credentials/{id}/secret`. A
successful response is HTTP 204 with no body. No `GET` operation returns a
secret. Replacement requires recent MFA confirmation and generates a security
outbox event.

## Access and approvals

| Method | Path | Purpose |
|---|---|---|
| GET | `/rdp/destinations` | Effective destinations and policy for current user |
| POST | `/rdp/approvals` | Request approval when effective policy requires it |
| GET | `/approvals` | Approver queue restricted to approval scope |
| POST | `/approvals/{id}/decision` | Approve or deny exactly once |
| POST | `/rdp/sessions` | Create one-time launch after all gates pass |
| DELETE | `/rdp/sessions/{id}` | Terminate own session |
| POST | `/admin/sessions/{id}/terminate` | Administrator termination |

## Error format

```json
{
  "error": {
    "code": "access_denied",
    "message": "Access is denied by policy.",
    "request_id": "uuid"
  }
}
```

Errors never distinguish an unknown username from an incorrect password and
never include database, broker, credential, assertion, or stack-trace details.
