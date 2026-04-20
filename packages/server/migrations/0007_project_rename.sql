ALTER TABLE rooms RENAME TO projects;

ALTER INDEX idx_rooms_organization_id
    RENAME TO idx_projects_organization_id;

ALTER TABLE room_summaries RENAME TO project_summaries;

ALTER TABLE project_summaries
    RENAME COLUMN room_id TO project_id;

ALTER TABLE hook_events
    RENAME COLUMN room_id TO project_id;

ALTER INDEX idx_hook_events_room_seq
    RENAME TO idx_hook_events_project_seq;
