-- Your SQL goes here
CREATE TABLE diary_entries (
    diary_date DATE PRIMARY KEY,
    diary_text TEXT NOT NULL,
    last_modified TIMESTAMP WITH TIME ZONE NOT NULL
)