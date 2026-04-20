-- Transcript-backed workflow storage.
--
-- workflow_kind in *_workflows (tracking) is distinct from workflow_kind in
-- *_workflow_documents (storage namespace). The tracking tables name the agent
-- that ran; the document tables name the shared file namespace that several
-- agents read and write. Memory extract + consolidate share one namespace so the
-- consolidator can read the raw files written by the extractor.

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
            'organization_memory_consolidate',
            'organization_skills'
        )
    ),
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'generating', 'error')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_processed_received_at TIMESTAMPTZ NOT NULL DEFAULT TO_TIMESTAMP(0),
    PRIMARY KEY (organization_id, workflow_kind)
);

-- Organization-scoped document storage. Shared namespaces 'organization_memories'
-- and 'organization_skills'. No CHECK — Rust validates allowed values.
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

-- Project tier: per-project tracking of memory extract, memory consolidate, and skills.
CREATE TABLE project_workflows (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    workflow_kind TEXT NOT NULL CHECK (
        workflow_kind IN (
            'project_memory_extract',
            'project_memory_consolidate',
            'project_skills'
        )
    ),
    status TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'generating', 'error')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_processed_received_at TIMESTAMPTZ NOT NULL DEFAULT TO_TIMESTAMP(0),
    PRIMARY KEY (project_id, workflow_kind)
);

-- Project-scoped document storage. Shared namespaces 'project_memories' (raw +
-- consolidated) and 'project_skills'. No CHECK — Rust validates allowed values.
CREATE TABLE project_workflow_documents (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    workflow_kind TEXT NOT NULL,
    document_path TEXT NOT NULL,
    content_text TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, workflow_kind, document_path)
);

CREATE INDEX idx_project_workflow_documents_updated_at
    ON project_workflow_documents (project_id, workflow_kind, updated_at DESC);
