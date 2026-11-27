-- Fast mode per chat (Claude Code's and Codex's faster tier, where the
-- model offers it): chosen at the start or between turns, and kept so a
-- resumed chat goes on with it. No row: the provider's default.
CREATE TABLE IF NOT EXISTS chat_fast_modes (
    chat_id TEXT NOT NULL PRIMARY KEY REFERENCES chats (id) ON DELETE CASCADE,
    fast BOOLEAN NOT NULL
);
