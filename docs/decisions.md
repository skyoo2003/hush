# 결정 기록 (ADR)

## ADR-001. 저장소는 SQLCipher + sqlite-vec(정적) + FTS5 단일 DB로 한다

- 날짜: 2026-11-22
- 상태: 채택
- 근거 예제: `spikes/sqlcipher_vec/` (일회용 스파이크, 제품 코드 아님)

### 결정

벡터(`vec0`), 키워드 색인(FTS5), 메타데이터를 SQLCipher 암호화 DB 파일 하나에 둔다. sqlite-vec는 `sqlite3_auto_extension`으로 정적 등록하고 dylib를 로드하지 않는다. 연결을 열 때 `PRAGMA cache_size`를 벡터 테이블 크기 이상으로 잡는다. 이 설정이 없으면 KNN이 10배 느려진다(아래 측정).

### 검증한 조합

| 구성 요소 | 버전 |
| --- | --- |
| `rusqlite` | 0.43.0, feature `bundled-sqlcipher-vendored-openssl` |
| `libsqlite3-sys` | 0.40.0 |
| SQLCipher | 4.20.0 community, SQLite 3.54.1 |
| OpenSSL (`openssl-src`) | 3.6.3, 정적 빌드 |
| `sqlite-vec` | 0.1.9 |
| 플랫폼 | Apple M4 (arm64), 10코어, 16 GB, macOS (Darwin 25.6.0), Rust 1.98.1 |

`sqlite-vec` 크레이트는 `sqlite-vec.c`를 `SQLITE_CORE`로 컴파일하고 SQLite 본체를 포함하지 않는다. 그래서 `sqlite3_*` 심볼은 libsqlite3-sys의 SQLCipher 정적 라이브러리로 연결된다. `otool -L` 결과는 `libiconv`와 `libSystem` 두 개뿐이고, `nm`으로 보면 `sqlite3_open_v2`와 `sqlite3_vec_init`이 각각 하나씩 바이너리 안에 있다.

### 검증 결과

`cargo run --release -- quick`으로 다시 실행할 수 있다. 각 항목은 `assert`로 확인한다.

1. 암호화 DB에서 `vec0(embedding float[384])`와 `fts5(body)`를 만들고 각각 1,000건을 넣었다. 1번 행의 벡터로 KNN을 하면 10행이 나오고, 1위가 1번 행이며 거리는 0.0이다. 거리는 오름차순이다. `MATCH 'topic3'`은 100행, `MATCH 'document AND "17"'`은 1행을 돌려준다. `sqlite_compileoption_used('ENABLE_FTS5')`는 1이다. **통과.**
2. 틀린 키와 키 없이 열면 첫 읽기에서 `SQLITE_NOTADB`(26, "file is not a database")로 실패한다. 맞는 키로 다시 열면 1,000행이 읽힌다. **통과.**
3. 연결이 열려 있고 체크포인트 전이라 WAL이 1.86 MB인 상태에서 DB, `-wal`, `-shm` 바이트를 대소문자 무시로 검색했다. `confidential-marker`, FTS5 토큰 `confidential`, 본문 `secret body`, 평문 헤더 `sqlite format 3`은 모두 0건이다. 연결을 닫은 뒤에도 0건이다. 대조군으로 같은 데이터를 평문 DB에 넣으면 스캐너가 5건을 찾는다. **통과.**
4. KNN 지연시간은 아래에 있다.

### KNN top-10 지연시간

측정 방법은 이렇다. 단위 정규화한 무작위 384차원 벡터를 L2 거리로 쓴다. N마다 새 DB를 만들고, 넣은 뒤 `wal_checkpoint(TRUNCATE)`를 하고, 새 연결로 연다. 워밍업 3회 뒤 20질의 × 5라운드 = 100질의를 잰다. 매 질의에서 10행이 나오는지 확인한다. 같은 바이너리를 두 번 실행했다. 두 번째 실행 p50은 첫 번째와 2% 안에서 같다. 라운드별 p50의 편차는 4% 미만이다. 측정 중 `softwareupdated`가 코어 하나를 썼다. KNN은 단일 스레드이고 코어는 10개다.

p50 ms (실행 1 / 실행 2), p95는 실행 2 값이다.

