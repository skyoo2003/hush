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

### Pro cap estimate

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
- The 9 IPC commands are registered through `AppManifest::commands` in `build.rs`. Each command then gets an `allow-<name>` permission, and any command a capability does not grant is rejected. `quick-search` may call only `search`, `open_source`, `copy_citation`, and `hide_window`; `main` may call only `list_folders`, `get_settings`, `get_license_status`, `list_models`, and `export_pack`. No `core:*`, fs, shell, or http permissions and no Tauri plugins (ADR-004 adds `global-shortcut`, used from Rust only, with no JS permission granted).
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

`com.hush.app` ends with `.app`, which Tauri warns conflicts with the `.app` bundle extension on macOS. It stays because the user chose it; decide again before release. Changing it also changes the keychain service name (`KEYCHAIN_SERVICE`) and the log and data directories.

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
| License | `license-provider.invalid` until a provider is chosen. `.invalid` never resolves. |
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
- Status: Accepted

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
