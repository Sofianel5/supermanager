CREATE TABLE hook_event_transcripts (
    event_id UUID PRIMARY KEY REFERENCES hook_events(event_id) ON DELETE CASCADE,
    transcript_path TEXT NOT NULL,
    content_text TEXT NOT NULL,
    truncated BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_hook_event_transcripts_created_at
    ON hook_event_transcripts (created_at DESC);

CREATE TABLE organization_workflows (
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    workflow_kind TEXT NOT NULL CHECK (
        workflow_kind IN (
            'organization_memories',
            'organization_skills'
        )
    ),
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'generating', 'error')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_processed_received_at TIMESTAMPTZ NOT NULL DEFAULT TO_TIMESTAMP(0),
    PRIMARY KEY (organization_id, workflow_kind)
);
