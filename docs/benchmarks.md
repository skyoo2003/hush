# Benchmarks

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
