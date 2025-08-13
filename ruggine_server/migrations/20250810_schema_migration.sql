-- =========================================================
-- Schema Ruggine - SQLite
-- FK attive + UUID testo + created_at in RFC3339 UTC
-- =========================================================

PRAGMA foreign_keys = ON;

-- ---------------------------------------------------------
-- Helper: generatore UUID v4 (stringa) via randomblob()
-- (in SQLite non esiste uuid_generate_v4)
-- ---------------------------------------------------------
-- Uso diretto nei DEFAULT delle tabelle, es:
-- DEFAULT (lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-' ||
--                hex(randomblob(2)) || '-' || hex(randomblob(2)) || '-' ||
--                hex(randomblob(6))))

-- ---------------------------------------------------------
-- Tabella: users
-- ---------------------------------------------------------
DROP TABLE IF EXISTS users;
CREATE TABLE users (
    id BLOB NOT NULL PRIMARY KEY
    DEFAULT (randomblob(16)),
    username      TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- ---------------------------------------------------------
-- Tabella: groups
-- ---------------------------------------------------------
DROP TABLE IF EXISTS groups;
CREATE TABLE groups (
    id BLOB NOT NULL PRIMARY KEY
    DEFAULT (randomblob(16)),

    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- ---------------------------------------------------------
-- Tabella ponte: group_members
-- ---------------------------------------------------------
DROP TABLE IF EXISTS group_members;
CREATE TABLE group_members (
    user_id  TEXT NOT NULL,
    group_id TEXT NOT NULL,
    PRIMARY KEY (user_id, group_id),
    FOREIGN KEY (user_id)  REFERENCES users(id)  ON DELETE CASCADE,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
);

-- Indici utili per join/ricerche
CREATE INDEX IF NOT EXISTS idx_group_members_user ON group_members(user_id);
CREATE INDEX IF NOT EXISTS idx_group_members_group ON group_members(group_id);

-- ---------------------------------------------------------
-- Tabella: group_invitations
-- ---------------------------------------------------------
DROP TABLE IF EXISTS group_invitations;
CREATE TABLE group_invitations (
    id BLOB NOT NULL PRIMARY KEY
    DEFAULT (randomblob(16)),

    group_id         TEXT NOT NULL,
    inviter_id       TEXT NOT NULL,
    invited_user_id  TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','accepted','declined')),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY (group_id)        REFERENCES groups(id) ON DELETE CASCADE,
    FOREIGN KEY (inviter_id)      REFERENCES users(id)  ON DELETE CASCADE,
    FOREIGN KEY (invited_user_id) REFERENCES users(id)  ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_group_invitations_group   ON group_invitations(group_id);
CREATE INDEX IF NOT EXISTS idx_group_invitations_invited ON group_invitations(invited_user_id);
CREATE INDEX IF NOT EXISTS idx_group_invitations_inviter ON group_invitations(inviter_id);
CREATE INDEX IF NOT EXISTS idx_group_invitations_status  ON group_invitations(status);

-- ---------------------------------------------------------
-- Tabella: group_messages
-- ---------------------------------------------------------
DROP TABLE IF EXISTS group_messages;
CREATE TABLE group_messages (
    id BLOB NOT NULL PRIMARY KEY
    DEFAULT (randomblob(16)),

    group_id   TEXT NOT NULL,
    user_id  TEXT, -- Possibilmente null per i messaggi di sistema
    content    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY (group_id)  REFERENCES groups(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id)  ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_group_messages_group_time ON group_messages(group_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_group_messages_user       ON group_messages(user_id);

-- =========================================================
-- ✅ Test rapido (opzionale): decommenta per provare
-- =========================================================
-- INSERT INTO users (username, password_hash) VALUES ('mario', 'hash1');
-- INSERT INTO users (username, password_hash) VALUES ('luigi', 'hash2');
-- INSERT INTO groups (name) VALUES ('Il Mio Gruppo Fantastico');
-- -- Aggiungi membri
-- INSERT INTO group_members (user_id, group_id)
-- SELECT u.id, g.id FROM users u, groups g WHERE u.username='mario' AND g.name='Il Mio Gruppo Fantastico';
-- -- Messaggio
-- INSERT INTO group_messages (group_id, sender_id, content)
-- SELECT g.id, u.id, 'Ciao a tutti!' FROM users u, groups g WHERE u.username='mario' AND g.name='Il Mio Gruppo Fantastico';
-- -- Invito
-- INSERT INTO group_invitations (group_id, inviter_id, invited_user_id)
-- SELECT g.id, u1.id, u2.id
-- FROM groups g, users u1, users u2
-- WHERE g.name='Il Mio Gruppo Fantastico' AND u1.username='mario' AND u2.username='luigi';
