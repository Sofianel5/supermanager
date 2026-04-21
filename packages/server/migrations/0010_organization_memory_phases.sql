-- Transcript-backed workflow storage and typed memory/skills tables.
--
-- Two tracking tables (organization_workflows, project_workflows) record which
-- workflow kinds are in flight. The actual memory and skill content lives in
-- five typed tables below, one per concept. Memory extract and memory
-- consolidate share project_memory_raw (the extractor stages rows; the
-- consolidator reads and then deletes them).

CREATE TABLE hook_event_transcripts (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    last_event_id UUID NOT NULL,
    member_user_id TEXT NOT NULL,
    member_name TEXT NOT NULL,
    client TEXT NOT NULL,
    repo_root TEXT NOT NULL,
    branch TEXT,
    transcript_path TEXT NOT NULL,
    content_text TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, session_id)
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
    -- Tiebreaker for batches that hit the row limit at a `received_at` value
    -- shared by multiple transcripts. NULL means the cursor advanced past the
    -- entire `received_at` instant (no further rows at that instant).
    last_processed_session_id TEXT,
    PRIMARY KEY (project_id, workflow_kind)
);

-- Tiebreaker on organization_summaries for the org-summary cursor. The org
-- summary cursor lives in `organization_summaries.updated_at`; this column
-- disambiguates rows that share the same `received_at` when a batch hits the
-- row limit at that instant. NULL means the cursor advanced past the entire
-- `received_at` instant.
ALTER TABLE organization_summaries
    ADD COLUMN last_processed_seq BIGINT;

-- Raw per-transcript memory candidates staged by project_memory_extract and
-- consumed by project_memory_consolidate.
CREATE TABLE project_memory_raw (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    session_id TEXT NOT NULL,
    content_text TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, session_id)
);

CREATE INDEX idx_project_memory_raw_updated_at
    ON project_memory_raw (project_id, updated_at DESC);

-- Durable project memory: one row per project. `handbook_text` is the full
-- handbook; `summary_text` is the short navigational index.
CREATE TABLE project_memory (
    project_id TEXT PRIMARY KEY REFERENCES projects(project_id) ON DELETE CASCADE,
    handbook_text TEXT NOT NULL DEFAULT '',
    summary_text TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Durable project skills: one row per (project, skill_name). `content_text`
-- holds the full SKILL.md body.
CREATE TABLE project_skills (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    skill_name TEXT NOT NULL,
    content_text TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project_id, skill_name)
);

CREATE INDEX idx_project_skills_updated_at
    ON project_skills (project_id, updated_at DESC);

-- Durable organization memory: one row per organization.
CREATE TABLE organization_memory (
    organization_id TEXT PRIMARY KEY REFERENCES organization(id) ON DELETE CASCADE,
    handbook_text TEXT NOT NULL DEFAULT '',
    summary_text TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Durable organization skills: one row per (organization, skill_name).
CREATE TABLE organization_skills (
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    skill_name TEXT NOT NULL,
    content_text TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, skill_name)
);

CREATE INDEX idx_organization_skills_updated_at
    ON organization_skills (organization_id, updated_at DESC);
