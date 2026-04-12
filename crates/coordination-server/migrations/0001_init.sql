CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS summaries (
    room_id TEXT PRIMARY KEY REFERENCES rooms(room_id) ON DELETE CASCADE,
    content_json JSONB NOT NULL,
    thread_id TEXT,
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'generating', 'error')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS hook_events (
    seq BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    room_id TEXT NOT NULL REFERENCES rooms(room_id) ON DELETE CASCADE,
    employee_name TEXT NOT NULL,
    client TEXT NOT NULL,
    repo_root TEXT NOT NULL,
    branch TEXT,
    payload_json JSONB NOT NULL DEFAULT 'null'::jsonb,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_hook_events_room_seq
    ON hook_events (room_id, seq DESC);
