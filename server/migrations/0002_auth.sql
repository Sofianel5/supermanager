CREATE TABLE IF NOT EXISTS "user" (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    image TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_email_unique
    ON "user" (email);

CREATE TABLE IF NOT EXISTS session (
    id TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL,
    token TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_address TEXT,
    user_agent TEXT,
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    active_organization_id TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_session_token_unique
    ON session (token);

CREATE INDEX IF NOT EXISTS idx_session_user_id
    ON session (user_id);

CREATE TABLE IF NOT EXISTS account (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
    access_token TEXT,
    refresh_token TEXT,
    id_token TEXT,
    access_token_expires_at TIMESTAMPTZ,
    refresh_token_expires_at TIMESTAMPTZ,
    scope TEXT,
    password TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_account_provider_account_unique
    ON account (provider_id, account_id);

CREATE INDEX IF NOT EXISTS idx_account_user_id
    ON account (user_id);

CREATE TABLE IF NOT EXISTS verification (
    id TEXT PRIMARY KEY,
    identifier TEXT NOT NULL,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_verification_identifier
    ON verification (identifier);

CREATE TABLE IF NOT EXISTS organization (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    logo TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_organization_slug_unique
    ON organization (slug);

CREATE TABLE IF NOT EXISTS member (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organization(id),
    user_id TEXT NOT NULL REFERENCES "user"(id),
    role TEXT NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_member_organization_id
    ON member (organization_id);

CREATE INDEX IF NOT EXISTS idx_member_user_id
    ON member (user_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_member_org_user_unique
    ON member (organization_id, user_id);

CREATE TABLE IF NOT EXISTS invitation (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organization(id),
    email TEXT NOT NULL,
    role TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    inviter_id TEXT NOT NULL REFERENCES "user"(id)
);

CREATE INDEX IF NOT EXISTS idx_invitation_organization_id
    ON invitation (organization_id);

CREATE INDEX IF NOT EXISTS idx_invitation_email
    ON invitation (email);

CREATE TABLE IF NOT EXISTS device_code (
    id TEXT PRIMARY KEY,
    device_code TEXT NOT NULL,
    user_code TEXT NOT NULL,
    user_id TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    last_polled_at TIMESTAMPTZ,
    polling_interval BIGINT,
    client_id TEXT,
    scope TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_device_code_device_code_unique
    ON device_code (device_code);

CREATE UNIQUE INDEX IF NOT EXISTS idx_device_code_user_code_unique
    ON device_code (user_code);

CREATE TABLE IF NOT EXISTS apikey (
    id TEXT PRIMARY KEY,
    config_id TEXT NOT NULL DEFAULT 'default',
    name TEXT,
    start TEXT,
    reference_id TEXT NOT NULL,
    prefix TEXT,
    key TEXT NOT NULL,
    refill_interval BIGINT,
    refill_amount BIGINT,
    last_refill_at TIMESTAMPTZ,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    rate_limit_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    rate_limit_time_window BIGINT NOT NULL DEFAULT 86400000,
    rate_limit_max BIGINT NOT NULL DEFAULT 100000,
    request_count BIGINT NOT NULL DEFAULT 0,
    remaining BIGINT,
    last_request TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    permissions TEXT,
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_apikey_config_id
    ON apikey (config_id);

CREATE INDEX IF NOT EXISTS idx_apikey_reference_id
    ON apikey (reference_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_apikey_key_unique
    ON apikey (key);

ALTER TABLE rooms
    ADD COLUMN IF NOT EXISTS organization_id TEXT REFERENCES organization(id),
    ADD COLUMN IF NOT EXISTS created_by_user_id TEXT REFERENCES "user"(id);

CREATE INDEX IF NOT EXISTS idx_rooms_organization_id
    ON rooms (organization_id);
