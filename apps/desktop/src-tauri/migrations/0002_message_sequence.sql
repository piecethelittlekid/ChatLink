CREATE TABLE messages_v2 (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    sender_device_id TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'stored', 'delivered', 'pending_delivery')),
    created_at TEXT NOT NULL,
    delivered_at TEXT
);

INSERT INTO messages_v2 (id, sender_device_id, content, status, created_at, delivered_at)
SELECT id, sender_device_id, content, status, created_at, delivered_at
FROM messages
ORDER BY created_at, id;

DROP TABLE messages;
ALTER TABLE messages_v2 RENAME TO messages;
CREATE INDEX idx_messages_created_at ON messages(created_at);
CREATE UNIQUE INDEX idx_one_paired_device ON devices ((1));
