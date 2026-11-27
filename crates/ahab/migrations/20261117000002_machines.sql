-- Machines: the server's record of each computer that runs a
-- daemon.
CREATE TABLE IF NOT EXISTS machines (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    is_local BOOLEAN NOT NULL DEFAULT FALSE,
    public_key BYTEA NOT NULL,
    os TEXT,
    arch TEXT,
    version TEXT,
    created_at BIGINT NOT NULL,
    approved_at BIGINT,
    last_seen_at BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS machines_unique_name ON machines (name);
CREATE UNIQUE INDEX IF NOT EXISTS machines_unique_public_key ON machines (public_key);
-- At most one local machine.
CREATE UNIQUE INDEX IF NOT EXISTS machines_unique_local ON machines (is_local) WHERE is_local = TRUE;
-- Pending machines that waited too long are removed as others join.
CREATE INDEX IF NOT EXISTS machines_by_state_and_created_at ON machines (state, created_at);

-- One-time codes that approve a joining machine at once. Only
-- their hash is stored.
CREATE TABLE IF NOT EXISTS join_codes (
    id TEXT NOT NULL PRIMARY KEY,
    code_hash BYTEA NOT NULL,
    name TEXT,
    expires_at BIGINT NOT NULL,
    used_by TEXT,
    created_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS join_codes_unique_code_hash ON join_codes (code_hash);

-- The latest agent report per machine.
CREATE TABLE IF NOT EXISTS machine_agents (
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    state TEXT NOT NULL,
    version TEXT,
    path TEXT,
    account TEXT,
    detail TEXT,
    label TEXT,
    checked_at BIGINT NOT NULL,
    PRIMARY KEY (machine_id, agent)
);

-- The ACP agents added to each machine, from the registry or custom.
CREATE TABLE IF NOT EXISTS machine_added_agents (
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    name TEXT NOT NULL,
    launch TEXT,
    launch_format INTEGER NOT NULL DEFAULT 1,
    added_at BIGINT NOT NULL,
    PRIMARY KEY (machine_id, agent)
);

-- Each agent's models on each machine, as last read.
CREATE TABLE IF NOT EXISTS agent_models (
    machine_id TEXT NOT NULL REFERENCES machines (id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    models TEXT NOT NULL,
    models_format INTEGER NOT NULL DEFAULT 1,
    fetched_at BIGINT NOT NULL,
    PRIMARY KEY (machine_id, agent)
);
