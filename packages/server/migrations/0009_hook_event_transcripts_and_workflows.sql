CREATE TABLE hook_event_transcripts (
    session_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    last_event_id UUID NOT NULL,
    member_user_id TEXT NOT NULL,
    member_name TEXT NOT NULL,
    client TEXT NOT NULL,
    repo_root TEXT NOT NULL,
    branch TEXT,
    transcript_path TEXT NOT NULL,
    content_text TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_hook_event_transcripts_project_received_at
    ON hook_event_transcripts (project_id, received_at DESC);

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

CREATE TABLE organization_workflow_documents (
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    workflow_kind TEXT NOT NULL,
    document_path TEXT NOT NULL,
    content_text TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, workflow_kind, document_path)
);

CREATE INDEX idx_organization_workflow_documents_updated_at
    ON organization_workflow_documents (organization_id, workflow_kind, updated_at DESC);
