-- Ahab's own settings (the editor's), per section and scope. A
-- scope is `server` (scope_id '') or `project` (scope_id the project's ID),
-- and later `user` (scope_id the user's ID), as plugins' settings are.
CREATE TABLE IF NOT EXISTS settings (
    section TEXT NOT NULL,
    scope TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    setting_values TEXT NOT NULL,
    setting_values_format INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (section, scope, scope_id)
);