| N | 암호화, 기본 캐시 (2 MB) | 암호화, `cache_size=-1048576` (1 GiB) | 평문, 기본 캐시 | DB 크기 |
| --- | --- | --- | --- | --- |
| 1,000 | 0.16 / 0.16 (p95 0.18) | 0.18 / 0.17 (p95 0.18) | 0.28 / 0.28 | 2 MB |
| 100,000 | 183.1 / 179.9 (p95 184.2) | 16.9 / 16.7 (p95 16.9) | 30.3 / 29.6 | 160 MB |
| 300,000 | 545.3 / 541.5 (p95 547.9) | 50.7 / 50.6 (p95 51.2) | 90.5 / 89.9 | 478 MB |

300,000건 삽입은 한 트랜잭션에서 3.4초가 걸렸다(벡터만).

**지연은 N에 선형이다.** sqlite-vec 0.1.9의 `vec0`에는 ANN 색인이 없어서 KNN은 모든 청크를 훑는다. 100k에서 300k로 가면 세 구성 모두 약 3배가 된다.

**기본 캐시에서 한계 요인은 페이지 HMAC 검증이다.** 100k 기본 캐시 구간을 macOS `sample`로 12초 잡았다(보고한 수치와는 별도 실행). 상위 스택은 `sha512_block_armv8` 56%, `pread` 12%, `vec0Filter_knn_chunks_iter`(거리 계산) 8%, `aes_v8_cbc_encrypt` 7%다. 캐시가 2 MB라서 질의마다 160 MB 전체를 디스크 캐시에서 다시 읽고 HMAC-SHA512로 검증하고 복호화한다. 캐시를 DB보다 크게 잡으면 복호화한 페이지가 메모리에 남는다. 그러면 지연이 10.7배 줄고 평문 기본 캐시보다도 빨라진다. 평문 기본 캐시는 매 질의 `pread`로 페이지를 복사하기 때문이다. 큰 캐시 구성의 한계 요인은 거리 계산과 메모리 대역폭으로 추정한다. 이 구성은 프로파일하지 않았다.

### Pro 상한 추정

큰 캐시 구성은 벡터 1개당 0.169 µs(50.6 ms / 300k)이고, 캐시 메모리는 벡터 1개당 약 1.6 KB(478 MB / 300k)를 쓴다. 아래 값은 측정값을 선형으로 늘린 추정이다.

| 질의 예산 | 최대 벡터 수 | 필요한 페이지 캐시 |
| --- | --- | --- |
| 50 ms | 약 300k (측정) | 약 480 MB |
| 100 ms | 약 590k (추정) | 약 940 MB (추정) |
| 100 ms, 기본 캐시 | 약 55k (추정) | 2 MB |

16 GB 머신에서는 지연보다 RAM이 먼저 상한을 정한다. 문서 수로 바꾸려면 문서당 청크 수가 필요하다. 문서당 20청크라고 가정하면(근거 없는 가정) 300k 벡터는 문서 15,000개다. 하이브리드 검색의 FTS5와 리랭커 시간은 이 숫자에 들어 있지 않다.

### 결과 (영향)

- 연결 설정에 `cache_size`를 넣는다. 크기는 벡터 테이블 크기에 맞춰 정하고, 메모리 상한을 둔다.
- 복호화한 페이지는 프로세스 메모리에만 있다. 파일에는 쓰지 않으므로 절대 규칙 4를 어기지 않는다. 메모리가 스왑되면 디스크에 닿지만, macOS 스왑은 기본으로 암호화된다.
- 번들 SQLite는 `SQLITE_ENABLE_LOAD_EXTENSION=1`로 빌드되지만, 런타임에서 `load_extension`은 기본으로 꺼져 있다. 앱은 `enable_load_extension`을 호출하지 않는다.
- "평문 캐시·임시 파일 금지"(규칙 4)에 따라, 제품 코드에서는 `PRAGMA temp_store=MEMORY`도 확인한다. 이 스파이크에서는 검사하지 않았다.

### 검토한 대안

조합이 동작하므로 아래 대안은 채택하지 않는다. 비교만 남긴다.

| 대안 | 장점 | 단점 |
| --- | --- | --- |
| BLOB 컬럼 + Rust 브루트포스 | 의존성 없음 | `vec0`과 같은 전체 스캔이다. 같은 HMAC 비용을 내거나, 벡터를 Rust 메모리에 따로 올려야 한다. |
| BLOB 컬럼 + Rust HNSW | 1M 이상에서 질의가 1 ms 수준 | 색인을 메모리에 다시 만들어야 하고, 디스크에 저장하면 평문 파일 금지 규칙 때문에 DB 안에 직렬화해야 한다. 크레이트가 하나 더 늘어난다. |
| LanceDB | 이번 범위가 아님 | 지시에 따라 검토하지 않았다. |

