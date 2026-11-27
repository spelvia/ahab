-- Notifications: what happened that the server's person should
-- know about, kept for the inbox. A notification with a `group_key` stands
-- for its group while unread: a newer one of the group replaces it.
-- `target` is what opening it shows (JSON, `target_format`).
CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    level TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    project_id TEXT REFERENCES projects (id) ON DELETE CASCADE,
    target TEXT NOT NULL,
    target_format INTEGER NOT NULL DEFAULT 1,
    group_key TEXT,
    created_at BIGINT NOT NULL,
    read_at BIGINT
);
CREATE INDEX IF NOT EXISTS notifications_by_created ON notifications (created_at);
CREATE INDEX IF NOT EXISTS notifications_by_read ON notifications (read_at);
CREATE UNIQUE INDEX IF NOT EXISTS notifications_unique_unread_group
    ON notifications (group_key) WHERE read_at IS NULL AND group_key IS NOT NULL;

-- The server's notification settings: one row, absent until changed.
-- `keep_days` NULL keeps read notifications forever; `kinds` holds the
-- kinds turned off or on (JSON, `kinds_format`); `outside` shows them
-- outside the app too (the browser's, the desktop's, the phone's).
CREATE TABLE IF NOT EXISTS notification_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    keep_days INTEGER,
    kinds TEXT NOT NULL DEFAULT '{}',
    kinds_format INTEGER NOT NULL DEFAULT 1,
    outside BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at BIGINT NOT NULL
);
