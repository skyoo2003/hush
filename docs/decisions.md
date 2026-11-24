# Decision records (ADR)

## ADR-001. Storage is a single DB: SQLCipher + sqlite-vec (static) + FTS5

- Date: 2026-11-22
- Status: Accepted
- Evidence: `spikes/sqlcipher_vec/` (throwaway spike, not product code)

### Decision

Vectors (`vec0`), the keyword index (FTS5), and metadata live in one SQLCipher-encrypted DB file. sqlite-vec is registered statically with `sqlite3_auto_extension`; no dylib is loaded. When opening a connection, set `PRAGMA cache_size` to at least the size of the vector table. Without it, KNN is 10x slower (measured below).

### Verified combination

| Component | Version |
| --- | --- |
| `rusqlite` | 0.43.0, feature `bundled-sqlcipher-vendored-openssl` |
| `libsqlite3-sys` | 0.40.0 |
| SQLCipher | 4.20.0 community, SQLite 3.54.1 |
| OpenSSL (`openssl-src`) | 3.6.3, static build |
| `sqlite-vec` | 0.1.9 |
| Platform | Apple M4 (arm64), 10 cores, 16 GB, macOS (Darwin 25.6.0), Rust 1.98.1 |

The `sqlite-vec` crate compiles `sqlite-vec.c` with `SQLITE_CORE` and does not bundle SQLite itself. Its `sqlite3_*` symbols therefore link against libsqlite3-sys's static SQLCipher library. `otool -L` lists only `libiconv` and `libSystem`, and `nm` shows exactly one `sqlite3_open_v2` and one `sqlite3_vec_init` in the binary.

### Verification results

Rerun with `cargo run --release -- quick`. Each item is checked with `assert`.

1. Created `vec0(embedding float[384])` and `fts5(body)` in an encrypted DB and inserted 1,000 rows into each. KNN with row 1's vector returns 10 rows; rank 1 is row 1 with distance 0.0, and distances are ascending. `MATCH 'topic3'` returns 100 rows and `MATCH 'document AND "17"'` returns 1. `sqlite_compileoption_used('ENABLE_FTS5')` is 1. **Pass.**
2. Opening with a wrong key or no key fails on the first read with `SQLITE_NOTADB` (26, "file is not a database"). Reopening with the right key reads 1,000 rows. **Pass.**
3. With the connection open and no checkpoint yet (WAL at 1.86 MB), searched the DB, `-wal`, and `-shm` bytes case-insensitively. `confidential-marker`, the FTS5 token `confidential`, the body `secret body`, and the plaintext header `sqlite format 3` all have 0 hits, and still 0 after closing the connection. As a control, the same data in a plaintext DB gives the scanner 5 hits. **Pass.**
4. KNN latency is below.

### KNN top-10 latency

Method: random unit-normalized 384-dim vectors with L2 distance. For each N, create a new DB, insert, run `wal_checkpoint(TRUNCATE)`, and open a fresh connection. After 3 warm-up queries, measure 20 queries × 5 rounds = 100 queries, checking that each returns 10 rows. The same binary ran twice; run 2's p50 is within 2% of run 1's. Per-round p50 varies by under 4%. `softwareupdated` used one core during measurement; KNN is single-threaded and there are 10 cores.

p50 ms (run 1 / run 2); p95 is from run 2.

| N | Encrypted, default cache (2 MB) | Encrypted, `cache_size=-1048576` (1 GiB) | Plaintext, default cache | DB size |
| --- | --- | --- | --- | --- |
| 1,000 | 0.16 / 0.16 (p95 0.18) | 0.18 / 0.17 (p95 0.18) | 0.28 / 0.28 | 2 MB |
| 100,000 | 183.1 / 179.9 (p95 184.2) | 16.9 / 16.7 (p95 16.9) | 30.3 / 29.6 | 160 MB |
| 300,000 | 545.3 / 541.5 (p95 547.9) | 50.7 / 50.6 (p95 51.2) | 90.5 / 89.9 | 478 MB |

Inserting 300,000 rows took 3.4 s in one transaction (vectors only).

**Latency is linear in N.** `vec0` in sqlite-vec 0.1.9 has no ANN index, so KNN scans every chunk. Going from 100k to 300k roughly triples latency in all three configurations.

**With the default cache, the limit is page HMAC verification.** Sampled the 100k default-cache run with macOS `sample` for 12 s (a separate run from the reported numbers). Top stacks: `sha512_block_armv8` 56%, `pread` 12%, `vec0Filter_knn_chunks_iter` (distance computation) 8%, `aes_v8_cbc_encrypt` 7%. With a 2 MB cache, every query rereads the whole 160 MB from the OS disk cache, verifies it with HMAC-SHA512, and decrypts it. A cache larger than the DB keeps decrypted pages in memory. Latency then drops 10.7x and beats even plaintext with the default cache, because plaintext still copies pages with `pread` on every query. The limit in the large-cache configuration is presumably distance computation and memory bandwidth; that configuration was not profiled.

### Capacity estimate

The large-cache configuration costs 0.169 µs per vector (50.6 ms / 300k) and about 1.6 KB of cache memory per vector (478 MB / 300k). The values below scale the measurements linearly.

| Query budget | Max vectors | Page cache needed |
| --- | --- | --- |
| 50 ms | ~300k (measured) | ~480 MB |
| 100 ms | ~590k (estimate) | ~940 MB (estimate) |
| 100 ms, default cache | ~55k (estimate) | 2 MB |

On a 16 GB machine, RAM sets the cap before latency does. Converting to a document count needs chunks per document. Assuming 20 chunks per document (an unfounded assumption), 300k vectors is 15,000 documents. FTS5 and reranker time for hybrid search are not included in these numbers.

### Consequences

- Add `cache_size` to the connection setup. Size it to the vector table and cap memory.
- Decrypted pages live only in process memory and are never written to a file, so absolute rule 4 holds. Swapped memory does reach disk, but macOS encrypts swap by default.
- The bundled SQLite is built with `SQLITE_ENABLE_LOAD_EXTENSION=1`, but `load_extension` is off at runtime by default. The app never calls `enable_load_extension`.
- Per "no plaintext caches or temp files" (rule 4), product code also verifies `PRAGMA temp_store=MEMORY`. This spike did not check it.

### Alternatives considered

The combination works, so these are not adopted. The comparison is kept for reference.

| Alternative | Pros | Cons |
| --- | --- | --- |
| BLOB column + Rust brute force | No dependency | Same full scan as `vec0`. Pays the same HMAC cost, or needs a separate copy of the vectors in Rust memory. |
| BLOB column + Rust HNSW | ~1 ms queries at 1M+ | The index must be rebuilt in memory, or serialized inside the DB because plaintext files are forbidden. One more crate. |
| LanceDB | Out of scope | Not evaluated, per instructions. |

### Quantization (measured 2026-11-22)

Rerun with `cargo run --release -- quant`. One encrypted DB per N holds four `vec0` tables with the same vectors: `float[384]`, `int8[384]` via `vec_quantize_int8(v, 'unit')`, `int8[384]` pre-scaled in Rust (`round(x * 508)`, i.e. [-0.25, 0.25] → int8), and `bit[384]` via `vec_quantize_binary`. Ground truth is exact float top-10. Each config gets a fresh connection, 3 warm-up queries, then 100 timed queries; every query must return 10 rows. `bit+rerank100` takes bit top-100 and reorders it by float L2 from the float table. Three full runs; p50 varied under 4% between runs, and the float rows match the KNN table above.

p50 ms (p95 in parentheses), run 3. Recall@10 is the mean over 100 queries.

| N | Config | Vector bytes | Default cache | 1 GiB cache | Recall@10 |
| --- | --- | --- | --- | --- | --- |
| 100k | float | 154 MB | 179.8 (181.4) | 17.0 (17.1) | 1.000 |
| 100k | int8 `'unit'` | 38.5 MB | 56.3 (56.9) | 14.7 (15.0) | 0.829 |
| 100k | int8 pre-scaled | 38.5 MB | 56.3 (56.6) | 14.7 (14.9) | 0.964 |
| 100k | bit | 4.8 MB | 6.8 (6.9) | 0.91 (0.93) | 0.147 |
| 100k | bit + float rerank of 100 | 4.8 MB + float | 92.4 (100.8) | 8.0 (10.2) | 0.451 |
| 300k | float | 461 MB | 541.8 (545.6) | 53.8 (54.6) | 1.000 |
| 300k | int8 `'unit'` | 115 MB | 168.6 (170.9) | 44.7 (45.0) | 0.825 |
| 300k | int8 pre-scaled | 115 MB | 168.9 (171.1) | 44.8 (45.3) | 0.970 |
| 300k | bit | 14.4 MB | 20.7 (21.0) | 2.6 (2.6) | 0.101 |
| 300k | bit + float rerank of 100 | 14.4 MB + float | 120.4 (128.2) | 21.9 (38.0) | 0.340 |

**Recall is on random vectors, not real embeddings.** Random isotropic vectors are a hard case for quantization, so recall on real embeddings may differ in either direction. Recheck with the real embedding model before choosing.

- **int8 needs scaling.** `'unit'` maps [-1, 1], but components of a 384-dim unit vector sit near ±0.05, so most of the int8 range is unused. Pre-scaling raises recall from 0.83 to 0.96–0.97 at the same latency. The scale must be calibrated on real embeddings.
- **Cold or small cache: latency follows bytes.** With the default cache, int8 is 3.2x faster than float, close to its 4x byte reduction, because HMAC verification is the limit (see above).
- **Warm cache: int8 saves memory, not time.** With 1 GiB, int8 is only 13–17% faster than float. A 15 s `sample` of an isolated int8 run (300k, 1 GiB, not reported) puts 90% of samples in `vec0Filter_knn_chunks_iter` (distance computation) and 7% in memmove, with no HMAC or pread. The `sqlite-vec` crate's `build.rs` does not define `SQLITE_VEC_ENABLE_NEON`, so its NEON distance functions are compiled out for both float and int8. The gain from enabling them is not measured.
- **bit alone is not usable at this recall**, and rerank of 100 still recalls 0.34–0.45 here. The rerank p95 is 1.7x its p50 with a warm cache, presumably from float pages the bit scan never warms (not profiled).

