DELETE FROM organization_summaries;

DELETE FROM room_summaries;

DELETE FROM hook_events
WHERE employee_user_id IS NULL;

ALTER TABLE hook_events
    DROP CONSTRAINT IF EXISTS hook_events_employee_user_id_fkey;

ALTER TABLE hook_events
    ALTER COLUMN employee_user_id SET NOT NULL;

ALTER TABLE hook_events
    ADD CONSTRAINT hook_events_employee_user_id_fkey
    FOREIGN KEY (employee_user_id) REFERENCES "user"(id) ON DELETE RESTRICT;