다음에 측정할 것은 sqlite-vec의 `int8[384]`와 `bit[384]` 양자화다. 이 형식은 바이트 수를 4배와 32배 줄인다. 그러면 캐시 메모리와 HMAC 비용이 함께 줄어든다고 추정한다(측정하지 않음).

### 새 크레이트

| 크레이트 | 이유 | 라이선스 | 네트워크 |
| --- | --- | --- | --- |
| `rusqlite` | SQLite/SQLCipher 바인딩 | MIT | 런타임 없음. 빌드할 때 crates.io에서 받는다. |
| `libsqlite3-sys` (`bundled-sqlcipher-vendored-openssl`) | SQLCipher와 OpenSSL을 정적으로 빌드 | MIT. SQLCipher community는 BSD 계열이라 고지가 필요하다. | 런타임 없음 |
| `openssl-src` (간접) | 시스템 OpenSSL 없이 빌드 | Apache-2.0 (OpenSSL 3.x) | 런타임 없음. 소스는 크레이트에 들어 있다. |
| `sqlite-vec` | `vec0` 가상 테이블. C 소스를 정적으로 링크한다. | MIT OR Apache-2.0 | 런타임 없음 |

## ADR-002. 앱 골격: Tauri v2 + React/TS + Vite, 창 2개와 창별 ACL

- 날짜: 2026-11-22
- 상태: 채택

### 결정

- 창은 `main`과 `quick-search` 두 개다. 둘 다 `index.html`을 읽고, `App.tsx`가 `getCurrentWindow().label`로 화면을 고른다.
- 앞으로 만들 `/compare`, `/pack`, `/viewer`는 `main` 창 안의 경로로 둔다. 창을 더 만들지 않는다. 지금은 화면이 하나뿐이라 라우터를 넣지 않았다. 경로가 실제로 생길 때 라우터를 고른다.
- IPC 커맨드 9개는 `build.rs`의 `AppManifest::commands`에 등록한다. 그러면 커맨드마다 `allow-<name>` 권한이 생기고, capability가 허용하지 않은 커맨드는 거부된다. `quick-search`는 `search`, `open_source`, `copy_citation`, `hide_window`만, `main`은 `list_folders`, `get_settings`, `get_license_status`, `list_models`, `export_pack`만 부를 수 있다. `core:*`, fs, shell, http 권한과 Tauri 플러그인은 넣지 않았다.
- 브랜드 문자열은 `src-tauri/src/brand.rs`에만 쓴다. `build.rs`가 이 파일을 `include!`해서 `src/brand.ts`를 만들고, `tauri.conf.json`의 `productName`, `identifier`, 창 제목을 맞춘다. 내용이 같으면 파일을 쓰지 않는다. tauri-build가 `tauri.conf.json`을 감시하므로, 매번 쓰면 빌드가 끝없이 다시 돈다. `scripts/check-brand.sh`가 하드코딩을 잡고, CI는 `git diff --exit-code`로 생성 파일이 커밋본과 같은지 확인한다.
- CSP는 `script-src 'self'`이고 `connect-src`는 `ipc:`와 `http://ipc.localhost`뿐이다. 개발용 `devCsp`에만 Vite HMR용 `ws://localhost:1420`을 더했다. `@vitejs/plugin-react`는 쓰지 않는다. 이 플러그인은 인라인 refresh 스크립트를 넣어서 `script-src 'self'`에 막힌다. 그래서 React Fast Refresh는 없고, 고치면 페이지 전체가 다시 로드된다.
- Vite 8은 변환에 esbuild 대신 Oxc를 쓴다. `esbuild` 옵션은 deprecated이고 타입에 `jsx`가 없다. 그래서 `vite.config.ts`는 `oxc: { jsx: { runtime: "automatic" } }`로 JSX를 설정한다.

### ACL 확인 결과

`src-tauri/src/lib.rs` 테스트는 `tauri::test::mock_builder()`에 실제 `builder()`와 `generate_context!(test = true)`를 붙인다. 그래서 실제 capability 파일이 그대로 쓰인다. `test = true`는 macOS Info.plist 심볼(`_EMBED_INFO_PLIST`)만 빼는 옵션이다. 이 옵션이 없으면 `run()`의 `generate_context!()`와 심볼이 겹쳐서 테스트 빌드가 실패한다.

