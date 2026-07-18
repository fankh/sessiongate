CREATE TABLE portal_users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    totp_secret BYTEA NOT NULL,
    roles TEXT[] NOT NULL DEFAULT ARRAY['user']::TEXT[],
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    failed_login_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_login_count >= 0),
    locked_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT portal_users_username_normalized CHECK (username = lower(username)),
    CONSTRAINT portal_users_roles_valid CHECK (
        roles <@ ARRAY['user','approver','auditor','administrator']::TEXT[]
        AND cardinality(roles) > 0
    )
);

CREATE UNIQUE INDEX portal_users_username_unique ON portal_users (lower(username));

CREATE TABLE portal_recovery_codes (
    user_id UUID NOT NULL REFERENCES portal_users(id) ON DELETE CASCADE,
    code_hash BYTEA NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, code_hash)
);

CREATE TABLE portal_login_challenges (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES portal_users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    consumed_at TIMESTAMPTZ,
    source_ip INET NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE portal_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES portal_users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    csrf_hash BYTEA NOT NULL,
    source_ip INET NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    idle_expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT portal_session_absolute_limit CHECK (expires_at <= created_at + interval '8 hours')
);

CREATE INDEX portal_sessions_active_token_idx ON portal_sessions (token_hash)
    WHERE revoked_at IS NULL;

CREATE TABLE security_outbox (
    event_id UUID PRIMARY KEY,
    schema_version TEXT NOT NULL DEFAULT '1.0',
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX security_outbox_pending_idx
    ON security_outbox (next_attempt_at, created_at) WHERE delivered_at IS NULL;
