-- Your SQL goes here
CREATE TABLE authorized_users (
    email VARCHAR(100) NOT NULL UNIQUE PRIMARY KEY,
    telegram_userid BIGINT
)