Decision: stay on `float[384]` for now. Revisit int8 (pre-scaled) when real embeddings exist. It cuts the cache needed per vector by 4x (about 300k vectors in ~120 MB of vector bytes) with recall to be confirmed.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `rusqlite` | SQLite/SQLCipher bindings | MIT | None at runtime. Downloaded from crates.io at build time. |
| `libsqlite3-sys` (`bundled-sqlcipher-vendored-openssl`) | Builds SQLCipher and OpenSSL statically | MIT. SQLCipher community is BSD-style and needs attribution. | None at runtime |
| `openssl-src` (transitive) | Builds without a system OpenSSL | Apache-2.0 (OpenSSL 3.x) | None at runtime. Sources ship inside the crate. |
| `sqlite-vec` | `vec0` virtual table; links the C source statically | MIT OR Apache-2.0 | None at runtime |

## ADR-002. App scaffold: Tauri v2 + React/TS + Vite, two windows with per-window ACL

- Date: 2026-11-22
- Status: Accepted

### Decision

- There are two windows, `main` and `quick-search`. Both load `index.html`, and `App.tsx` picks the UI from `getCurrentWindow().label`.
- The future `/compare`, `/pack`, and `/viewer` views are routes inside the `main` window, not new windows. There is only one screen today, so no router yet; pick one when the routes exist.
- The IPC commands are registered through `AppManifest::commands` in `build.rs`. Each command then gets an `allow-<name>` permission, and any command a capability does not grant is rejected. `quick-search` may call only `search`, `open_source`, `copy_citation`, and `hide_window`; `main` may call only `list_folders`, `get_settings`, `list_models`, and `export_pack` (`set_air_gap` per ADR-004; `get_license_status` was removed by ADR-011). No `core:*`, fs, shell, or http permissions and no Tauri plugins (ADR-004 adds `global-shortcut`, used from Rust only, with no JS permission granted).
- Brand strings are written only in `src-tauri/src/brand.rs`. `build.rs` `include!`s that file, generates `src/brand.ts`, and syncs `productName`, `identifier`, and window titles in `tauri.conf.json`. It skips the write when content is unchanged: tauri-build watches `tauri.conf.json`, so writing every time would rebuild forever. `scripts/check-brand.sh` catches hardcoded strings, and CI runs `git diff --exit-code` to confirm the generated files match what is committed.
- The CSP is `script-src 'self'`, and `connect-src` is only `ipc:` and `http://ipc.localhost`. Only the development `devCsp` adds `ws://localhost:1420` for Vite HMR. No `@vitejs/plugin-react`: it injects an inline refresh script that `script-src 'self'` blocks. So there is no React Fast Refresh, and an edit reloads the whole page.
- Vite 8 transforms with Oxc instead of esbuild. The `esbuild` option is deprecated and its type has no `jsx`, so `vite.config.ts` sets JSX with `oxc: { jsx: { runtime: "automatic" } }`.

### ACL findings

The test in `src-tauri/src/lib.rs` wires `tauri::test::mock_builder()` to the real `builder()` and `generate_context!(test = true)`, so the real capability files are used. `test = true` only drops the macOS Info.plist symbol (`_EMBED_INFO_PLIST`). Without it, the symbol collides with the one from `generate_context!()` in `run()` and the test build fails.

- Calling `get_settings` from `quick-search` is rejected by the ACL with `get_settings not allowed on window "quick-search", webview "quick-search", URL: local ... referenced by: capability: main-window, permission: allow-get-settings`. It is not an AppError.
- `search` from `quick-search` passes the ACL and returns `not_implemented`. `hide_window` returns Ok.
- From `main`, `get_settings` passes (`not_implemented`) and `search` is rejected.
- The test request URL is `tauri://localhost`. Tauri treats a request as `Origin::Local` only when its URL is the app's local origin; any other URL counts as remote. Capabilities allow only local origins by default.
- In the Tauri 2.12 source (`webview/mod.rs`, `on_message`), app commands from local requests are checked against the ACL only when an app ACL manifest exists. Without `AppManifest::commands`, a local window can call every app command regardless of capabilities. So every new command must also go into `COMMANDS` in `build.rs`. A capability that references an unregistered command fails the build.
- Check: temporarily adding `allow-get-settings` to `quick-search.json` makes the test fail; reverting makes it pass.
- In the mock build, the setup hook does not run and the configured windows are not created, so the test creates the windows itself. No log file is written either.

### Bundle ID warning

The original ID `com.hush.app` ended with `.app`, which Tauri warns conflicts with the `.app` bundle extension on macOS. Resolved by ADR-012, which changes it to `io.github.skyoo2003.locite`.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `tauri` 2.12 | Desktop shell, IPC, windows, ACL. The `test` feature is enabled only in dev-dependencies. | MIT OR Apache-2.0 | Makes no requests itself at runtime. No updater or http plugin. |
| `tauri-build` 2.7 | Config and capability validation, command permission generation | MIT OR Apache-2.0 | None |
| `serde` | IPC serialization | MIT OR Apache-2.0 | None |
| `serde_json` | Test serialization. As a build-dependency, `preserve_order` keeps the key order of `tauri.conf.json`. | MIT OR Apache-2.0 | None |
| `thiserror` 2 | Per-module error types | MIT OR Apache-2.0 | None |
| `tracing` | Logging | MIT | None |
| `tracing-subscriber` (`env-filter`, `fmt`) | Log filter (`RUST_LOG`, default `info`) and formatting | MIT | None |
| `tracing-appender` | Daily rotating files in the app log directory, 7 kept | MIT | None |

### New npm packages

| Package | Reason | License | Network |
| --- | --- | --- | --- |
| `react`, `react-dom` 19 | UI | MIT | None |
| `@tauri-apps/api` 2 | `invoke`, current window label | MIT OR Apache-2.0 | None. IPC only. |
| `@tauri-apps/cli` 2 (dev) | `tauri dev`, `tauri build`, icon generation | MIT OR Apache-2.0 | Dev tool; not shipped in the app. |
| `vite` 8 (dev) | Bundler, dev server (port 1420) | MIT | Dev server listens locally only; not shipped in the app. |
| `typescript` 7 (dev) | Type checking (strict) | Apache-2.0 | None |
| `@biomejs/biome` (dev) | Lint | MIT OR Apache-2.0 | None |
| `vitest` (dev) | Frontend tests | MIT | None |
| `@types/react`, `@types/react-dom` (dev) | Types | MIT | None |

## ADR-003. Storage, keychain, Air-Gap guard, and scoped paths

- Date: 2026-11-22
- Status: Accepted

### Decision

- The DB key is 32 bytes from `OsRng`, stored in the login keychain as service `KEYCHAIN_SERVICE`, account `db-key-v1`, and given to SQLCipher as a raw hex key (`PRAGMA key = "x'..'"`), which skips PBKDF2. `open_or_create` writes the key before creating the DB. A stored key that does not open the DB returns `WrongKey`. A missing key next to an existing DB returns `KeyMissing`. Neither case makes a new key.
- Each connection sets `cipher_memory_security = ON`, `foreign_keys = ON`, and `temp_store = MEMORY`. The writer sets WAL. Each of the 4 readers sets `query_only`. Migrations are SQL files indexed by `user_version`, one transaction each. A DB newer than the app is rejected.
- FTS5 (external content) and `vec0` get no FK cascade. Triggers on `chunks` keep both in sync on insert, update, and delete, including rows removed by the `documents -> chunks` cascade. So `delete_document` is one `DELETE FROM documents` statement, and every future chunk delete path is covered too. `doc_versions` also cascades.
- Air-Gap defaults to ON and persists in `settings.airgap_enabled`. Any value other than `"0"` means ON. OFF allows exactly one host per purpose, matched exactly and case-insensitively. `net::Client` accepts only `https` URLs, follows no redirects, and uses no proxy, because either would reach a host the guard never checked. (Amended by ADR-017: `get` follows redirects itself, each hop checked.)
- `ScopedPath` canonicalizes the root and candidate, then requires the result to stay under the root. This resolves `..`, absolute paths, and symlinks. A symlink cycle fails as `Unresolvable`.

### Allowed hosts (placeholders)

| Purpose | Host |
| --- | --- |
| ModelDownload | `huggingface.co` |
| UpdateCheck | Host of `brand::SITE_URL` (ADR-022: `github.com`, the host of `brand::UPDATE_MANIFEST_URL`) |

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `rusqlite` 0.43 (`bundled-sqlcipher-vendored-openssl`) | SQLCipher, per ADR-001 | MIT | None at runtime |
| `sqlite-vec` 0.1.9 | `vec0`, per ADR-001 | MIT OR Apache-2.0 | None at runtime |
| `zeroize` | `Zeroizing` key buffers (rule 6) | MIT OR Apache-2.0 | None |
| `rand_core` 0.9 (`os_rng`) | `OsRng` for the DB key | MIT OR Apache-2.0 | None |
| `security-framework` 3 (macOS only) | Keychain generic password. Already a transitive dependency of rustls. Chosen over `keyring` 4, whose defaults turn on Windows and D-Bus stores. | MIT OR Apache-2.0 | None |
| `reqwest` 0.13 (`default-features = false`, `rustls`) | The single HTTP client. Pulls `aws-lc-rs` and `rustls-platform-verifier`. No system proxy, no redirects. | MIT OR Apache-2.0 | Only through `net::Client`, behind `AirGapGuard`. |
| `tempfile` (dev) | Test directories | MIT OR Apache-2.0 | None |

## ADR-004. Page cache budget, reader choice, Air-Gap command, global shortcut

- Date: 2026-11-22
- Status: Accepted

### Decision

