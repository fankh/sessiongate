CREATE TABLE credential_references (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL CHECK (
        provider IN ('local_encrypted', 'hcp_vault_secrets', 'azure_key_vault')
    ),
    external_ref TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unknown' CHECK (
        status IN ('unknown', 'healthy', 'unavailable', 'disabled')
    ),
    last_rotated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, external_ref),
    CONSTRAINT credential_reference_nonempty CHECK (
        length(btrim(name)) BETWEEN 1 AND 128
        AND length(btrim(external_ref)) BETWEEN 1 AND 512
    )
);

ALTER TABLE rdp_targets
    ALTER COLUMN credential_ref TYPE UUID USING credential_ref::UUID;

ALTER TABLE rdp_targets
    ADD CONSTRAINT rdp_targets_credential_ref_fk
    FOREIGN KEY (credential_ref) REFERENCES credential_references(id)
    ON DELETE RESTRICT;

CREATE TABLE rdp_approval_requests (
    id UUID PRIMARY KEY,
    requester_id UUID NOT NULL REFERENCES portal_users(id) ON DELETE RESTRICT,
    target_id UUID NOT NULL REFERENCES rdp_targets(id) ON DELETE RESTRICT,
    policy_snapshot JSONB NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (
        state IN ('pending', 'approved', 'denied', 'expired', 'cancelled', 'consumed')
    ),
    decided_by UUID REFERENCES portal_users(id) ON DELETE RESTRICT,
    decision_reason TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT approval_decision_consistent CHECK (
        (state = 'pending' AND decided_by IS NULL AND decided_at IS NULL)
        OR state IN ('expired', 'cancelled', 'consumed')
        OR (state IN ('approved', 'denied') AND decided_by IS NOT NULL AND decided_at IS NOT NULL)
    )
);

CREATE INDEX rdp_approval_pending_idx ON rdp_approval_requests (expires_at, created_at)
    WHERE state = 'pending';

CREATE TABLE management_audit_events (
    id UUID PRIMARY KEY,
    actor_user_id UUID REFERENCES portal_users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('allowed', 'denied', 'error')),
    source_ip INET NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX management_audit_created_idx ON management_audit_events (created_at DESC);
