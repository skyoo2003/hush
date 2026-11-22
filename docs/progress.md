# 진행 기록

## 2026-11-22

한 일
- `spikes/sqlcipher_vec/`에 SQLCipher + sqlite-vec(정적, `sqlite3_auto_extension`) + FTS5 스파이크를 만들었다. 암호화, 틀린 키 거부, WAL/SHM 평문 누출 없음, KNN과 MATCH 결과를 모두 확인했다.
- 1k, 100k, 300k에서 KNN top-10을 쟀다. 결과와 결정은 `docs/decisions.md` ADR-001에 있다.

TODO
- 연결 설정에 `cache_size`를 넣는다. 메모리 상한 정책을 정한다.
- 문서당 평균 청크 수를 실제 문서로 재서 Pro 상한을 문서 수로 바꾼다.
- `vec0`의 `int8[384]`와 `bit[384]` 양자화 지연과 재현율을 잰다.
- 제품 코드에서 `PRAGMA temp_store=MEMORY`를 확인한다.

제약
- 측정 환경은 Apple M4 한 대다. Intel Mac과 Windows는 재지 않았다.
- `vec0`는 브루트포스라서 지연은 벡터 수에 선형이다.