- `quick-search`에서 `get_settings`를 부르면 ACL이 거부한다. 메시지는 `get_settings not allowed on window "quick-search", webview "quick-search", URL: local ... referenced by: capability: main-window, permission: allow-get-settings`다. AppError가 아니다.
- `quick-search`의 `search`는 ACL을 통과하고 `not_implemented`를 돌려준다. `hide_window`는 Ok다.
- `main`의 `get_settings`는 통과하고(`not_implemented`), `search`는 거부된다.
- 테스트 요청의 URL은 `tauri://localhost`로 둔다. Tauri는 이 URL이 앱의 로컬 주소일 때만 `Origin::Local`로 보고, 다른 URL은 원격으로 취급한다. capability는 기본으로 로컬만 허용한다.
- Tauri 2.12 소스(`webview/mod.rs` `on_message`)를 보면, 로컬 요청의 앱 커맨드는 앱 ACL 매니페스트가 있을 때만 ACL을 검사한다. `AppManifest::commands`를 등록하지 않으면 로컬 창은 capability와 상관없이 모든 앱 커맨드를 부를 수 있다. 그래서 새 커맨드는 반드시 `build.rs`의 `COMMANDS`에도 넣어야 한다. 넣지 않은 커맨드를 capability에서 참조하면 빌드가 실패한다.
- 검증: `quick-search.json`에 `allow-get-settings`를 임시로 넣으면 테스트가 실패했다. 되돌리면 통과한다.
- mock 빌드에서는 setup 훅이 돌지 않고 설정의 창도 만들어지지 않는다. 테스트가 창을 직접 만든다. 로그 파일도 생기지 않는다.

### 번들 ID 경고

`com.hush.app`은 `.app`으로 끝난다. Tauri는 macOS에서 `.app` 확장자와 겹친다고 경고한다. 사용자가 정한 값이라 그대로 둔다. 배포 전에 바꿀지 다시 정한다. 바꾸면 키체인 서비스명(`KEYCHAIN_SERVICE`)과 로그·데이터 디렉터리도 같이 바뀐다.

### 새 크레이트

| 크레이트 | 이유 | 라이선스 | 네트워크 |
| --- | --- | --- | --- |
| `tauri` 2.12 | 데스크톱 셸, IPC, 창, ACL. dev-dependency에서만 `test` feature를 켠다. | MIT OR Apache-2.0 | 런타임에 직접 요청하지 않는다. 업데이터, http 플러그인은 넣지 않았다. |
| `tauri-build` 2.7 | 설정·capability 검증, 커맨드 권한 생성 | MIT OR Apache-2.0 | 없음 |
| `serde` | IPC 직렬화 | MIT OR Apache-2.0 | 없음 |
| `serde_json` | 테스트 직렬화. build-dependency에서는 `preserve_order`로 `tauri.conf.json`의 키 순서를 지킨다. | MIT OR Apache-2.0 | 없음 |
| `thiserror` 2 | 모듈별 에러 타입 | MIT OR Apache-2.0 | 없음 |
| `tracing` | 로그 | MIT | 없음 |
| `tracing-subscriber` (`env-filter`, `fmt`) | 로그 필터(`RUST_LOG`, 기본 `info`)와 포맷 | MIT | 없음 |
| `tracing-appender` | 앱 로그 디렉터리에 하루 단위로 파일을 돌리고 7개만 남긴다 | MIT | 없음 |

### 새 npm 패키지

| 패키지 | 이유 | 라이선스 | 네트워크 |
| --- | --- | --- | --- |
| `react`, `react-dom` 19 | UI | MIT | 없음 |
| `@tauri-apps/api` 2 | `invoke`, 현재 창 라벨 | MIT OR Apache-2.0 | 없음. IPC만 쓴다. |
| `@tauri-apps/cli` 2 (dev) | `tauri dev`, `tauri build`, 아이콘 생성 | MIT OR Apache-2.0 | 개발 도구. 앱에 들어가지 않는다. |
| `vite` 8 (dev) | 번들러, 개발 서버(1420 포트) | MIT | 개발 서버는 로컬에서만 연다. 앱에 들어가지 않는다. |
| `typescript` 7 (dev) | 타입 검사(strict) | Apache-2.0 | 없음 |
| `@biomejs/biome` (dev) | 린트 | MIT OR Apache-2.0 | 없음 |
| `vitest` (dev) | 프런트엔드 테스트 | MIT | 없음 |
| `@types/react`, `@types/react-dom` (dev) | 타입 | MIT | 없음 |
