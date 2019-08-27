-- Your SQL goes here
CREATE TABLE diary_cache (
    diary_datetime TIMESTAMP WITH TIME ZONE NOT NULL PRIMARY KEY UNIQUE,
    diary_text TEXT
)
