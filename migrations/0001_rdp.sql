CREATE TABLE rdp_targets (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    hostname TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 3389 CHECK (port = 3389),
    domain TEXT NOT NULL DEFAULT '',
    certificate_fingerprint TEXT NOT NULL,
    network_zone TEXT NOT NULL,
    credential_ref TEXT,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hostname, port)
);

CREATE TABLE rdp_policies (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    priority INTEGER NOT NULL DEFAULT 0,
    clipboard_to_browser BOOLEAN NOT NULL DEFAULT FALSE,
    clipboard_to_remote BOOLEAN NOT NULL DEFAULT FALSE,
    upload BOOLEAN NOT NULL DEFAULT FALSE,
    download BOOLEAN NOT NULL DEFAULT FALSE,
    printing BOOLEAN NOT NULL DEFAULT FALSE,
    audio_output BOOLEAN NOT NULL DEFAULT FALSE,
    microphone BOOLEAN NOT NULL DEFAULT FALSE,
    recording BOOLEAN NOT NULL DEFAULT FALSE,
    maximum_duration_seconds INTEGER NOT NULL DEFAULT 900
        CHECK (maximum_duration_seconds BETWEEN 1 AND 28800),
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE rdp_policy_bindings (
    id UUID PRIMARY KEY,
    policy_id UUID NOT NULL REFERENCES rdp_policies(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES rdp_targets(id) ON DELETE CASCADE,
    subject_type TEXT NOT NULL CHECK (subject_type IN ('user', 'role', 'group')),
    subject_id TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    UNIQUE (policy_id, target_id, subject_type, subject_id)
);

CREATE TABLE rdp_sessions (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT,
    target_id UUID NOT NULL REFERENCES rdp_targets(id),
    policy_snapshot JSONB NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'active', 'ended', 'denied')),
    source_ip INET NOT NULL,
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    termination_reason TEXT,
    recording_object_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX rdp_sessions_active_user_idx
    ON rdp_sessions (user_id, state) WHERE state IN ('pending', 'active');
CREATE INDEX rdp_sessions_target_created_idx ON rdp_sessions (target_id, created_at DESC);

CREATE TABLE rdp_audit_events (
    id UUID PRIMARY KEY,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_id UUID REFERENCES rdp_targets(id) ON DELETE SET NULL,
    session_id UUID REFERENCES rdp_sessions(id) ON DELETE SET NULL,
    source_ip INET NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('allowed', 'denied', 'error')),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX rdp_audit_actor_created_idx ON rdp_audit_events (actor_id, created_at DESC);
