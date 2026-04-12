ALTER TABLE rooms
    ADD COLUMN IF NOT EXISTS workos_organization_id TEXT,
    ADD COLUMN IF NOT EXISTS owner_workos_user_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_rooms_workos_organization_id
    ON rooms (workos_organization_id)
    WHERE workos_organization_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS room_invite_links (
    invite_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_by_workos_user_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_room_invite_links_room_id
    ON room_invite_links (room_id);
