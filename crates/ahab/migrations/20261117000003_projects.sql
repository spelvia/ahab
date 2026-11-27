-- Projects: one folder on one machine each.
--
-- `mode` is `read-write` or `read-only`; `group` is the optional heading
-- the project is listed under. Names need not be unique, and neither need
-- folders: the same folder may be several projects, on one machine or on
-- several.
CREATE TABLE IF NOT EXISTS projects (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    state TEXT NOT NULL,
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    mode TEXT NOT NULL,
    project_group TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- A machine's projects, and those of one of its folders.
CREATE INDEX IF NOT EXISTS projects_by_machine_id_and_path ON projects (machine_id, path);

-- Each project keeps one layout of windows per kind of screen.
CREATE TABLE IF NOT EXISTS project_layouts (
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    client_class TEXT NOT NULL,
    layout TEXT NOT NULL,
    layout_format INTEGER NOT NULL DEFAULT 1,
    previous TEXT,
    previous_format INTEGER NOT NULL DEFAULT 1,
    revision BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (project_id, client_class)
);
