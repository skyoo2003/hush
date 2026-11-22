# AirGap Search — Agent Rules (PRD v2.0)

## 제품 한 줄

지정한 로컬 폴더 문서를 100% 온디바이스로 인덱싱·검색·요약하고, 모든 결과에 원문 출처(파일·페이지·위치·발췌)를 붙이는 Zero-Cloud RAG 데스크톱 앱. Tauri v2 + Rust + React/TS. 티어: Free / Supporter / Lite / Pro.

## 절대 규칙 (위반이 필요하면 작업 중단 후 보고)

1. 문서 내용·청크·임베딩·질의·파일 경로를 네트워크로 보내는 코드를 작성하지 않는다.
2. 모든 아웃바운드 HTTP는 `net::Client` 하나로만 하고, 매 요청 전 `security::airgap::AirGapGuard::check(NetPurpose)`를 호출한다. 허용 용도는 License, ModelDownload, UpdateCheck 3가지뿐이고, 용도마다 허용 호스트는 1개다. 후원은 외부 브라우저나 QR로만 처리하고 앱에서 요청하지 않는다.
3. 파일 읽기는 `security::paths::ScopedPath`로 검증된 경로만 허용한다. 등록 폴더 밖, `..`, 심볼릭 링크 탈출은 금지다.
4. 경로·스니펫·메타데이터·벡터·FTS 색인·변경 이력은 SQLCipher DB 하나에만 저장한다. 평문 캐시·임시 파일은 금지다.
5. 로그에 문서 본문·스니펫·질의·파일명·키를 남기지 않는다. 식별이 필요하면 doc_id를 쓴다. 검색 횟수 같은 카운터는 남겨도 되지만, 질의 문자열은 어디에도 저장하지 않는다.
6. 키·토큰·라이선스 키는 `Zeroizing`/`SecretBox`로만 다루고 Debug/Display를 구현하지 않는다.
7. LLM 답변에서 인용이 없는 문장은 사용자에게 보여주지 않는다. 근거가 부족하면 「제공된 로컬 문서에서 확인되지 않음」을 반환한다.
8. 티어 권한은 `entitlement::EntitlementManager`만 판단한다. 권한이 필요한 IPC 커맨드는 본문 첫 줄에서 `require(Capability::X)`를 호출한다. 프론트엔드의 티어 분기는 표시용이며 보안 경계가 아니다.
9. 브랜드명·번들 ID·키체인 서비스명·파일 확장자·도메인은 `brand.rs`(Rust)와 `src/brand.ts`(빌드 시 생성) 상수로만 참조한다. 문자열 하드코딩은 금지다.
10. 유료 티어는 기능을 더하기만 한다. 문서·폴더 수 제한, 검색 품질 차등, 안정성·안전 기능(스케줄러, PII 경고, 근거 부족 게이트, 크래시 복구)의 티어 분기는 만들지 않는다. 구독이 끝나도 사용자 데이터는 지우거나 숨기지 않는다. 자체 라이선스 서버도 만들지 않는다.

## 모듈 경계

- ingest/: 파일 → ParsedDocument → Vec<Chunk>. DB·모델은 모른다.
- rag/: 임베딩, hybrid(벡터+키워드+리랭커), synthesizer(출처 Q&A와 가드), agpack. 파일시스템은 모른다.
- storage/: SQLCipher 연결, 스키마·마이그레이션, FTS, 변경 이력, 키체인.
- security/: AirGapGuard, ScopedPath, pii_guard, license(토큰 검증).
- entitlement.rs: PlanTier, TierCapabilities, 한도 검사. 다른 모듈을 호출하지 않는 순수 로직이다.
- commands.rs: IPC 진입점만 둔다. 권한 검사 → 서비스 호출 → AppError 변환 순서를 지킨다.
- 모듈별 error.rs(thiserror). IPC 경계에서만 AppError { code, message }로 변환한다. 권한 부족 코드는 `upgrade_required`다.

## 코딩 규칙

- Rust stable, edition 2021. unwrap/expect는 테스트에서만 쓴다. unsafe는 FFI에서만 쓰고 SAFETY 주석을 단다.
- 파싱·OCR·임베딩·리랭크·LLM은 spawn_blocking이나 전용 워커에서 돌린다. async 런타임을 막지 않는다.
- 새 크레이트를 쓰기 전에 이유·라이선스·네트워크 사용 여부를 docs/decisions.md에 기록한다. 런타임 자동 다운로드 기능은 끄거나 다른 것으로 바꾼다.
- IPC 타입은 Rust에서 생성한다(tauri-specta 또는 ts-rs). TS strict를 쓰고, 타입을 손으로 두 번 정의하지 않는다.
- DB 스키마 변경은 항상 새 마이그레이션으로 한다. 이미 배포한 마이그레이션은 수정하지 않는다.

## 작업 방식

- 구현 전에 입출력 구조체·에러 타입·공개 시그니처를 먼저 제시한다.
- 기능 코드와 단위 테스트는 같은 변경에 넣는다. 티어가 필요한 기능은 Free에서 거부되고 허용 티어에서 통과하는 테스트를 포함한다.
- 완료 전 실행: cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && pnpm lint && pnpm typecheck && pnpm test
- 끝나면 docs/progress.md에 날짜·한 일·TODO·제약을 추가한다.
- 요구가 모호하거나 절대 규칙과 충돌하면, 선택지 2~3개를 제시하고 멈춘다.
