-- Your SQL goes here
CREATE TABLE authorized_users (
    email TEXT NOT NULL UNIQUE PRIMARY KEY,
    telegram_userid BIGINT
)
