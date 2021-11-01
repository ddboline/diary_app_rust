CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE diary_conflict (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    sync_datetime TIMESTAMP WITH TIME ZONE NOT NULL,
    diary_date DATE NOT NULL,
    diff_type TEXT NOT NULL,
    diff_text TEXT NOT NULL
)
