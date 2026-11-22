-- v2. Per-chunk content hash.

ALTER TABLE chunks ADD COLUMN sha256 BLOB;
