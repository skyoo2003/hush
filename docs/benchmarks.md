# Benchmarks

## Beta stress set: 2,000 files (2026-11-26)

- Machine: Apple M4, 10 cores, 16 GB, on AC power. Queue defaults (5 parse workers, one embed thread).
- Model: bge-small-en-v1.5 on candle CPU (Accelerate). Hybrid search, limit 10, no reranker.
- Corpus: `swift scripts/make-stress-corpus.swift <dir>`, deterministic, built from the public license and policy texts in `eval/corpus`:

| Files | Kind |
|---|---|
| 700 | `.txt`, about 3 KB |
| 368 | `.md`, about 3 KB |
| 300 | `.docx` (textutil), about 3 KB |
| 400 | text PDFs, 1 to 4 pages |
| 200 | scanned PDFs (10%): 1 or 2 bitmap pages at 150 dpi, no text layer, so every page goes through OCR |
| 2 | text PDFs of 520 and 610 pages |
| 10 | password-protected PDFs |
| 10 | 0-byte files (`.txt`, `.md`, `.pdf`, `.docx`, `.png`) |
| 10 | broken `.docx`: 5 truncated zips, 5 text files named `.docx` |

- Command: `/usr/bin/time -l cargo run --release --example stress_bench -- <corpus> <work>`. Indexing time is from `rescan` to an idle queue. DB size is the DB, WAL, and SHM after `wal_checkpoint(TRUNCATE)`. Search is 300 searches over 10 queries after indexing, three warm-ups not counted. Peak RSS is `maximum resident set size` from `time -l`, which includes the model (about 130 MB) and PDFium.

| Run | Indexing | Indexed | Failed (reason) | Chunks | DB size | Peak RSS | Search p50 / p95 / p99 |
|---|---|---|---|---|---|---|---|
| v0.1.0 registry (PDFs and images ignored) | 250 s | 1,372 | 12 corrupted; 616 files never admitted | 3,824 | 15.4 MB | 1.25 GB | 11 / 13 / 13 ms |
| Full registry, embed batch 32 | 1,345 s | 1,974 | 16 corrupted, 10 encrypted | 12,135 | 50.4 MB | 5.30 GB | 15 / 19 / 19 ms |
| Full registry, embed batch 8 (shipped) | 1,369 s | 1,974 | 16 corrupted, 10 encrypted | 12,135 | 50.4 MB | 2.16 GB | 15 / 19 / 20 ms |

`integrity_check` returned `ok` after every run. The 16 corrupted are the 10 broken DOCX plus the six 0-byte files of a type with a magic number (`.pdf`, `.docx`, `.png`, two of each). 0-byte `.txt` and `.md` index as empty documents.

### What the numbers say

- Peak memory came from embedding, not parsing or OCR. Parsing and chunking the 610-page PDF alone peaks at 0.41 GB (`examples/chunk`). The 200 scanned PDFs alone peak at 1.47 GB. The two long PDFs alone peaked at 5.22 GB: their chunks are all near the 512-token limit, and attention memory grows with batch x tokens. On the 610-page PDF alone, batch 32 peaked at 4.37 GB and batch 8 at 1.90 GB, with indexing time 328 s and 344 s (run side by side, so within noise). The full run with batch 8 took 2% longer than with batch 32.
- Time is dominated by OCR and the long PDFs. The 200 scanned PDFs alone took 1,238 s (about 4 s per page, run beside other benchmarks, so an upper bound). The 610-page PDF alone took 344 s, one embed thread at about 8 chunks/s.
- Search p95 stays under 20 ms idle at 12,000 chunks, far below the 300 ms target.

### Force-quit during indexing

`stress_bench <corpus> <work> crash 300` starts a full run as a child process, sends SIGKILL after 300 s (no destructor, no checkpoint), reopens the DB, runs `integrity_check`, counts unfinished documents (`pending`, or `indexed` with no hash), rescans, and fails unless no document is left unfinished, no chunk lacks its document, no indexed document lacks chunks, and `integrity_check` passes again. Both checks run on the writer connection: `PRAGMA integrity_check` and FTS5's own `integrity-check` with rank 1, which also compares the keyword index with the `chunks` rows.

