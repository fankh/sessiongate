# SessionGate SIEM Integration Contract

The portal uses a transactional PostgreSQL outbox. Security events are inserted
in the same transaction as the state change they describe, then delivered by a
separate exporter. This prevents a temporary SIEM outage from losing an event or
blocking an already-authorized desktop session.

## Delivery

- HTTPS `POST` with JSON batches is the default transport.
- Production authentication is mutual TLS. An optional HMAC-SHA-256 signature
  covers the uncompressed request body.
- Delivery is at least once. Consumers deduplicate on `event_id`.
- A successful receiver returns HTTP 200, 201, 202, or 204.
- HTTP 408, 425, 429, and 5xx responses are retried with exponential backoff and
  jitter. Other 4xx responses enter the dead-letter state.
- Passwords, session cookies, CSRF values, recovery codes, launch IDs, vault
  responses, and Guacamole assertions are prohibited fields.

## Event envelope

```json
{
  "schema_version": "1.0",
  "event_id": "01900000-0000-7000-8000-000000000000",
  "timestamp": "2026-07-18T08:00:00Z",
  "event_type": "rdp.session.launch",
  "outcome": "allowed",
  "actor": { "id": "uuid", "roles": ["user"] },
  "source": { "ip": "192.0.2.1" },
  "resource": { "session_id": "uuid", "target_id": "uuid" },
  "reason": null,
  "policy": { "id": "uuid", "snapshot_hash": "sha256:..." }
}
```

The exporter adapter will be selected after the SIEM ingestion interface is
known. HTTPS JSON, syslog/CEF, Kafka, and local-agent adapters must preserve the
same envelope and delivery semantics.
