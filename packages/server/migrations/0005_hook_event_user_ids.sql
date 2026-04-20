ALTER TABLE hook_events
    ADD COLUMN IF NOT EXISTS employee_user_id TEXT REFERENCES "user"(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_hook_events_employee_user_id
    ON hook_events (employee_user_id);
