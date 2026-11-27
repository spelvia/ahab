-- Sessions with several chats and terminals: the name you give a
-- terminal, kept apart from the title its shell sets (which changes as it
-- runs). A terminal without one shows its shell's title or its command.
CREATE TABLE IF NOT EXISTS terminal_labels (
    terminal_id TEXT NOT NULL PRIMARY KEY REFERENCES terminals (id) ON DELETE CASCADE,
    label TEXT NOT NULL
);
