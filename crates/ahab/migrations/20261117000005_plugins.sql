-- Plugins: the installed ones, the machines a plugin runs on when
-- it asks to be placed, and their settings per scope.
CREATE TABLE IF NOT EXISTS plugins (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    description TEXT,
    source TEXT NOT NULL,
    location TEXT NOT NULL,
    digest TEXT NOT NULL,
    manifest TEXT NOT NULL,
    manifest_format INTEGER NOT NULL DEFAULT 1,
    enabled_at BIGINT,
    -- Its place among interceptors, as the person ordered them (the API's
    -- `position`).
    run_order INTEGER NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS plugin_machines (
    plugin_id TEXT NOT NULL REFERENCES plugins (id) ON DELETE CASCADE,
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    PRIMARY KEY (plugin_id, machine_id)
);

CREATE INDEX IF NOT EXISTS plugin_machines_by_machine_id ON plugin_machines (machine_id);

-- A scope is `server` (scope_id ''), `machine` or `project` (scope_id the
-- ID), and later `user` (scope_id the user's ID).
CREATE TABLE IF NOT EXISTS plugin_settings (
    plugin_id TEXT NOT NULL REFERENCES plugins (id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    setting_values TEXT NOT NULL,
    setting_values_format INTEGER NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (plugin_id, scope, scope_id)
);
