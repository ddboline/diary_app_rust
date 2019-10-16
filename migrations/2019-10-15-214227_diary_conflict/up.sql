-- Your SQL goes here
CREATE TABLE diary_conflict (
    id SERIAL PRIMARY KEY,
    sync_datetime TIMESTAMP WITH TIME ZONE NOT NULL,
    diary_date DATE NOT NULL,
    diff_type TEXT NOT NULL,
    diff_text TEXT NOT NULL
)
