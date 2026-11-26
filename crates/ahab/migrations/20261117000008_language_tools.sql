-- Language tooling: tool settings per machine and project, the
-- latest problems per project, and what an agent's turn changed.

-- Which tools, where and how they start, per machine (scope `machine`,
-- scope_id the machine's ID) and project (`project`, the project's ID).
-- Deleted with what they belong to by the services that delete it.
CREATE TABLE IF NOT EXISTS tool_settings (
    scope TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    settings TEXT NOT NULL,
    settings_format INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (scope, scope_id)
);

-- Problems: one row per project, file and tool, replaced on every change.
CREATE TABLE IF NOT EXISTS project_diagnostics (
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    source TEXT NOT NULL,
    diagnostics TEXT NOT NULL,
    diagnostics_format INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (project_id, path, source)
);

-- A chat turn's new and fixed problems ("3 new errors, 1 fixed").
CREATE TABLE IF NOT EXISTS turn_diagnostics (
    chat_id TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    turn TEXT NOT NULL,
    summary TEXT NOT NULL,
    summary_format INTEGER NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (chat_id, turn)
);