| Stage | integrity_check | FTS5 check | Indexed | Pending (unfinished) | Failed | Chunks |
|---|---|---|---|---|---|---|
| Right after SIGKILL at 300 s | ok | ok | 505 | 11 | 4 corrupted, 2 encrypted | 1,805 |
| After resume (1,129 s) | ok | ok | 1,974 | 0 | 16 corrupted, 10 encrypted | 12,135 |

The end state matches a clean run exactly, with no orphan chunks and no non-empty indexed document without chunks. The 11 pending rows are files a parse worker had claimed; their chunks and hash had not committed, so the rescan treated them as new. A 400-file text subset killed after 15 s gave the same result three times.

False alarm worth knowing: a first version of this check ran `PRAGMA integrity_check` on a reader connection, before and after resuming. The second run reported `fts5: corruption found reading blob … from table "chunks_fts"` every time, even without a crash (index 200 files, add 200, check again). The same DB checked from a fresh connection was clean, and on that same reader a `MATCH` count equaled the writer's (630 and 630), so searches were right. The report needs a connection whose only FTS5 use was an earlier integrity_check, made stale by writes from other connections. The app never runs integrity_check, so this affects only tools; run integrity checks on the writer or a fresh connection.

### Not measured

- An 8 GB or M1 Mac, and battery power. The peak RSS above is on 16 GB, where macOS does not press on memory.
- Korean text and the multilingual model.
- Search while this set is indexing (the scheduler benchmark below covers that for `.txt`).

## Search latency while indexing (index scheduler, ADR-019)

- Date: 2026-11-25
- Machine: Apple M4, 10 cores, 16 GB, on AC power. The queue spawns 5 parse workers (`cores / 2`).
- Command: `cargo run --release --example sched_bench -- 60` (needs `scripts/fetch-models.sh`).
- Model: bge-small-en-v1.5 on candle CPU (Accelerate). Hybrid search, limit 10, no reranker.
- Corpus: synthetic `.txt` files of about 3 KB, shuffled sentences from the license texts in `eval/corpus`. A 1,000-document seed folder is indexed first (181 s, 5.5 documents/s at full speed). Then each run indexes a fresh 1,000-document folder while one thread searches every 150 ms for 60 s, cycling 10 queries. Three warm-up searches per run are not counted.
- Latency is the `SearchService::search` call (query embedding, vector KNN, FTS, fusion). It does not include IPC or rendering.

| Run | Plan | Searches | p50 ms | p95 ms | p99 ms | max ms | Documents indexed in the 60 s |
|---|---|---|---|---|---|---|---|
| Scheduler OFF | 5 parse workers, embedding on | 389 | 35 | 49 | 58 | 65 | 290 |
| Scheduler ON, Normal | 2 parse workers, embedding on | 388 | 40 | 49 | 67 | 84 | 294 |
| Scheduler ON, Interactive (search open) | 1 parse worker, embedding held | 388 | 32 | 36 | 38 | 55 | 0 |

Target: p95 at or below 300 ms while indexing. Met in every run, including with the scheduler off.

### What the numbers say

- Interactive lowers p95 from 49 to 36 ms and p99 from 58 to 38 ms. With embedding held, a search shares the CPU only with one parser.
- The limit on indexing speed is the single embed thread, not the parse workers. Normal (2 workers) indexed as many documents as OFF (5 workers). Normal costs no throughput on this corpus.
- Interactive indexes nothing until search closes. That is the intended trade: in the app it lasts only while the quick-search window is open.
- The query-first gate in `BertEmbedder` showed no measurable effect here. A second run with the gate removed (a throwaway patch, not committed) gave OFF p95 45 ms, Normal 54 ms, Interactive 37 ms: within run-to-run noise of the table above. On 10 cores a query's forward pass runs beside a passage batch without waiting for it. The gate is kept for slower machines, where it is not yet measured.

### Not measured

- PDFs and OCR. Parsing them is far heavier than `.txt`, so parse workers would compete with search more than here.
- An 8 GB or M1 Mac, and battery power.
- Latency through the webview (IPC and rendering).
