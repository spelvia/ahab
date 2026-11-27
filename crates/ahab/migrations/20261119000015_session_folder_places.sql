-- Folders in folders, and pinned folders: where a folder sits (its parent,
-- none for the project's top level) and when it was pinned (pinned ones
-- come first). No row: a top-level folder, not pinned.
CREATE TABLE IF NOT EXISTS session_folder_places (
    folder_id TEXT NOT NULL PRIMARY KEY REFERENCES session_folders (id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES session_folders (id) ON DELETE SET NULL,
    pinned_at BIGINT
);

CREATE INDEX IF NOT EXISTS session_folder_places_by_parent_id ON session_folder_places (parent_id);
