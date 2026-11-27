-- Many repositories: when each repository or worktree of a
-- project was last used (a file in it opened, or changed by an agent's
-- turn), and whether it is pinned. `worktree` is '' for a repository's own
-- checkout (or the project's folder), else the worktree's folder; both
-- relative to the project's folder. Deleted with the project.
CREATE TABLE IF NOT EXISTS repository_activity (
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    repository TEXT NOT NULL,
    worktree TEXT NOT NULL DEFAULT '',
    last_active_at BIGINT,
    pinned BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (project_id, repository, worktree)
);
