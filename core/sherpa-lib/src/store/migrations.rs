//! Версионированная схема БД. Миграции только вперёд; каждая — в транзакции.

/// Миграция N переводит схему из N-1 в N (индекс + 1).
pub const MIGRATIONS: &[&str] = &[
    // 1: базовая схема MVP «Подключения».
    r#"
    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE groups (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        name_lc    TEXT NOT NULL,
        parent_id  TEXT REFERENCES groups(id),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX groups_parent ON groups(parent_id);

    CREATE TABLE connections (
        id                 TEXT PRIMARY KEY,
        name               TEXT NOT NULL,
        name_lc            TEXT NOT NULL UNIQUE,
        host               TEXT NOT NULL,
        port               INTEGER NOT NULL,
        user               TEXT,
        auth_method        TEXT NOT NULL,
        key_path           TEXT,
        key_type           TEXT,
        public_fingerprint TEXT,
        secret_password    BLOB,
        secret_key_text    BLOB,
        secret_passphrase  BLOB,
        description        TEXT,
        group_id           TEXT REFERENCES groups(id),
        source             TEXT NOT NULL,
        status_state       TEXT NOT NULL DEFAULT 'unknown',
        status_checked_at  TEXT,
        status_latency_ms  INTEGER,
        status_error       TEXT,
        created_at         TEXT NOT NULL,
        updated_at         TEXT NOT NULL
    );
    CREATE INDEX connections_group ON connections(group_id);

    CREATE TABLE connection_tags (
        connection_id TEXT NOT NULL REFERENCES connections(id) ON DELETE CASCADE,
        tag           TEXT NOT NULL,
        PRIMARY KEY (connection_id, tag)
    );
    CREATE INDEX connection_tags_tag ON connection_tags(tag);

    CREATE TABLE host_keys (
        id                   INTEGER PRIMARY KEY,
        host                 TEXT NOT NULL,
        port                 INTEGER NOT NULL,
        key_type             TEXT NOT NULL,
        fingerprint          TEXT NOT NULL,
        state                TEXT NOT NULL,
        first_seen_at        TEXT NOT NULL,
        accepted_at          TEXT,
        replaced_fingerprint TEXT,
        UNIQUE (host, port, fingerprint)
    );
    "#,
    // Библиотеки рабочего стола и журнал: отдельные пространства имён.
    r#"
    CREATE TABLE desktop_documents (
        kind TEXT NOT NULL, id TEXT NOT NULL, connection_id TEXT,
        body TEXT NOT NULL, secret BLOB, updated_at TEXT NOT NULL,
        PRIMARY KEY(kind, id)
    );
    CREATE INDEX desktop_documents_connection ON desktop_documents(connection_id);
    CREATE TABLE desktop_audit (
        id TEXT PRIMARY KEY, timestamp TEXT NOT NULL, client TEXT NOT NULL,
        connection_id TEXT, method TEXT NOT NULL, outcome TEXT NOT NULL
    );
    CREATE INDEX desktop_audit_time ON desktop_audit(timestamp);
    "#,
];

pub fn latest() -> u32 {
    MIGRATIONS.len() as u32
}
