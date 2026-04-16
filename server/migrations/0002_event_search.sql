CREATE EXTENSION IF NOT EXISTS vector;

ALTER TABLE hook_events
    ADD COLUMN IF NOT EXISTS search_text TEXT,
    ADD COLUMN IF NOT EXISTS embedding vector(3072),
    ADD COLUMN IF NOT EXISTS indexed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_hook_events_embedding
    ON hook_events USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;
