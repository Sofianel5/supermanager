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
