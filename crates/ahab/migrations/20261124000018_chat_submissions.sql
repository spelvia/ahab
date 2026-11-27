-- Durable queue and receipts for message submissions.
CREATE TABLE IF NOT EXISTS chat_submissions (
    chat_id TEXT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    state TEXT NOT NULL,
    queue_position BIGINT,
    after_seq BIGINT,
    interrupting BOOLEAN NOT NULL,
    shown TEXT NOT NULL,
    prompt TEXT NOT NULL,
    command TEXT NOT NULL DEFAULT 'null',
    command_format INTEGER NOT NULL DEFAULT 1,
    attachments TEXT NOT NULL,
    attachments_format INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (chat_id, id)
);

CREATE INDEX IF NOT EXISTS chat_submissions_queue
    ON chat_submissions (chat_id, state, queue_position);

CREATE UNIQUE INDEX IF NOT EXISTS chat_submissions_sending
    ON chat_submissions (chat_id) WHERE state = 'sending' AND queue_position IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS chat_submissions_dispatched
    ON chat_submissions (chat_id, after_seq)
    WHERE state IN ('sending', 'sent', 'unconfirmed') AND queue_position IS NOT NULL;
