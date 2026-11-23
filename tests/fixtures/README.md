# Test fixtures

This folder holds no fixture files yet, only `.gitkeep`. Tests build their inputs in code and write them to temporary directories:

- PDF tests in `src-tauri/src/ingest/pdf.rs` create PDFs with PDFium (`build`) using synthetic text.
- Image and OCR tests in `src-tauri/src/ingest/image.rs` encode synthetic images with the `image` crate.
- Ingestion, watcher, and search tests write short made-up text files into a `tempfile` directory.

The retrieval eval corpus lives in `eval/corpus/`, and the source and license of each file are listed in [eval/README.md](../../eval/README.md).

## Adding a fixture

Prefer generating the input in the test. When a file must be checked in:

1. Make it yourself from invented text, or take it from a public source whose license allows redistribution in a GPL-3.0-or-later project.
2. Never use a real private document, a real query, a real file path, or a key, even if it seems harmless.
3. Add a row to the table below in the same change.

| File | Source | License |
| --- | --- | --- |
| (none yet) | | |
