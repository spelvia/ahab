-- Searching sessions: the text of every finished message, the person's and
-- the agent's, taken from the chats' `itemEnd` events as they are kept
-- (and, once, from the events kept before this). `search_text` is the text
-- lowercased by the server, so both databases match it the same way.
CREATE TABLE IF NOT EXISTS chat_messages (
    chat_id TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    text TEXT NOT NULL,
    search_text TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (chat_id, seq)
);

CREATE INDEX IF NOT EXISTS chat_messages_by_project_id_and_created_at
    ON chat_messages (project_id, created_at);
CREATE INDEX IF NOT EXISTS chat_messages_by_session_id ON chat_messages (session_id);
