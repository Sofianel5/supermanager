ALTER TABLE summaries RENAME TO room_summaries;

CREATE TABLE organization_summaries (
    organization_id TEXT PRIMARY KEY REFERENCES organization(id) ON DELETE CASCADE,
    content_json JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'generating', 'error')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
