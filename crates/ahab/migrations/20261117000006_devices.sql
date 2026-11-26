-- Devices: the browsers, phones, command lines and scripts paired
-- with this server, each with its own credential. Only the credential's
-- SHA-256 is stored. Revoked devices are kept for 30 days, then deleted.
CREATE TABLE IF NOT EXISTS devices (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    credential_hash BYTEA NOT NULL,
    paired_by TEXT,
    paired_via TEXT NOT NULL,
    user_agent TEXT,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT,
    last_address TEXT,
    encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at BIGINT
);

CREATE UNIQUE INDEX IF NOT EXISTS devices_unique_credential_hash ON devices (credential_hash);
CREATE INDEX IF NOT EXISTS devices_by_revoked_at ON devices (revoked_at);

-- Open pairing links, codes and requests. Only hashes of their
-- secrets are stored; a request's short code is shown to whoever approves
-- it, so it is kept as it is too.
CREATE TABLE IF NOT EXISTS pairings (
    id TEXT NOT NULL PRIMARY KEY,
    kind TEXT NOT NULL,
    secret_hash BYTEA NOT NULL,
    code_hash BYTEA,
    code TEXT,
    created_by TEXT,
    approved_by TEXT,
    device_name TEXT,
    device_kind TEXT,
    user_agent TEXT,
    address TEXT,
    state TEXT NOT NULL,
    device_id TEXT,
    expires_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS pairings_unique_secret_hash ON pairings (secret_hash);
CREATE INDEX IF NOT EXISTS pairings_by_code_hash ON pairings (code_hash);
CREATE INDEX IF NOT EXISTS pairings_by_state ON pairings (state, expires_at);
