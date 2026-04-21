-- Append-only timestamped statements emitted by the workflow-agent.
-- Polymorphic scope: organization_id is always set; project_id and
-- member_user_id are populated according to the `scope` discriminator.

CREATE TABLE updates (
    seq BIGSERIAL PRIMARY KEY,
    update_id UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    project_id TEXT REFERENCES projects(project_id) ON DELETE CASCADE,
    member_user_id TEXT REFERENCES "user"(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('organization', 'project', 'member')),
    -- Soft 180-char cap is enforced in prompts and display layers; the DB cap
    -- is generous so a borderline overrun never drops a meaningful statement.
    body_text TEXT NOT NULL CHECK (length(body_text) BETWEEN 1 AND 1000),
    source_workflow_kind TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (
        (scope = 'organization' AND project_id IS NULL     AND member_user_id IS NULL)
        OR (scope = 'project'   AND project_id IS NOT NULL AND member_user_id IS NULL)
        OR (scope = 'member'    AND project_id IS NULL     AND member_user_id IS NOT NULL)
    )
);

CREATE INDEX idx_updates_org_created_at
    ON updates (organization_id, created_at DESC, seq DESC);

CREATE INDEX idx_updates_project_created_at
    ON updates (project_id, created_at DESC, seq DESC)
    WHERE project_id IS NOT NULL;

CREATE INDEX idx_updates_org_member_created_at
    ON updates (organization_id, member_user_id, created_at DESC, seq DESC)
    WHERE member_user_id IS NOT NULL;

-- Extend the workflow-tracking CHECKs so the coordinator can claim the new
-- emitter kinds with the existing claim/heartbeat machinery.
ALTER TABLE project_workflows
    DROP CONSTRAINT project_workflows_workflow_kind_check,
    ADD CONSTRAINT project_workflows_workflow_kind_check CHECK (workflow_kind IN (
        'project_memory_extract',
        'project_memory_consolidate',
        'project_skills',
        'project_updates_emit'
    ));

ALTER TABLE organization_workflows
    DROP CONSTRAINT organization_workflows_workflow_kind_check,
    ADD CONSTRAINT organization_workflows_workflow_kind_check CHECK (workflow_kind IN (
        'organization_memory_consolidate',
        'organization_skills',
        'organization_updates_emit'
    ));
