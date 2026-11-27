-- Sessions, organised in folders per project.
--
-- Everything here belongs to a project and a machine, and goes with
-- them: deleting a project or a machine deletes its
-- sessions, chats, events and terminals (ON DELETE CASCADE), as it deleted
-- their records before. A deleted folder's sessions stay, out of any folder
-- (ON DELETE SET NULL; the service also moves them out first). Each column
-- a delete goes through has an index, so it finds the rows without a scan.
CREATE TABLE IF NOT EXISTS session_folders (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS session_folders_by_project_id_and_name ON session_folders (project_id, name);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'chat',
    title TEXT NOT NULL,
    folder_id TEXT REFERENCES session_folders (id) ON DELETE SET NULL,
    pinned_at BIGINT,
    archived_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_by_project_id_and_updated_at ON sessions (project_id, updated_at);
CREATE INDEX IF NOT EXISTS sessions_by_folder_id ON sessions (folder_id);

-- A session's chats and their events.
CREATE TABLE IF NOT EXISTS chats (
    id TEXT NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    native_id TEXT,
    model TEXT,
    effort TEXT,
    mode TEXT,
    title TEXT NOT NULL,
    state TEXT NOT NULL,
    status TEXT NOT NULL,
    unrestricted BOOLEAN NOT NULL DEFAULT FALSE,
    current_turn TEXT,
    open_requests TEXT NOT NULL,
    open_requests_format INTEGER NOT NULL DEFAULT 1,
    close_reason TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    closed_at BIGINT
);

CREATE INDEX IF NOT EXISTS chats_by_project_id_and_updated_at ON chats (project_id, updated_at);
CREATE INDEX IF NOT EXISTS chats_by_session_id_and_updated_at ON chats (session_id, updated_at);
CREATE INDEX IF NOT EXISTS chats_by_machine_id ON chats (machine_id);
-- The live chats are asked for every few seconds and as each machine
-- connects.
CREATE INDEX IF NOT EXISTS chats_by_state_and_machine_id ON chats (state, machine_id);

CREATE TABLE IF NOT EXISTS chat_events (
    chat_id TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL,
    data_format INTEGER NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (chat_id, seq)
);

-- The finished items of a session's chats, for the files its agents edited.
CREATE INDEX IF NOT EXISTS chat_events_by_chat_id_and_kind ON chat_events (chat_id, kind);

-- What each agent starts with unless chosen otherwise: the last model,
-- reasoning effort and permission mode picked. Server-wide while
-- `user_id` is NULL (see 20261117000001_platform.sql).
CREATE TABLE IF NOT EXISTS chat_defaults (
    agent TEXT NOT NULL,
    user_id TEXT,
    model TEXT,
    effort TEXT,
    mode TEXT,
    updated_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS chat_defaults_unique_server_agent ON chat_defaults (agent) WHERE user_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS chat_defaults_unique_user_agent ON chat_defaults (user_id, agent) WHERE user_id IS NOT NULL;

-- The terminals of terminal sessions, with the plugin that started
-- each, what for, and which plugins changed how it started.
CREATE TABLE IF NOT EXISTS terminals (
    id TEXT NOT NULL PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    command TEXT,
    title TEXT,
    plugin TEXT,
    context TEXT,
    context_format INTEGER NOT NULL DEFAULT 1,
    -- The IDs of the plugins, as a JSON array.
    hooked_by TEXT NOT NULL,
    hooked_by_format INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL,
    exit_code INTEGER,
    col_count INTEGER NOT NULL,
    row_count INTEGER NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS terminals_by_project_id_and_updated_at ON terminals (project_id, updated_at);
CREATE INDEX IF NOT EXISTS terminals_by_session_id ON terminals (session_id);
CREATE INDEX IF NOT EXISTS terminals_by_machine_id_and_state ON terminals (machine_id, state);
CREATE INDEX IF NOT EXISTS terminals_by_plugin ON terminals (plugin);
