ALTER TABLE room_summaries
    ADD COLUMN IF NOT EXISTS last_processed_seq BIGINT NOT NULL DEFAULT 0;
