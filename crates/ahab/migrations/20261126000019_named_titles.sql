-- Sessions and chats someone renamed. An agent's title (Stubb's `title`
-- event) replaces the one taken from the first message, never one of these.
CREATE TABLE IF NOT EXISTS named_sessions (
    session_id TEXT NOT NULL PRIMARY KEY REFERENCES sessions (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS named_chats (
    chat_id TEXT NOT NULL PRIMARY KEY REFERENCES chats (id) ON DELETE CASCADE
);
