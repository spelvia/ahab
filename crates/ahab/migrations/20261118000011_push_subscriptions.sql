-- Web Push: browsers that asked for notifications while Ahab is
-- closed. Each belongs to the paired device that registered it; revoking
-- it stops its pushes, and purging it removes them. `keys` holds the
-- browser's `p256dh` and `auth` (JSON, `keys_format`). A push service
-- answering 404 or 410 removes the subscription.
CREATE TABLE IF NOT EXISTS push_subscriptions (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL UNIQUE,
    keys TEXT NOT NULL,
    keys_format INTEGER NOT NULL DEFAULT 1,
    label TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT
);
CREATE INDEX IF NOT EXISTS push_subscriptions_by_device ON push_subscriptions (device_id);
