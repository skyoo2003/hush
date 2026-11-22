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
