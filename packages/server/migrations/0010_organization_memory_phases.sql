-- Memory writing splits into extract (per transcript) + consolidate (periodic).
-- Extract lives at the project tier; consolidate runs at both project and org tiers
-- (org consolidate reads project-consolidated docs via get_snapshot, no transcripts).
--
-- workflow_kind in *_workflows (tracking) is distinct from workflow_kind in
-- *_workflow_documents (storage namespace). The tracking table names the agent that
-- ran; the document table names the shared file namespace that several agents read
-- and write. Memory extract and memory consolidate share one namespace so the
-- consolidator can see the raw files written by the extractor.

ALTER TABLE organization_workflows
    DROP CONSTRAINT organization_workflows_workflow_kind_check;

UPDATE organization_workflows
SET workflow_kind = 'organization_memory_consolidate'
WHERE workflow_kind = 'organization_memories';

ALTER TABLE organization_workflows
    ADD CONSTRAINT organization_workflows_workflow_kind_check CHECK (
        workflow_kind IN (
            'organization_memory_consolidate',
            'organization_skills'
        )
    );

-- organization_workflow_documents.workflow_kind keeps its existing values
-- ('organization_memories', 'organization_skills') as the storage namespace.
-- No migration needed there.

-- Project tier: per-project tracking of three workflows.
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
