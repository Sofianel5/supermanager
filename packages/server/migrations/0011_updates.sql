CREATE TABLE project_updates (
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    source_event_id UUID NOT NULL REFERENCES hook_events(event_id) ON DELETE CASCADE,
    ordinal INT NOT NULL CHECK (ordinal >= 0),
    statement_text TEXT NOT NULL CHECK (NULLIF(BTRIM(statement_text), '') IS NOT NULL),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (project_id, source_event_id, ordinal)
);

CREATE INDEX idx_project_updates_created_at
    ON project_updates (project_id, created_at DESC, source_event_id DESC, ordinal DESC);

CREATE TABLE member_updates (
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    member_user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE RESTRICT,
    source_event_id UUID NOT NULL REFERENCES hook_events(event_id) ON DELETE CASCADE,
    ordinal INT NOT NULL CHECK (ordinal >= 0),
    statement_text TEXT NOT NULL CHECK (NULLIF(BTRIM(statement_text), '') IS NOT NULL),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (organization_id, member_user_id, source_event_id, ordinal)
);

CREATE INDEX idx_member_updates_created_at
    ON member_updates (
        organization_id,
        member_user_id,
        created_at DESC,
        source_event_id DESC,
        ordinal DESC
    );

CREATE TABLE organization_updates (
    organization_id TEXT NOT NULL REFERENCES organization(id) ON DELETE CASCADE,
    source_window_key TEXT NOT NULL,
    ordinal INT NOT NULL CHECK (ordinal >= 0),
    statement_text TEXT NOT NULL CHECK (NULLIF(BTRIM(statement_text), '') IS NOT NULL),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (organization_id, source_window_key, ordinal)
);

CREATE INDEX idx_organization_updates_created_at
    ON organization_updates (
        organization_id,
        created_at DESC,
        source_window_key DESC,
        ordinal DESC
    );
