-- v1. Shipped migrations are never edited; add a new file instead.

CREATE TABLE folders (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    label TEXT,
    added_at INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL
) STRICT;

CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    folder_id INTEGER NOT NULL REFERENCES folders(id),
    rel_path TEXT NOT NULL,
    ext TEXT NOT NULL,
    sha256 BLOB,
    size INTEGER,
    mtime INTEGER,
    status TEXT NOT NULL,
    error_code TEXT,
    indexed_at INTEGER,
    needs_ocr INTEGER NOT NULL DEFAULT 0,
    UNIQUE (folder_id, rel_path)
) STRICT;

-- One row each time a document's hash changes (change history).
CREATE TABLE doc_versions (
    id INTEGER PRIMARY KEY,
    doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    sha256 BLOB NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    seen_at INTEGER NOT NULL
) STRICT;
CREATE INDEX doc_versions_doc_id ON doc_versions(doc_id);

CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    page INTEGER,
    section_path TEXT,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    bbox_json TEXT,
    text TEXT NOT NULL,
    UNIQUE (doc_id, chunk_index)
) STRICT;

CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id INTEGER PRIMARY KEY, embedding float[384]);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

-- Virtual tables get no FK cascade, so every chunk write path keeps them in sync here,
-- including rows removed by the documents -> chunks cascade.
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    DELETE FROM vec_chunks WHERE chunk_id = old.id;
END;
CREATE TRIGGER chunks_au AFTER UPDATE OF text ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE counters (
    key TEXT PRIMARY KEY,
    value INTEGER NOT NULL DEFAULT 0
) STRICT;
