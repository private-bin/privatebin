CREATE TABLE pastes (
    id          TEXT PRIMARY KEY NOT NULL,
    data        BLOB NOT NULL,     -- {"v":2,"adata":...,"ct":...}
    expire_at   INTEGER NULL,      -- unix seconds, NULL = never
    burn        INTEGER NOT NULL,
    discussion  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE INDEX idx_pastes_expire ON pastes(expire_at);

CREATE TABLE comments (
    id          TEXT PRIMARY KEY NOT NULL,
    paste_id    TEXT NOT NULL REFERENCES pastes(id) ON DELETE CASCADE,
    parent_id   TEXT NOT NULL,
    data        BLOB NOT NULL,     -- {"v":2,"adata":...,"ct":...}
    created_at  INTEGER NOT NULL
);

CREATE INDEX idx_comments_paste ON comments(paste_id, created_at);
