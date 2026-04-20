DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'hook_events'
          AND column_name = 'employee_name'
    ) THEN
        ALTER TABLE hook_events
            RENAME COLUMN employee_name TO member_name;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'hook_events'
          AND column_name = 'employee_user_id'
    ) THEN
        ALTER TABLE hook_events
            RENAME COLUMN employee_user_id TO member_user_id;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM pg_class
        WHERE relkind = 'i'
          AND relname = 'idx_hook_events_employee_user_id'
    ) AND NOT EXISTS (
        SELECT 1
        FROM pg_class
        WHERE relkind = 'i'
          AND relname = 'idx_hook_events_member_user_id'
    ) THEN
        ALTER INDEX idx_hook_events_employee_user_id
            RENAME TO idx_hook_events_member_user_id;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'hook_events_employee_user_id_fkey'
    ) AND NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'hook_events_member_user_id_fkey'
    ) THEN
        ALTER TABLE hook_events
            RENAME CONSTRAINT hook_events_employee_user_id_fkey TO hook_events_member_user_id_fkey;
    END IF;
END $$;

CREATE OR REPLACE FUNCTION rename_employee_snapshot_members(items JSONB)
RETURNS JSONB
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT COALESCE(
        jsonb_agg(
            (
                item - 'employee_user_id' - 'employee_name'
            ) || jsonb_build_object(
                'member_user_id',
                COALESCE(item -> 'member_user_id', item -> 'employee_user_id'),
                'member_name',
                COALESCE(item -> 'member_name', item -> 'employee_name')
            )
        ),
        '[]'::jsonb
    )
    FROM jsonb_array_elements(COALESCE(items, '[]'::jsonb)) AS entries(item);
$$;

UPDATE project_summaries
SET content_json = (content_json - 'employees') || jsonb_build_object(
    'members',
    rename_employee_snapshot_members(content_json -> 'employees')
)
WHERE content_json ? 'employees';

UPDATE organization_summaries
SET content_json = (content_json - 'employees') || jsonb_build_object(
    'members',
    rename_employee_snapshot_members(content_json -> 'employees')
)
WHERE content_json ? 'employees';

DROP FUNCTION rename_employee_snapshot_members(JSONB);