- **Cache budget.** Each reader connection sets `cache_size = -1048576` (1 GiB) and `soft_heap_limit = 1 GiB`. `cache_size` is a ceiling and SQLite allocates pages only as they are read, so a small DB costs little. `soft_heap_limit` is process-wide: once SQLite's heap nears it, the page cache recycles pages instead of growing, so all connections together stay near 1 GiB. Per ADR-001, 1 GiB holds about 590k float vectors (estimate). The writer keeps the default cache. The budget is a fixed constant; deriving it from physical RAM waits until 8 GB Macs show swapping.
- **Reader choice.** `Db::read` takes the lowest-indexed free reader and waits on a round-robin reader only when all four are busy. A single caller therefore always gets reader 0, so one page cache stays warm instead of four caches each warming the same pages, which under `soft_heap_limit` would evict each other. Tests: sequential reads use the same connection, and a read held open does not block reads on other threads (this test fails on the old round-robin code).
- **`temp_store`.** `PRAGMA temp_store` returns 2 (MEMORY) on every connection, and `compile_options` has no `TEMP_STORE=0`, which would force temp files whatever the pragma says. Both are asserted in `connection_pragmas`.
- **Air-Gap command.** `set_air_gap(enabled: bool)` is a `main`-window command. It calls `AirGapGuard::set_enabled`, which persists before switching. It is not tier-gated, because Air-Gap is a safety feature (rule 10). A test invokes it through IPC and checks both the live guard and the reloaded setting. `quick-search` is denied by the ACL.
- **Global shortcut.** (Superseded by ADR-016.) `CmdOrCtrl+Shift+Space` toggles the `quick-search` window (show and focus, or hide). It is registered in Rust during setup. If another app owns the shortcut, the app logs a warning and starts anyway.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `tauri-plugin-global-shortcut` 2.4 (pulls `global-hotkey` 0.8) | System-wide shortcut for `quick-search` | MIT OR Apache-2.0 | None |

## ADR-005. GPL-3.0-only license and dependency license check

- Date: 2026-11-22
- Status: Superseded in part by ADR-011 (license identifier, paid tiers)

### Decision

The project is licensed GPL-3.0-only. Contributions are accepted under the DCO with no CLA. `cargo deny check licenses` runs in CI against an allowlist in `src-tauri/deny.toml`.

### Reasons

- `-only` rather than `-or-later`, so a future GPL version does not apply without a deliberate decision.
- DCO rather than a CLA keeps contributing simple. The cost is that relicensing needs every contributor's consent.
- The allowlist holds only licenses that are compatible with GPL-3.0: 0BSD, Apache-2.0 (with or without LLVM-exception), BSD-3-Clause, blessing (SQLite), CC0-1.0, CDLA-Permissive-2.0, GPL-3.0-only, ISC, MIT, MIT-0, MPL-2.0, Unicode-3.0, Unlicense, WTFPL, and Zlib. `blessing` and `WTFPL` come from `sqlcipher-src`. Static OpenSSL 3.x is Apache-2.0.
- Paid tiers stay compatible with the GPL. The binaries and the subscription are sold, and the source is published. Anyone can build a copy with the license check removed, so the subscription pays for convenience, signed builds, and updates, not for access.

### Tool

`cargo-deny` (MIT OR Apache-2.0) runs only in CI through `EmbarkStudios/cargo-deny-action@v2` on Linux, because the action needs Docker. It is not a crate dependency and ships nothing. `check licenses` reads local `cargo metadata` and makes no network requests beyond the CI fetch of the crate index.

### Known gaps

- npm packages are not checked automatically. A manual `pnpm licenses list` on 2026-11-22 found only Apache-2.0, BSD-3-Clause, ISC, MIT, and MPL-2.0.
- Source files carry no SPDX headers.

## ADR-006. Document parsers: TXT, Markdown, DOCX

- Date: 2026-11-22
- Status: Accepted

### Decision

- `ingest::parser::ParserRegistry` picks a parser by lower-cased extension, then checks a magic prefix where the format has one (DOCX: `PK\x03\x04`). Unknown extensions return `UnsupportedType`, and a prefix mismatch returns `MagicMismatch`.
- Every parser reads through one capped reader: files over 200 MiB (`MAX_FILE_BYTES`) are rejected from the stat, and the read itself is capped too, so a file that grows after the stat is still rejected. Everything stays in memory, with no temp files.
- TXT decoding order: BOM, then BOM-less UTF-16 by a NUL-byte heuristic, then UTF-8, then CP949 (`EUC_KR` in encoding_rs), then Windows-1252. The UTF-16 check runs before UTF-8 because ASCII-only UTF-16 is also valid UTF-8.
- DOCX parts are read against a total decompressed budget (`DocxParser::max_unzipped_bytes`, default 512 MiB). Declared zip sizes are never trusted. Headings come from `styles.xml` names ("heading N", case-insensitive), so localized style IDs such as `1` work.
- Parse errors carry fixed messages, never paths or document text. Types that hold document text do not implement Debug.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `encoding_rs` 0.8 | BOM sniffing and CP949, UTF-16, and Windows-1252 decoding for TXT and Markdown | (Apache-2.0 OR MIT) AND BSD-3-Clause | None |
| `pulldown-cmark` 0.13 (`default-features = false`) | Markdown events for heading sections. No `html` or `getopts`. | MIT | None |
| `quick-xml` 0.42 (no features) | Streaming XML for DOCX `document.xml`, `styles.xml`, and `core.xml` | MIT | None |
| `zip` 9 (`default-features = false`, `deflate-flate2`) | Reads DOCX containers. No aes, bzip2, zstd, lzma, time, or chrono. Pulls `typed-path`, `constant_time_eq`, and `nix` (fs, macOS). | MIT | None |
| `flate2` 1 (`default-features = false`, `rust_backend`) | Pins zip's deflate to the pure-Rust `miniz_oxide` backend. Already in the tree through `png`. | MIT OR Apache-2.0 | None |

## ADR-007. PDF text layer with pdfium, OCR fallback

- Date: 2026-11-22
- Status: Accepted

### Decision

