-- Ahab's schema, in the SQL that SQLite and Postgres share. IDs are UUIDs as
-- lowercase hyphenated TEXT, times are BIGINT microseconds since the Unix
-- epoch (UTC), JSON is TEXT and enums are TEXT. Each JSON column has a
-- `<column>_format` beside it: the shape its JSON was written in, so a later
-- Ahab can change the shape and an older one never misreads it.
--
-- One file per module, in the order the modules use each other. Every
-- statement is `IF NOT EXISTS`, so a migration that stopped half-way can run
-- again. A released migration never changes: later changes are new files.
--
-- Preferences that are server-wide today have a nullable `user_id` for when
-- Ahab has users: NULL is the server-wide row, a user's own rows will carry
-- their ID. As NULLs never clash in a unique index (on SQLite and Postgres
-- alike), each such table has two partial unique indexes, one for the
-- server-wide rows and one for users' rows, and writes name the one they
-- mean: `ON CONFLICT (feature) WHERE user_id IS NULL`.

-- Core features switched off (Terminal, Files, Git, Search): a row per
-- feature that differs from its default.
CREATE TABLE IF NOT EXISTS features (
    feature TEXT NOT NULL,
    user_id TEXT,
    enabled BOOLEAN NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS features_unique_server_feature ON features (feature) WHERE user_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS features_unique_user_feature ON features (user_id, feature) WHERE user_id IS NOT NULL;
