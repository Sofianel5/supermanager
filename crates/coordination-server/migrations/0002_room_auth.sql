CREATE TABLE IF NOT EXISTS workspaces (
    workspace_id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    workos_organization_id TEXT NOT NULL UNIQUE,
    owner_workos_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE rooms
    ADD COLUMN workspace_id UUID NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_rooms_workspace_id
    ON rooms (workspace_id);

CREATE TABLE IF NOT EXISTS room_reporter_tokens (
    token_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    repo_root TEXT,
    created_by_workos_user_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_room_reporter_tokens_room_id
    ON room_reporter_tokens (room_id);
