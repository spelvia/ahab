-- How a project looks in lists: where it sits when you order them by hand
-- (none: after those, by name), and its own mark instead of its initial,
-- an emoji or a small image (base64, at most 256 KiB). No row: by name,
-- with its initial.
CREATE TABLE IF NOT EXISTS project_looks (
    project_id TEXT NOT NULL PRIMARY KEY REFERENCES projects (id) ON DELETE CASCADE,
    position BIGINT,
    emoji TEXT,
    image_type TEXT,
    image TEXT,
    icon_at BIGINT
);
