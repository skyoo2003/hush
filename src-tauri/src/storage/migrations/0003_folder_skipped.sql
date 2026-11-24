-- v3. Files the last full scan of a folder did not index: unsupported types and excluded files.

ALTER TABLE folders ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0;
