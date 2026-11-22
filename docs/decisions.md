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
- Air-Gap defaults to ON and persists in `settings.airgap_enabled`. Any value other than `"0"` means ON. OFF allows exactly one host per purpose, matched exactly and case-insensitively. `net::Client` accepts only `https` URLs, follows no redirects, and uses no proxy, because either would reach a host the guard never checked.
- `ScopedPath` canonicalizes the root and candidate, then requires the result to stay under the root. This resolves `..`, absolute paths, and symlinks. A symlink cycle fails as `Unresolvable`.

### Allowed hosts (placeholders)

| Purpose | Host |
| --- | --- |
| ModelDownload | `huggingface.co` |
| UpdateCheck | Host of `brand::SITE_URL` |

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
- **Global shortcut.** `CmdOrCtrl+Shift+Space` toggles the `quick-search` window (show and focus, or hide). It is registered in Rust during setup. If another app owns the shortcut, the app logs a warning and starts anyway.

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
- Tokens are counted as `ceil(chars / 4)` (`CharApprox`) until the embedding model's tokenizer lands; this undercounts Hangul and CJK.
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
- Reading a response body after `get` returns is not covered. Model downloads must re-check `turned_on` while they stream.

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
- GitHub repository URLs stay `skyoo2003/hush` until the repository is renamed.

### Consequences

- Existing dev installs start fresh. The old DB in `~/Library/Application Support/com.hush.app/`, the logs, and the keychain item `com.hush.app` are left in place, unused.
- Anyone with `HUSH_DEV_SIGN_IDENTITY` set must rename it to `LOCITE_DEV_SIGN_IDENTITY`.