- `ingest::pdf::PdfParser` reads PDFs through PDFium (`pdfium-render` 0.9.6 over the pinned `bblanchon/pdfium-binaries` release `chromium/8125`). Files go through `read_capped` and are loaded from memory (`load_pdf_from_byte_vec`), never from a path, and nothing is written to disk.
- Each page becomes one `Locator::Page(n)` unit (1-based) with one `Span` per word (a maximal run of non-whitespace chars, the union of the chars' tight bounds). `\r\n` from PDFium becomes `\n`. Pages that yield no text get no unit but always get a `PageInfo`.
- Span conventions: `char_start..char_end` are Unicode scalar (`char`) indices into `TextUnit::text`. `bbox` is `[x0, y0, x1, y1]` normalized 0..1 with a top-left origin; PDF and Vision y coordinates (bottom-left origin) are flipped.
- Scan detection: `needs_ocr` is set when a page has fewer than 30 non-whitespace chars (`MIN_TEXT_CHARS`), or its word boxes cover less than 0.1% of the page (`MIN_TEXT_AREA_RATIO`). A 40-char line of 8 pt text covers about 0.15% of an A4 page, so a normal page with one line is not flagged, and below 0.1% only a footer-sized string is left.
- Table hint: `has_tables` is set when the page's path objects hold at least 3 horizontal and 3 vertical axis-aligned line segments longer than 5% of the page width or height. Rectangles count as their four edges. Form XObjects are not walked.
- OCR fallback: a `needs_ocr` page is rendered in memory (about 2000 px wide, at most 12 MP) and passed to the `OcrEngine`. The OCR text replaces the text layer only when it has more non-whitespace chars. Rendered pixels never leave memory.
- `ingest::ocr::OcrEngine` has two implementations: `VisionOcr` on macOS (Apple Vision `VNRecognizeTextRequest`, accurate level, language correction on, languages `ko-KR` then `en-US`, top candidate only, all on-device) and `NoopOcr` elsewhere. Lines are put into reading order top to bottom, then left to right for lines whose vertical centers overlap. Layout analysis (columns, table cells) is out of scope.
- `ingest::image::ImageParser` handles PNG and JPEG with one registry entry per format, so each has its magic check. Decoding uses explicit formats and `image::Limits` (8192 px per side, 300 MiB of decoder allocations) against decompression bombs. One `Locator::Image` unit with one span per OCR line, or no unit when nothing is recognized.
- PDFium bindings are process-global in `pdfium-render`: a second `bind_to_library` returns `PdfiumLibraryBindingsAlreadyInitialized`. All `PdfParser`s therefore share one `Pdfium` in a `OnceLock`, bound under a mutex on first use. With the `thread_safe` feature `Pdfium` is `Send + Sync` and every PDFium call is serialized behind a mutex, so parallel parsing of PDFs gains nothing.
- Encrypted PDFs fail with `ParseError::Encrypted`. Other PDFium and decoder errors map to fixed-message variants.

### Options compared

Checked against the crate sources fetched with `cargo fetch` on 2026-11-22 (no other network use). "Not verified" marks claims taken from documentation without running code.

| Option | Per-char/word boxes | Renders pages (for OCR) | Paths (grid lines) | Notes |
| --- | --- | --- | --- | --- |
| `pdfium-render` 0.9.6 + PDFium | Yes: `PdfPageTextChar::tight_bounds` | Yes: `render_with_config` -> `image` | Yes: path objects and segments | Chromium's PDF engine. Cost: a 15 MB universal (x86_64 + arm64) dylib per app, and a C++ parser running on untrusted files. |
| `pdf_oxide` 0.3.78 | Yes: `TextChar::bbox`, `extract_words` | Behind the `rendering` feature (tiny-skia, system fonts) | Yes: `extract_paths`, `extract_lines`, `extract_tables` | Pure Rust, MIT OR Apache-2.0, about 450k lines and young (0.3.x). Its `ocr` and `tsa-client` features pull `ureq` and download OCR models, so they must stay off. Quality on real files not verified. |
| `pdf-extract` 0.12 | Partly: `OutputDev::output_character` gives a transform and advance width, not glyph bounds | No | No | MIT, built on lopdf. About 100 `unwrap`/`expect`/`panic!` sites in `lib.rs`. |
| `lopdf` 0.47 | No: `extract_text` returns a `String` | No | Content stream operators only | MIT, `forbid(unsafe_code)`. A low-level object model. |
| `pdf` (pdf-rs) 0.10 | No text extraction API; ToUnicode lives in `font.rs` | No (a separate `pdf_render` crate, not checked) | Content operators only | MIT. |
| `hayro` 0.9 | No text API; glyphs expose `as_unicode` via `hayro-interpret` | Yes, CPU rasterizer | Through its device trait | Apache-2.0 OR MIT, `forbid(unsafe_code)`. README: no encrypted PDFs and several unsupported features. |

PDFium wins because one engine gives char bounds, rendering, and path objects, and it handles the widest range of real PDFs (not measured here). The pure-Rust stacks would need two or three crates (`pdf_oxide` for text plus `hayro` for rendering) and more of our own code. Revisit `pdf_oxide` plus `hayro` if the native dylib or its attack surface becomes the bigger cost.

### PDFium binary

- `scripts/fetch-pdfium.sh` downloads `pdfium-mac-univ.tgz` from release `chromium/8125` (SHA-256 `0cf06803b9a4acc489fc967a1b05990622ca1cf41fa027ff9cc89eeab9b19279`) into `src-tauri/vendor/pdfium/` (git-ignored). It is developer and CI tooling, like `verification/airgap/run.sh`; the app never downloads it.
- `tauri.conf.json` bundles `lib/libpdfium.dylib` as a macOS framework and its license files as resources. tauri-build copies both at build time, so `cargo build` fails until the script has run.
- Licenses in the archive: the package itself is MIT (Benoit Blanchon); PDFium is BSD-3-Clause plus Apache-2.0; bundled third-party code is abseil, highway (Apache-2.0), AGG 2.3 (permissive notice), dragonbox (Apache-2.0 with LLVM exception, or Boost 1.0), fast_float and simdutf (MIT), FreeType (FreeType License), HarfBuzz (Old MIT), ICU (Unicode-3.0), Little CMS (MIT), libjpeg-turbo (IJG plus BSD-3-Clause), OpenJPEG (BSD-2-Clause), libpng (PNG Reference Library License v2), LLVM libc (Apache-2.0 with LLVM exception), and zlib (Zlib). All are compatible with GPL-3.0-only. The FreeType License asks for credit in the documentation, which the bundled license files provide.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `pdfium-render` 0.9.6 (`default-features = false`, `pdfium_8125`, `thread_safe`, `image_025`) | PDFium bindings: text chars and bounds, path objects, rendering, metadata | MIT OR Apache-2.0 | None. Its `log` calls are WASM-only. Pulls `libloading` (ISC), `chrono`, `itertools`, `utf16string`, `vecmath`, `maybe-owned`. |
| `image` 0.25 (`default-features = false`, `png`, `jpeg`) | PNG/JPEG decoding with limits; the RGBA buffer passed to OCR | MIT OR Apache-2.0 | None. No rayon, no other formats. Pulls `zune-jpeg` (MIT OR Apache-2.0 OR Zlib), `moxcms` and `pxfm` (BSD-3-Clause OR Apache-2.0). |
| `objc2` 0.6, `objc2-foundation` 0.3, `objc2-core-foundation` 0.3, `objc2-core-graphics` 0.3 (macOS only, minimal features) | Objective-C runtime, `NSString`/`NSArray`, `CFData`, and `CGImage` for Vision | `objc2`, `objc2-foundation`: MIT. `objc2-core-foundation`, `objc2-core-graphics`: Zlib OR Apache-2.0 OR MIT | None. Already in the tree through Tauri. |
| `objc2-vision` 0.3 (macOS only, `VNRecognizeTextRequest`, `VNRequestHandler`, `VNObservation`) | Apple Vision text recognition, on-device | Zlib OR Apache-2.0 OR MIT | None. Vision's recognition runs locally. |

`cargo deny check licenses` passes without widening the allowlist.

## ADR-008. Chunker, change detection, and the per-file pipeline

- Date: 2026-11-22
- Status: Accepted

### Decision

- `ingest::chunker::chunk` turns a `ParsedDocument` into chunks with `char` offsets into `doc_text` (units joined with `\n`). A chunk never spans a block: one page, one heading section, one run of `Line` units, or one image. Defaults: target 400, max 512, overlap 60 tokens.
- Within a block, text is cut into atoms by paragraph, then sentence, then word (UAX #29 via `unicode-segmentation`), then raw chars. A piece is cut further only while it is over `max - ceil(1.2 x overlap)` tokens, so overlap plus one atom always fits. Atoms are packed greedily to the target.
- Overlap starts at the latest sentence start (else word start, else char) whose suffix counts 0.8 to 1.2 x overlap. Chunks under 2 x overlap do not overlap.
- Tokens are counted as `ceil(chars / 4)` (`CharApprox`) until the embedding model's tokenizer lands; this undercounts Hangul and CJK. Superseded by ADR-013: the embedding model's tokenizer counts tokens.
- `bbox` is one rect per text line of the chunk's spans, merged into at most 8.
- `ingest::hash::decide` skips a file whose size and mtime are unchanged without reading it. A changed stat hashes the file (SHA-256, 64 KiB blocks); the same hash skips and stores the new stat.
- `ingest::pipeline::process_file` is the one ingest file that calls storage. `Db::replace_chunks` replaces the chunks, marks the document indexed, and adds one `doc_versions` row in one transaction. A crash before it leaves `sha256` NULL, so the next run indexes the file again.
- Migration 0002 adds `chunks.sha256`.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `sha2` 0.10 | SHA-256 of files and chunk text. Already in the tree through Tauri. | MIT OR Apache-2.0 | None |
| `unicode-segmentation` 1 | UAX #29 sentence and word boundaries for the chunker. Already in the tree through `keyboard-types`. | MIT OR Apache-2.0 | None |
| `proptest` 1 (dev only, `default-features = false`, `std`) | Property tests over generated documents. Pulls `rand_xorshift` and `unarray` (MIT OR Apache-2.0). No fork or timeout features. | MIT OR Apache-2.0 | None |

`cargo deny check licenses` passes without widening the allowlist.

## ADR-009. Air-Gap ON cancels and drains in-flight requests

- Date: 2026-11-23
- Status: Accepted

### Context

The TLA+ model (`verification/airgap/`) found that `Client::get` checked the mode once, before `send().await`. A request that passed while Air-Gap was OFF could still go out after `set_air_gap(true)` returned Ok. It also found that overlapping `set_enabled` calls could leave memory ON and the DB row `"0"`, so a relaunch came up OFF. Only the sync command kept that second race from happening.

### Decision

- `AirGapGuard` keeps the mode in a `tokio::sync::watch` channel. `Client::get` races `send()` against `turned_on()`, so turning ON drops the request future.
- `AirGapGuard::begin` counts the request in `in_flight` before it checks the mode. `set_enabled(true)` waits on a `Condvar` until the count is 0, then returns. Any request that could have seen OFF has been cancelled or has finished before the user sees ON.
- A `toggle` Mutex is held across the DB write, the store, and the drain, so toggles never overlap, whatever calls them.
- Holding a lock across the whole request was rejected: a model download would block the ON toggle (and the main thread) until it finished.

### Consequences

- `set_enabled(true)` blocks its thread until every in-flight request reaches its next poll. A caller that holds a started `get` future without polling it would block ON forever. Only `Client::get` itself may hold an `InFlight`.
- Reading a response body after `get` returns is not covered. Model downloads must re-check `turned_on` while they stream. (Resolved by ADR-017: `get` returns a `Body` that holds the `InFlight` and races `turned_on` per chunk.)

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `tokio` 1 (`default-features = false`, `sync`, `macros`) | `watch` for the mode, `select!` to cancel a request. Already in the tree through reqwest and Tauri; this only adds `tokio-macros`. | MIT | None |

## ADR-010. IPC types generated with ts-rs

- Date: 2026-11-23
- Status: Accepted

### Decision

- IPC structs and enums derive `ts_rs::TS` under `cfg_attr(test, ...)` with `#[ts(export)]`. `cargo test` writes them to `src/ipc/` (`TS_RS_EXPORT_DIR` in `src-tauri/.cargo/config.toml`). The generated files are committed, and CI fails when `git diff src/ipc` is not empty after `cargo test`.
- ts-rs reads the serde attributes, so `#[serde(rename_all = "camelCase")]` shapes both the JSON and the TS type.
- tauri-specta was not chosen. It also generates typed `invoke` wrappers, but its Tauri v2 line is still a release candidate. The command names stay plain strings in the frontend for now. The Tauri ACL test catches a name that does not exist.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `ts-rs` 12 (dev only) | Generates the TS types for IPC inputs and outputs. It is not in the release binary. | MIT | None |

## ADR-011. Free app funded by sponsors, GPL-3.0-or-later

- Date: 2026-11-23
- Status: Accepted
- Supersedes: the `-only` choice and the paid-tier reasoning in ADR-005, and the License host in ADR-003

### Decision

- Locite is free. Every feature works the same for every user. There are no tiers, feature locks, usage limits, upgrade prompts, payment code, or license checks. Maintenance is paid for by sponsors (GitHub Sponsors monthly, Buy Me a Coffee one-time). Sponsors get only symbolic thanks.
- Removed: `entitlement.rs`, the `get_license_status` command and its permission, `ErrorCode::UpgradeRequired`, and `NetPurpose::License`. The app now has two network purposes, `ModelDownload` and `UpdateCheck`, and no telemetry.
- The SPDX identifier is `GPL-3.0-or-later`, the usual choice, which leaves room to use a later GPL version. Every commit so far has one author, so the change needs no other consent. `deny.toml` adds `GPL-3.0-or-later` to the allowlist and keeps `GPL-3.0-only`, which is compatible for dependencies.
- Distribution is GitHub Releases and Homebrew only, because the Mac App Store terms conflict with the GPL. Each release uploads the source archive of its tag, and the About screen shows the license notice and a source link. `docs/building.md` explains self-builds.
- The name and icon are protected separately by `TRADEMARK.md`: forks must rename.

### Consequences

- Absolute rules 2, 6, 8, and 10 in `CLAUDE.md` change. Rule 8 now forbids feature gates instead of routing them through an entitlement manager, and rule 10 says the public repository is the complete source of official builds.
- Without a CLA, relicensing later needs every contributor's consent. Decide before the repository goes public (`docs/roadmap.md`).

## ADR-012. Rename Hush to Locite

- Date: 2026-11-23
- Status: Accepted

### Context

"Hush" is already the name of an MIT-licensed privacy app for macOS and iOS ([oblador/hush](https://github.com/oblador/hush), "Hush Nag Blocker" on the App Store). A fork rule in `TRADEMARK.md` means little if the name itself collides.

### Decision

- The name is Locite (local + cite). `brand.rs` now holds `APP_NAME = "Locite"`, `APP_SLUG = "locite"`, and `PACK_EXT = "locitepack"`. The crate is `locite` (lib `locite_lib`), the npm package is `locite`, the dev signing variable is `LOCITE_DEV_SIGN_IDENTITY`, and the skill is `verify-locite`.
- `BUNDLE_ID` and `KEYCHAIN_SERVICE` become `io.github.skyoo2003.locite`, a reverse-DNS ID under a domain the maintainer controls. This also removes the `.app` suffix warning from ADR-002.
- `SITE_URL` becomes `https://skyoo2003.github.io/locite`, so the UpdateCheck host is `skyoo2003.github.io`. The old host `hush.app` was not a domain the project owned. With Air-Gap OFF, the app would have contacted whoever owned it.
- The GitHub repository is renamed to `skyoo2003/locite`. GitHub redirects the old URLs.

### Consequences

- Existing dev installs start fresh. The old DB in `~/Library/Application Support/com.hush.app/`, the logs, and the keychain item `com.hush.app` are left in place, unused.
- Anyone with `HUSH_DEV_SIGN_IDENTITY` set must rename it to `LOCITE_DEV_SIGN_IDENTITY`.

## ADR-013. Embedding model and runtime: bge-small-en-v1.5 on candle

- Date: 2026-11-23
- Status: Accepted
- Evidence: `spikes/embed_bench/` (throwaway spike, not product code). The task asked for this record as ADR-003, but that number is taken (storage, keychain), so it is ADR-013.

### Decision

- The default model is `bge-small-en-v1.5` (`rag::embedder::MODELS[0]`): 384 dimensions, CLS pooling, query prefix `Represent this sentence for searching relevant passages: `, no passage prefix. `multilingual-e5-small` is also registered (mean pooling, `query: ` and `passage: ` prefixes) but is not the default.
- The runtime is candle 0.11.1 on the CPU, with Apple Accelerate BLAS on macOS. It is pure Rust and downloads nothing at build time or at runtime.
- `trait Embedder: Send + Sync { dim, model_id, embed_passages, embed_query }`. Vectors are L2-normalized. Batches hold 32 texts, padded to the longest text, truncated at 512 tokens.
- `BertEmbedder::load(models_dir, id)` reads `models_dir/{id}/` through `ScopedPath`, checks the sha256 of `config.json`, `tokenizer.json`, and `model.safetensors` against `manifest.json`, then builds the model from the bytes it hashed. A file that was swapped after the check is never loaded. A file missing from the manifest counts as a mismatch.
- `BertEmbedder` implements the chunker's `TokenCounter`. It counts the passage prefix, the text, and the special tokens, so a chunk under `max_tokens` (512) is never truncated. `CharApprox` is now test-only.
- `FakeEmbedder` (384 dimensions, word hashing, deterministic) stands in for the model in CI. Real-model tests are `#[ignore]`. Run `scripts/fetch-models.sh` first, then `cargo test --release -- --ignored`.
- The dimension is 384, which matches `vec_chunks float[384]` from migration 0001, so no migration is needed.

### Comparison

Measured with `cargo run --release --manifest-path spikes/embed_bench/Cargo.toml -- models` on an Apple M4 (10 cores, 16 GB). **No M1 was available, so M1 times are not measured.** Each run embeds 1,000 synthetic English chunks of about 1,600 chars, the chunker's 400-token target, in batches of 32. There is one warm-up batch, then 3 runs.

| Model | License | Dim | `model.safetensors` | Tokens per chunk | 1,000 chunks, CPU + Accelerate (3 runs) | CPU, no Accelerate | Retrieval check R@1 / MRR | Termination: related / unrelated cosine |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| bge-small-en-v1.5 | MIT | 384 | 133 MB | 293 | 48.5 / 51.2 / 51.8 s | 100.6 / 107.8 / 118.0 s | 20/24, 0.892 | 0.600 / 0.360 |
| multilingual-e5-small | MIT | 384 | 471 MB (+17 MB tokenizer) | 328 | 61.5 / 60.9 / 61.9 s | not run | 20/24, 0.900 | 0.834 / 0.728 |
| nomic-embed-text-v1.5 | Apache-2.0 | 768 | 547 MB | 297 | 119.3 / 145.1 / 167.7 s | not run | 21/24, 0.925 | 0.682 / 0.416 |

- All three licenses allow redistribution.
- The retrieval check is 24 hand-written English query and passage pairs whose passages share vocabulary. Each query ranks all 24 passages. It is too small to separate the models: one query is the whole gap. It shows that each model runs correctly with its prefixes and pooling, but it is not a quality benchmark. Building that is the week-3 eval.
- bge wins on speed, size, and English focus, and ties e5 on the check. e5's 250k-token vocabulary makes the file 3.5x larger for no English gain. nomic needs 768 dimensions, so `vec_chunks` would double in size and KNN time (ADR-001). It is also 2.5x slower, and its run-to-run spread (119 to 168 s) is larger than any quality gap measured here.
- **What limits CPU time.** A 10 s `sample` of the bge run puts most threads in idle waits, about 2.5 of 10 cores busy. The busiest frames are scalar `libm erff`, the exact GELU, at about 3x the BLAS samples. Then come the elementwise divide and subtract, softmax reductions, and layer norm. The bound is candle's CPU elementwise kernels, not matrix multiplication.
- **Metal.** candle 0.11.1 fails on Metal with `no metal implementation for layer-norm` for this BERT, so it was not measured.

### Rejected

- **ort (ONNX Runtime, optional CoreML EP).** It would probably remove the GELU and threading bound, but it was not measured. `ort` 2.0 is still a release candidate built for ONNX Runtime 1.31. Its default `download-binaries` fetches prebuilt C++ at build time, which conflicts with rule 10 and the no-auto-download rule. The alternative is shipping a separately built `libonnxruntime.dylib`. Revisit this if indexing time becomes a complaint, starting with a spike that links a pinned ONNX Runtime built from source.
- **fastembed-rs.** It wraps ort, so it has the same binary problem, and it downloads models from the network by default.

### Consequences

- About 50 s per 1,000 chunks on an M4 means about 85 minutes for a 100k-chunk library, run in the background. M1 is expected to be slower (not measured).
- The weights are not in git. `scripts/fetch-models.sh` downloads files pinned to a commit and checks them against the tracked manifests. Shipping models to users (ModelDownload or the offline kit) is separate work.
- The tracked manifest is the trust anchor. Anyone who can replace both a model file and its manifest in the models folder can load other weights.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `candle-core`, `candle-nn`, `candle-transformers` 0.11.1 (`accelerate` on macOS) | BERT inference from safetensors. Pulls `gemm`, `half`, `safetensors`, `accelerate-src` (links the system Accelerate framework). | MIT OR Apache-2.0 | None |
| `tokenizers` 0.23.2 (`default-features = false`, `fancy-regex`) | The model's tokenizer, for embedding and chunk token counts. No `http`/`hf-hub`, no `onig`, no progress bar. | Apache-2.0 | None |

`cargo deny check licenses` passes without widening the allowlist.

## ADR-014. Folder scanner and index queue

- Date: 2026-11-23
- Status: Accepted

### Decision

- `ingest::scanner::scan` walks a folder with `ignore::WalkBuilder`. It does not follow symlinks, skips hidden entries, stops at depth 32, and turns off the standard filters, so a `.gitignore` in a documents folder hides nothing. Excludes are gitignore lines: `scanner::DEFAULT_EXCLUDES` plus the `exclude_patterns` setting (one line per pattern; invalid lines are skipped). The walk is compared with the DB by stat into new, reindex, and delete lists. Deletes are listed only after a walk with no read errors.
- `ingest::queue::IndexQueue` runs `cores / 2` parse threads (stat, hash, parse, chunk) and one thread that embeds and commits. They are joined by a `sync_channel` holding one parsed document per parse thread, so parsing blocks when embedding falls behind. Jobs are keyed by `(folder_id, rel_path)`. A job for a queued path merges into it, and a job for a running path runs after it.
- `pipeline::commit` re-stats the file before its one-transaction write. A changed stat writes nothing and requeues; a vanished file deletes the document. This replaces the old shortcut where an edit between hashing and parsing could store content newer than the stored hash.
- Cancel is a `tokio_util::sync::CancellationToken`, so the future async scheduler can own the same token. The embed thread checks it after embedding and before the commit.
- `index://progress` carries counts only. A dedicated thread emits it at most every 250 ms, and only when something changed.
- The managed `Db` is now `Arc<Db>`, so the queue's threads can share it with the IPC commands.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `ignore` 0.4 (pulls `globset`, `bstr`, `walkdir`, `same-file`) | Directory walk with gitignore-syntax excludes, depth limit, and no symlink following. | Unlicense OR MIT | None |
| `tokio-util` 0.7 (`default-features = false`) | `CancellationToken`, usable from both threads and async code. Already in the tree through `reqwest`. | MIT | None |

`cargo deny check licenses` passes without widening the allowlist.

## ADR-015. Folder watcher, folder availability, dialog-only add, and model idle unload

- Date: 2026-11-23
- Status: Accepted

### Decision

- `ingest::watcher::FolderWatcher` runs one actor thread. It owns a `notify-debouncer-full` debouncer (1.5 s, FSEvents on macOS) and the folder map. The debouncer's handler and the IPC side only send it messages on one `std::sync::mpsc` channel; the thread blocks on `recv`. At start it brings every registered folder in line with the disk and rescans the present ones, which replaces the startup rescan loop in `lib.rs`.
- Each event path is handled by what is on disk now, not by the event kind: an admitted file gets a reindex job (the pipeline skips an unchanged hash), a directory rescans the folder (a moved-in directory brings no per-file events), and a missing path deletes every document at or under it. A rename reports both paths, so it falls out of the same rules. FSEvents keeps a recent file's "created" flag, and the debouncer reports a rename of a just-created file as a create at the new path only. In tests this hit a file created seconds before its rename in 7 of 8 runs. So each batch also deletes the documents whose file is gone from any directory that holds a touched path. An event with the rescan flag rescans the folders it touches, or every active folder if it has no paths. The watcher filters single paths with `scanner::Filter`, the same hidden, exclude, and extension rules as the walk.
- Availability. A root that is missing (moved away, or its drive unplugged) marks the folder `unavailable` and keeps every document. A recursive watch on a volume's folder gets no event when the volume unmounts, so the watcher also watches `/Volumes` non-recursively: an unmount reports `/Volumes/<name>` removed and a remount reports it created. An event on a root or an ancestor of it reconciles the folder; a root that is back gets a fresh watch and a rescan. As a second guard, the queue's delete step does nothing while a job's root is missing.
- Dialog-only add. `add_watched_folder` takes no path: Rust opens the native folder picker with `tauri-plugin-dialog`. No window is granted a dialog permission, so the webview cannot open a picker or pass a path itself. A folder that equals, contains, or is inside a registered folder (by path component) is rejected with `folder_overlap`. `/`, a top-level directory, the home folder, or an ancestor of it is added with the `broad_path` warning.
- Idle workers block. Parse workers wait on the queue's condvar with no timeout when nothing waits for a retry. An outside cancel of the `CancellationToken` still wakes them: the queue polls `cancelled_owned()` once with a `Wake` that notifies the condvar. The progress thread blocks until something changed.
- `BertEmbedder` keeps the config and tokenizer loaded and holds the model in an idle slot. The model is dropped after 10 minutes without an embed (`MODEL_IDLE`), and the next embed reloads `model.safetensors` through `ScopedPath` and checks it against the sha256 from load time. One thread per slot waits on a condvar for the deadline.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `notify-debouncer-full` 0.8 (pulls `notify` 9, `notify-types`, `file-id`, `xxhash-rust`) | File system events with debouncing and rename pairing; FSEvents on macOS. | MIT OR Apache-2.0 (`notify` CC0-1.0, `xxhash-rust` BSL-1.0) | None |
| `tauri-plugin-dialog` 2 (pulls `rfd`, `tauri-plugin-fs`) | Native folder picker opened from Rust. The fs plugin is a library dependency only; it is not registered, so its commands do not exist. | Apache-2.0 OR MIT (`rfd` MIT) | None |

`xxhash-rust` (used by `notify`'s poll watcher) is BSL-1.0, which was not on the `cargo deny` allowlist. The Boost Software License 1.0 is permissive and the FSF lists it as GPL-compatible, so `BSL-1.0` is added to `src-tauri/deny.toml`. `cargo deny check licenses` passes.

### Consequences

- A recently created file moved to another directory can report only the new path, so the old path's document stays until the next rescan (next launch, remount, or rescan event). A rename within one directory is covered by the sweep above.
- A folder that is unavailable at launch and is not under `/Volumes` is only noticed again at the next launch, or when an event touches its root or an ancestor that is watched.
- A folder added before the embedding model loads has no watcher yet; it is scanned at the next launch.
- A job already running when a folder is removed may fail against the deleted folder (foreign keys reject it) and counts as failed. Nothing is written for the removed folder.
- `pipeline::commit` deletes a document whose file vanished between parse and commit only while the folder root still exists. A missing root returns `Commit::Unavailable` and writes nothing, so the document in flight on an unplugged drive is kept.

## ADR-016. Quick-search window, tray, and Dock policy

- Date: 2026-11-24
- Status: Accepted

### Decision

- **Shortcut.** Default `Cmd+Shift+Space`, saved in the `quick_search_shortcut` setting and changed from the main window (`set_shortcut`, main window only). A failed swap keeps the old shortcut registered and saved. Alt+Space was the first choice and was dropped: Alfred and Raycast use it, macOS lets two apps register the same Carbon hotkey, so registration succeeds while the other app takes focus and the popup hides on blur (seen with Alfred on the maintainer's machine). Registration failure is the only conflict the app can detect; `get_settings.shortcutRegistered` is false then, the main window opens at launch, and it shows a notice.
- **Window.** `quick-search` (680x460) is created hidden at startup and only shown and hidden. Each show moves it to the monitor under the cursor, centered, top edge a quarter of the way down, in logical points (on macOS the cursor is scaled by the primary monitor and each monitor by its own factor). It hides on Esc and on focus loss. Hiding also hides the app when the main window is closed, which returns focus to the previous app; showing unhides it first. Measured show latency: 22 to 56 ms over 10 presses on an M4 (budget 150 ms).
- **Dock.** `ActivationPolicy::Accessory` at launch; `Regular` while the main window is open. Closing the main window hides it. The main window opens at launch only when no folder is registered or the shortcut did not register, and on a reopen of the app. No API used needs the Accessibility permission (Carbon hotkeys, `NSEvent.mouseLocation`).
- **`search://focus`.** `true` when the window shows and on every `search` call (that is, while the user types); `false` when it hides. Emitted from Rust, so no window needs `core:event:allow-emit`. Until `ingest/scheduler.rs` exists, `start_indexing` listens and limits parsing to one worker while it is `true`.
- **Tray.** Search, Indexing n/m (finished of total, "Up to date" when idle), Pause/Resume Indexing, Air-Gap status, Sponsor (one item per `SPONSOR_URLS` entry, opened in the browser through `/usr/bin/open`), Settings, Quit. Labels update from `index://progress` and `set_air_gap`.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `tray-icon` 0.25 (tauri `tray-icon` feature; already in Cargo.lock) | Menu bar icon and menu | MIT OR Apache-2.0 | None |

### Constraints

- A shortcut another app also registers (Alfred, Raycast) is not detected: it registers, and the other app wins focus.
- The tray menu was not driven in the running app (verification stopped before it).

## ADR-017. Model manager, built-in catalog, and Hugging Face CDN redirects

- Date: 2026-11-24
- Status: Accepted (changes absolute rule 2; needs the GOVERNANCE.md issue and comment period before merge)
- Amends: ADR-003 (no redirects), ADR-009 (body reads were not covered)

### Context

Users need the embedding model on their machine. It is too large for the app bundle, and a fully offline machine cannot download anything. Hugging Face answers `https://huggingface.co/{repo}/resolve/{commit}/{file}` for a weights file with `302` to a CDN host (`us.aws.cdn.hf.co` for bge, e5, and the GGUF candidates, checked with `curl -I` on 2026-11-24; the `us.` prefix suggests regional hosts). Small files answer `307` to a path on `huggingface.co`. `net::Client` followed no redirects and the guard allowed exactly one host, so no weights file could be downloaded.

### Decision

- **Catalog.** `models/catalog.json` is built into the binary (`include_str!`). Each model has `id`, `kind` (`embedding`, `reranker`, `llm`), `license`, `redistributable`, and `files` of `{ name, url, size, sha256 }`. A list of files replaces the single `url`/`size`/`sha256` the task named, because the embedder needs three files per model. The catalog replaces `models/*/manifest.json` as the only place hashes are stored. `BertEmbedder::load` and `scripts/fetch-models.sh` read it. There is no remote catalog. URLs pin a repo commit.
- **Redirects.** `Client::get` follows up to 3 redirects itself. Every hop passes `AirGapGuard::begin` with a `Hop`. A first hop must reach `NetPurpose::host()` exactly. A ModelDownload redirect hop may also reach a host ending in `.hf.co`. UpdateCheck gets no extra host (amended by ADR-022). Each file's sha256 is pinned, so a CDN host can withhold a file but cannot change it. A suffix was chosen over an exact host list, because a regional host list would break whenever HF adds a region.
- **Body in flight.** `get` returns a `Body` that holds the request's `InFlight` until the body is read or dropped. `Body::chunk` races `turned_on`. Turning Air-Gap ON now stops a download mid-file, and `set_enabled(true)` returns only after the body is dropped. The TLA+ model gains `BodyDone` and the `NoBodyAfterOnAck` invariant.
- **Install.** `ModelManager` writes `{name}.part`, hashes as it goes (the kept prefix is re-hashed on resume), checks the size and sha256, `sync_all`s, and renames into `app_data_dir/models/{id}/{name}`. A file at its final name is therefore verified, and `is_installed` checks names and sizes only. A wrong hash deletes the part. A dropped connection or a cancel keeps it, and the next download sends `Range: bytes=N-`. A `200` reply to a Range request restarts the file. A `206` whose `Content-Range` does not start at N is an error.
- **Import.** For an offline machine, the user picks a file in the native picker, which opens from Rust, so no path comes from the webview. The file is read through `ScopedPath` with its own folder as the scope, so a picked symlink cannot pull a file from elsewhere. That is the exception to rule 3's "registered folders", needed because model files live outside them. A file whose size matches no catalog file is rejected before it is read. Otherwise it is copied to `models/.import-N.part` while hashed, and renamed to the catalog file with that hash. The bytes are never parsed before the hash matches.
- **Air-Gap.** Downloads are refused while Air-Gap is ON (`air_gap_enabled`). On first run without an embedding model, the panel offers "Turn off Air-Gap for this download" or "Import a file…". The first sends `download_model { airGapOffForDownload: true }`. Rust turns Air-Gap OFF, downloads, and turns it back ON however the download ends. Nothing turns Air-Gap off except a click.
- **Loading.** At startup and after each install, `load_model` loads the first installed model in `embedder::MODELS` order. `ModelManager::claim_embedder_load` makes sure only one call starts the queue.
- **IPC.** `list_models`, `download_model`, `cancel_model_download`, and `import_model_file` are main window only. Progress goes to `models://progress` at most every 200 ms. New error codes: `air_gap_enabled`, `hash_mismatch`, `not_in_catalog`, `download_cancelled`, `download_failed`.

### Consequences

- If the app quits during a download that turned Air-Gap OFF temporarily, it relaunches with Air-Gap OFF. The user still sees that state in the panel and the tray.
- `.import-N.part` files left by a crash are not cleaned up.
- `rag/models.rs` does file I/O, which the module boundary in CLAUDE.md reserves for other modules. The task placed it there. It reads only `app_data_dir/models` and the picked file.
- No new crates.

## ADR-018. Local LLM candidate: Phi-4-mini-instruct Q4_K_M

- Date: 2026-11-24
- Status: Accepted for the synthesizer work. The task asked for ADR-004, but that number is taken (page cache, Air-Gap command), so it is ADR-018.

### Context

Cited Q&A needs a local LLM of about 3B parameters, quantized to GGUF Q4_K_M, that runs on an 8 GB Apple Silicon Mac. Its license must let the project redistribute the weights in the offline kit without adding use restrictions on top of the GPL.

### Method

`llama-bench` from Homebrew llama.cpp 1.0.0, Metal with all layers on the GPU, 4 threads. Prompt processing is 512 tokens (`pp512`), generation is 128 tokens (`tg128`), each repeated 3 times at context depth 0 and 2 times at depth 4096 (`-d 4096`), which is a typical RAG prompt. Each file was pinned to a repo commit and matched against Hugging Face's `x-linked-etag` sha256. Peak RSS comes from `/usr/bin/time -l` and includes the memory-mapped weights. Run on an **M4, 16 GB** (no 8 GB Mac was available). The script is not kept in the repo (throwaway). The commands are the ones above.

### Results (measured, M4 16 GB, tokens per second)

| Model (Q4_K_M) | Params | File | License | Korean on card | pp512 d0 | tg128 d0 | pp512 d4096 | tg128 d4096 | Peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SmolLM3-3B | 3.08B | 1.92 GB | Apache-2.0 | No | 414 | 47.8 | 316 | 40.6 | 2.12 GB |
| Qwen2.5-3B-Instruct | 3.40B | 2.10 GB | Qwen Research (non-commercial) | n/a | 412 | 46.2 | 287 | 38.8 | 2.28 GB |
| Llama-3.2-3B-Instruct | 3.21B | 2.02 GB | Llama 3.2 Community (AUP, attribution) | No | 412 | 45.6 | 292 | 36.3 | 2.26 GB |
| Gemma-3-4B-it | 3.88B | 2.49 GB | Gemma Terms (prohibited-use policy) | Not listed | 361 | 36.6 | 329 | 33.1 | 2.71 GB |
| **Phi-4-mini-instruct** | 3.84B | 2.49 GB | **MIT** | **Yes** | 354 | 35.8 | 245 | 29.1 | 2.73 GB |
| Qwen3-4B-Instruct-2507 | 4.02B | 2.50 GB | Apache-2.0 | Not listed | 315 | 36.2 | 164 | 23.1 | 2.73 GB |

Run-to-run spread was below 1% for every number (stddev at most 2 t/s on pp, 0.3 t/s on tg).

### Estimates for an 8 GB Mac (not measured)

- Generation is bound by memory bandwidth, so scaling the depth-4096 `tg128` by bandwidth (M1 68 GB/s, M2/M3 100 GB/s, M4 120 GB/s) gives Phi-4-mini about 16 to 24 t/s.
- Prompt processing is bound by GPU compute. A 4k-token prompt takes about 14 s on the M4 for Phi-4-mini (4096 / the mean of 354 and 245). Expect roughly 20 to 25 s on an M1.
- Memory: about 2.7 GB of weights plus the app (embedder about 0.2 GB, SQLite cache up to 1 GiB per ADR-004) fits in 8 GB. It will push other apps into compression or swap.

### Decision

- **Phi-4-mini-instruct Q4_K_M** is the default LLM. It has the only license here that is both permissive (MIT) and lists Korean on its card, and the UI and the "not found" answer are Korean. It is the third slowest (29 t/s at depth 4096 on the M4).
- Excluded on license: Qwen2.5-3B (non-commercial), and Llama 3.2 and Gemma 3. Llama 3.2 and Gemma 3 can be redistributed, but they add use policies and attribution terms that users would have to accept on top of the GPL.
- Not chosen: SmolLM3-3B is the fastest and Apache-2.0, but its card lists no Korean. Qwen3-4B-2507 is Apache-2.0, but its prompt processing at depth 4096 is the slowest (164 t/s), which would roughly double time to first token on an 8 GB Mac.
- Pin, for the catalog entry once the synthesizer can load it: `https://huggingface.co/bartowski/microsoft_Phi-4-mini-instruct-GGUF/resolve/7ff82c2aaa4dde30121698a973765f39be5288c0/microsoft_Phi-4-mini-instruct-Q4_K_M.gguf`, size 2491874688, sha256 `01999f17c39cc3074afae5e9c539bc82d45f2dd7faa3917c66cbef76fce8c0c2`. It is left out of `models/catalog.json` until then, so the app does not offer a 2.5 GB download that nothing uses.

### Not measured

- Answer quality and how well each model follows citations. That needs the synthesizer and an eval in the style of `eval/`. Rerun this table against that eval before shipping.
- A real 8 GB machine, and a llama.cpp binding inside the app (its crate and license go in a new ADR).

## ADR-019. Index scheduler

- Date: 2026-11-25
- Status: Accepted

### Decision

- `ingest::scheduler` turns signals into one `Reason`, and the reason into a `Plan { parse_workers, embedding }` that `scheduler::apply` runs on the `IndexQueue`. `decide` and `plan` are pure functions with table tests. A `Reason` maps to exactly one state, so a state and a reason that disagree cannot be represented.
- Precedence, first match wins: user Paused, user Run now, outside allowed hours, thermal serious or worse, Low Power Mode, on battery at 20% or less, search open (Interactive), 5 minutes without input (Idle), else Normal. Manual controls beat everything, including allowed hours. Low power beats Interactive, so a search on a low battery never starts work the user asked to stop.
- Plans, with `max` the parse workers spawned (`cores / 2`): Paused and outside hours run nothing (the queue is paused, so progress reads "paused"); Run now and Idle run `max` with embedding; Normal runs `max / 2` (at least 1) with embedding; Interactive runs 1 parse worker with embedding held; LowPower runs nothing or 1 worker with embedding, per the `low_power` setting. Charging at a low charge is not low power.
- Query first. `IndexQueue::set_embedding(false)` holds parsed documents before the embedder. Inside `BertEmbedder`, passage batches wait while any query embeds (`QueryFirst`), so a document that was already embedding when search opened yields between batches of 32.
- Signals: `search://focus` events are sent to the scheduler thread at once; power, idle time, and the local clock are polled every 5 s and on each event. Power is IOKit `IOPSCopyPowerSourcesInfo` (internal battery state and charge) and `NSProcessInfo` (`isLowPowerModeEnabled`, `thermalState`). Idle is `CGEventSourceSecondsSinceLastEventType`. It needs no Accessibility permission: an ad-hoc signed app bundle with `AXIsProcessTrusted() == false` read live values that matched IOKit's `HIDIdleTime`. Allowed hours use `NSCalendar` local time.
- Settings (`mode`, `allowedHours`, `lowPower`) are JSON in the `index_schedule` setting. Invalid saved JSON falls back to the defaults; `set_schedule` rejects an empty or out-of-range time range with `schedule_invalid`. The tray's Pause item now toggles the saved mode, so the panel, the tray, and a relaunch agree.

### New crates

None. New features on existing crates: `objc2-foundation` (`NSCalendar`, `NSDate`, `NSEnumerator`, `NSProcessInfo`) and `objc2-core-graphics` (`CGEventSource`, `CGEventTypes`). IOKit's three power-source functions are declared in `ingest/scheduler/macos.rs`. No network use.

## ADR-020. Sponsor links, the QR code, and the supporter flag

- Date: 2026-11-25
- Status: Accepted

### Decision

- `sponsor::link(channel, air_gap, browser)` is the one place that opens a sponsor link. With Air-Gap off it hands the URL to the external browser (`/usr/bin/open`). With Air-Gap on it opens nothing and returns a QR code of the URL, generated in process as SVG. The IPC command `open_sponsor` and the tray's Sponsor menu both call it; the tray sends the QR to the main window on `sponsor://qr`. The app makes no request for sponsorship in either case.
- `brand::SPONSOR_URLS` holds exactly the two channels of `.github/FUNDING.yml` (GitHub Sponsors, then Buy Me a Coffee). build.rs fails the build if they differ.
- The funding goal shown in Settings > Sponsor is the `**Current goal:**` line of `docs/funding.md`, copied by build.rs into the generated `src/funding.ts`. The app never fetches it.
- The milestone banner shows when `counters.search_count` or the indexed document count reaches 100 and stays until dismissed. Dismissing it, or turning on "I sponsored", sets `sponsor_banner_dismissed`, so it never shows again and the banner never reads the supporter flag.
- `settings.supporter` is self-reported and never verified. Outside tests, only `commands.rs` (store and return), `src/ipc/Settings.ts` (generated type), and `SponsorPanel.tsx` (the switch and badge) name it; `scripts/check-supporter.sh` enforces this in CI. The OLED dark and Terminal themes are selectable by everyone.

### New crates

- `qrcode` 0.15 (MIT OR Apache-2.0), with `default-features = false, features = ["svg"]`, so it pulls in no dependencies (the default `image` feature is off). It encodes a string into an SVG QR code in memory. No network use, no file access.

## ADR-021. Release pipeline

- Date: 2026-11-25
- Status: Accepted

### Decision

- `.github/workflows/release.yml` runs only on a pushed `v*` tag. Every action is pinned by commit SHA, the workflow starts with no permissions, and each job grants only what it needs. Four jobs:
  1. `gate` (Linux, read-only): the tag is `vX.Y.Z`, the tagged commit is on `main`, the version in `tauri.conf.json`, `Cargo.toml`, and `package.json` equals the tag, and CHANGELOG.md has a section for it (`scripts/release-notes.sh`).
  2. `build` (macOS, read-only, no environment, no secrets): toolchain from `rust-toolchain.toml`, `pnpm install`, PDFium, `pnpm tauri build --bundles app`. It also writes the CycloneDX SBOM, `THIRD_PARTY_NOTICES.txt`, the `git archive` source tarball of the tag (checked to contain both lockfiles), and the release notes. No caches are restored, so no other workflow run can feed state into a release.
  3. `sign` (macOS, environment `release`): checks out the tag only for its scripts, imports the Developer ID certificate into a temporary keychain with `security`, signs, notarizes, and staples with `scripts/sign-macos.sh`, then checks the result with `scripts/check-signatures.sh --developer-id` and `spctl` on the DMG. The keychain and the API key file are deleted in an `if: always()` step.
  4. `publish` (Linux, `contents: write`, `id-token: write`, `attestations: write`): `SHA256SUMS` over the DMG, SBOM, notices, and source tarball; a build provenance attestation for every asset including `SHA256SUMS`; a **draft** GitHub release.
- **Secrets never meet build scripts.** `pnpm install` runs package lifecycle scripts and `cargo build` runs every crate's `build.rs` and proc macros: thousands of third-party programs. They run only in `build`, which has no secrets. The signing secrets are scoped to the `release` environment, which only `sign` names, and `sign` runs only this repository's scripts and Apple's tools on the finished bundle. An attacker who controls a dependency can still change the binary, but cannot take the Developer ID certificate or the notarization key, and the change stays in public source and the SBOM.
- **Who can release.** The `release` environment requires the lead maintainer's approval and accepts deployments only from `v*` tags; a tag ruleset lets only the repository admin role create, move, or delete `v*` tags. `scripts/setup-release-environment.sh` applies both. `scripts/check-workflows.sh` (CI) fails if another workflow names a secret other than `GITHUB_TOKEN` or an environment, if any workflow uses `pull_request_target` or `workflow_run`, or if `release.yml` gains another trigger.
- **Two notarizations.** The app is notarized and stapled before it goes into the DMG, and the DMG is notarized and stapled too. A stapled app passes Gatekeeper with no network, which is how Air-Gap users run it, including after they copy it out of the DMG. The stapled DMG passes Gatekeeper when it is opened offline.
- **No entitlements.** The hardened runtime runs with zero entitlements. Library validation accepts `libpdfium.dylib` because the pipeline re-signs it with the same team identity (tested with a development identity: a hardened process loaded the re-signed dylib and refused an ad-hoc one).
- **Draft releases.** A person downloads the draft DMG, runs the Gatekeeper check on a clean Mac, and only then publishes it. Publishing a release is not automated.
- **Self-builds.** `SIGN_IDENTITY=- scripts/sign-macos.sh` ad-hoc signs a local build without the hardened runtime or notarization (docs/building.md).
- **Updater pending.** No updater is included. `tauri-plugin-updater` would make requests outside `net::Client` and `AirGapGuard`, which absolute rule 2 forbids; it waits for its own decision.

### New tools (CI and release only; neither is a crate dependency or ships in the app)

| Tool | Reason | License | Network use |
| --- | --- | --- | --- |
| `cargo-about` 0.9.3 | Rust crate license texts for `THIRD_PARTY_NOTICES.txt` (`src-tauri/about.toml`, `about.hbs`) | MIT OR Apache-2.0 | None: run with `--frozen` (offline, locked), reading license files from the local cargo registry only, never ClearlyDefined. Installed with `cargo install --locked` from crates.io. |
| `cargo-cyclonedx` 0.5.10 | CycloneDX 1.5 JSON SBOM of the aarch64-apple-darwin crate graph | Apache-2.0 | None: reads local `cargo metadata`. Installed with `cargo install --locked` from crates.io. |

## ADR-022. Signed updates through net::Client

- Date: 2026-11-25
- Status: Accepted by the lead maintainer (changes absolute rule 2; needs the GOVERNANCE.md issue and comment period before merge)
- Amends: ADR-003 (UpdateCheck host), ADR-017 (UpdateCheck gets no redirect host)
- Number: ADR-021 is taken by the release pipeline on a parallel branch.

### Context

Users need a way to get new versions, and the release workflow publishes to GitHub Releases. The UpdateCheck host was the project site (`skyoo2003.github.io`), which serves no release files. tauri-plugin-updater can check, verify, and install updates, but `Updater::check`, `Update::download`, and `download_and_install` use the plugin's own `reqwest` client, which would bypass `net::Client` and the guard (rule 2). Measured: `github.com/<o>/<r>/releases/latest/download/X` answers `302` to `github.com/<o>/<r>/releases/download/<tag>/X`, which answers `302` to `release-assets.githubusercontent.com`.

### Decision

- **Hosts.** UpdateCheck starts at `github.com`, the host of `brand::UPDATE_MANIFEST_URL` (`https://github.com/skyoo2003/locite/releases/latest/download/latest.json`). A redirect hop may also reach exactly `release-assets.githubusercontent.com` (`RELEASE_ASSET_HOST` in `security/airgap.rs`); a first hop there is refused. ModelDownload cannot reach it, and UpdateCheck cannot reach `*.hf.co`. An exact host, not a suffix, because GitHub serves release assets from one host.
- **Only net::Client fetches.** `update::check` GETs the manifest through `net::Client::get(NetPurpose::UpdateCheck, ..)` (at most 64 KiB) and passes it to `Updater::restore_update`, which makes no request and forces `require_signed_version`. `update::install` GETs `download_url` the same way (at most 512 MiB; the guard checks every hop) and calls `Update::install(bytes)`, which runs minisign verification against the configured key, then requires the version in the signature's trusted comment to equal the manifest's version, before it touches the app. No `Update` leaves `update.rs`.
- **Enforced.** `scripts/check-net-usage.sh` fails if non-test Rust code calls `.check()`, `download_and_install`, or `.download(` in a file that names the plugin, or if a file other than `lib.rs` and `update.rs` names `tauri_plugin_updater`. No capability grants an `updater:*` permission, so no window can call the plugin's JS commands (ACL test). The plugin is built with `default-features = false` (no system proxy, no zip).
- **Key.** The public key is `plugins.updater.pubkey` in `tauri.conf.json`, committed empty. While it is empty, `check_for_update` fails with `updater_not_configured` before any request. The maintainer runs `pnpm tauri signer generate`, commits the `.pub` file's contents there, and keeps the private key in the release environment's secrets.
- **Manual only.** No check at startup, no timer, no setting. The Security panel has "Check for updates", disabled with the reason while Air-Gap is on, and "Install and restart" once a newer version is found. After a verified install the app calls `AppHandle::restart`.
- **IPC.** `check_for_update -> Option<AvailableUpdate { version }>` and `install_update`, main window only. `install_update` installs only the update the last check found. New error codes: `updater_not_configured`, `update_signature_invalid` (bad, missing, or wrong-key signature, or a signed version that differs from the manifest), `update_failed`. Air-Gap ON returns `air_gap_enabled` with zero requests sent.
- **Release format.** The release uploads `Locite.app.tar.gz` (the Developer ID-signed, notarized, stapled app, archived after signing), its `.sig`, and `latest.json` made by `scripts/make-latest-json.sh <version> <notes-file> <sig-file> <asset-url> <pub-date>` (platform `darwin-aarch64`). The `.sig` comes from `tauri signer sign --app-version <version>`, whose trusted comment is `timestamp:<unix>\tfile:<name>\tversion:<version>`; the plugin reads the tab-separated `version:` field.

### Consequences

- `releases/latest/download` resolves only to the newest published release, so a draft release offers no update until it is published.
- If GitHub moves release assets to another host, update downloads fail with `HostNotAllowed` until an ADR adds it. They never fall back to an unchecked host.
- The plugin's `reqwest` client is compiled into the app but never called; docs/network-deps.md shows the path.
- A pending update lives in memory and is lost on relaunch; the user checks again.
- Tests sign packages with a throwaway Ed25519 key in the legacy minisign format (`ring`), since real `tauri signer` signatures are prehashed with BLAKE2b. A one-off run with a CLI-generated key and signature passed the same install path, and the tampered copy was rejected.

### New crates

| Crate | Reason | License | Network |
| --- | --- | --- | --- |
| `tauri-plugin-updater` 2.14.1 (`default-features = false`) | Manifest parsing, minisign verification, macOS install | MIT OR Apache-2.0 | Has its own `reqwest` client; never called (CI check above). `restore_update` and `install` make no request. |
| `minisign-verify`, `tar`, `filetime`, `xattr`, `osakit`, `objc2-osa-kit` (via the plugin) | Signature check, archive extraction, privileged install fallback | MIT, MIT OR Apache-2.0 | None |
| `ring` 0.17, `base64` 0.23 (dev only, both already in the build graph) | Sign throwaway update packages in tests | Apache-2.0 AND ISC, MIT OR Apache-2.0 | None |
