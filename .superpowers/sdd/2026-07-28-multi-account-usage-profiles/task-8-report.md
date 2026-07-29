# Task 8: Runtime Integration and Safe Diagnostics

## 1. Problem analysis

- Tasks 1–7에서 제공한 프로필 설정 worker, 프로필별 폴링/계정 worker, 타입 지정 UI 동작을
  실제 `AppRuntime`에 연결해야 했습니다.
- 프로필 추가는 설정 저장 성공 뒤에만 컨텍스트 생성과 로그인을 시작하고, 삭제는 진행 중인
  프로필 작업의 quiesce 완료 뒤에만 저장소 삭제를 제출해야 했습니다.
- 선택 변경과 로그인 완료는 내구성 설정 성공 전까지 기존 렌더링을 유지해야 하며, 프로필별
  오류와 마지막 정상 사용량은 다른 프로필에 영향을 주면 안 됩니다.
- 진단은 최대 8개의 집계 개수만 기록하고 프로필 이름, 숫자 식별자, 관리 경로, 계정 정보와 RPC
  원문을 포함하지 않아야 했습니다.

## 2. Assumptions and risks

- `UiBackend::snapshot`과 `settings`가 `&self` 계약이므로 이벤트 drain 중 상태를 갱신하기 위해
  `ProfileRuntimeState`를 짧게 잠그는 `Mutex`로 소유합니다. 외부 I/O는 잠금 안에서 실행하지
  않으며 worker 명령 제출은 채널 전송만 수행합니다.
- 프로필 삭제 저장이 실패하면 설정과 마지막 정상 스냅샷을 보존하고 해당 프로필의 quiesce
  표시만 제거합니다. `PollState`, 오류 백오프와 다음 예약 시각은 변경하지 않습니다.
- 실제 Win32 대화상자, 브라우저 로그인, Windows 10/11 및 DPI/Explorer 시나리오는 이 환경에서
  수동 실행하지 않았습니다. 기존 플랫폼 독립 확인/타입 변환/수명 테스트는 모두 통과했습니다.

## 3. Solution design

- `ProfileRuntimeState`를 순수 상태 전이 계층으로 추가하고 `ProfileRuntimeCommand`만 반환하게 해
  설정 worker와 폴링 worker 사이의 순서를 결정적으로 테스트했습니다.
- 시작 시 설정을 먼저 읽고 앱 전용 `UsageProfileRoot`에서 시스템/관리 실행 컨텍스트를 만든 뒤,
  설정 worker와 선택 우선 프로필 폴러를 시작합니다.
- `snapshot`, `settings`, `dispatch` 시작 시 두 이벤트 큐를 비차단 drain합니다. 성공 이벤트는
  프로필 catalog만 병합해 더 최신 UI 환경설정을 덮지 않습니다.
- 프로필 설정 요청에는 서비스별 단조 증가 불투명 ID와 타입 지정 작업 종류를 부여합니다. 성공과
  실패 이벤트는 ID와 종류가 현재 pending 작업에 모두 일치할 때만 상태를 바꾸며 preference와
  시작 정리 실패는 프로필 pending 상태를 해제하지 않습니다.
- 모든 add/select/rename/delete/login/logout UI 동작은 기존 직렬 worker에 명령만 제출하고 즉시
  최신 UI 복사본을 반환합니다. 로그인 성공은 내구성 select 뒤에 `ForcedAuth` 갱신을 실행합니다.
- 위젯은 저장된 선택 ID의 스냅샷만 사용하고, 프로필 목록은 각각의 독립 스냅샷에서 남은 주간
  비율, 갱신 중, 로그인 필요 또는 사용 불가 요약을 만듭니다.
- 종료 시 폴링 worker를 먼저 취소·join하고 설정 worker 명령을 모두 처리한 뒤 stop·join합니다.
- UI 진단은 용량이 제한된 `try_send` 큐에만 제출합니다. 파일 생성·회전·추가는 진단 worker가
  수행하고 종료 시 앞선 이벤트를 drain한 뒤 join합니다.
- `SafeDiagnostic::Profiles`는 설정 유효성과 configured/ok/login-required/request-failed 개수만
  포함하며 기록 시 각 값을 8로 제한합니다.

## 4. Implementation

변경 파일:

- `src/app.rs`: 프로필 런타임 상태 기계, AppRuntime worker 통합, 이벤트 drain, 선택 프로필 렌더,
  프로필 요약, 상관관계가 있는 설정 이벤트, 비동기 집계 진단, 명시적 shutdown을 구현했습니다.
- `src/diagnostics.rs`: aggregate-only `Profiles` 안전 진단 이벤트, 실제 설정/root 기반 집계기,
  제한된 비동기 진단 writer를 추가했습니다.
- `src/lib.rs`: 결정적 런타임 테스트가 사용하는 프로필 런타임 타입을 공개했습니다.
- `src/profile_poller.rs`: 마지막 정상 사용량과 백오프를 보존하는 profile resume 명령을 추가했습니다.
- `src/profile_settings.rs`: 단조 증가 요청 ID, 타입 지정 작업 종류와 label/path를 숨기는 안전 Debug를
  추가했습니다. 기존 `submit -> io::Result<()>`와 `ProfileSettingsEvent` shape는 유지하고 런타임용
  correlated submit/event API를 별도로 제공해 공개 API 호환성을 보존했습니다.
- `src/codex/app_server.rs`: 단일 프로필 런타임에서만 쓰던 중복 시스템 로그인 helper를 제거하고
  프로필 계정 worker 경로만 유지했습니다.
- `tests/profile_runtime.rs`: 내구성 add/delete/select 순서, 로그인 성공/취소/오류, logout,
  선택/비선택 삭제, 저장 실패 resume, preference/stale event interleaving을 검증했습니다.
- `tests/profile_poller_runtime.rs`: resume 뒤 자동/수동 조회, last-good/backoff 보존, 비선택 삭제 뒤
  authoritative selection refresh를 검증했습니다.
- `tests/diagnostics_runtime.rs`: 개수 제한과 sentinel 설정 이름/root 경로가 실제 집계기와 비동기
  writer를 통과한 뒤에도 로그에 기록되지 않음을 검증했습니다.

Task 9 문서 변경은 시작하지 않았습니다.

## 5. Usage example or test example

TDD RED 증거:

- 최초 런타임 테스트는 `ProfileRuntimeState`/`ProfileRuntimeCommand` 미구현으로 컴파일 실패했습니다.
- 집계 진단 테스트는 `SafeDiagnostic::Profiles` 미구현으로 컴파일 실패했습니다.
- 최신 UI preference 보존 테스트는 `show_remaining_percent`가 설정 성공 이벤트에 의해 되돌아가며
  실패했고, catalog만 병합하도록 수정한 뒤 통과했습니다.
- 선택 프로필 무캐시 로딩 테스트는 렌더 상태 helper 부재로 실패했고, 무캐시를 `Loading`으로
  분류한 뒤 통과했습니다.
- resume 테스트는 `ProfilePollingService::resume` 부재로 실패한 뒤 target quiesce 표시만 제거하도록
  구현해 자동/수동 조회와 last-good 보존이 통과했습니다.
- 요청 상관관계 테스트는 요청 ID/작업 종류/event 필드 부재로 실패한 뒤 monotonic ID와 matching
  전이를 구현해 preference/stale/mismatched 이벤트 격리가 통과했습니다.
- 비동기 진단 테스트는 writer/sink 경계 부재로 실패한 뒤 UI enqueue의 worker-thread 실행과
  shutdown drain/join이 통과했습니다.
- 실제 redaction 테스트는 집계 API 부재로 실패한 뒤 sentinel label/root를 실제 생성 경로에
  전달해도 aggregate log에 포함되지 않음을 검증했습니다.

검증 결과:

- `cargo test --test profile_runtime --test profile_poller_runtime --test diagnostics_runtime`: PASS
  (37 + 11 + 10 tests)
- `cargo test --all-targets`: PASS
- `cargo fmt --all -- --check`: PASS
- `git diff --check`: PASS
- `cargo clippy --all-targets --all-features -- -D warnings`: 기존
  `src/config.rs:271`의 `clippy::redundant_closure_call`만으로 FAIL
- `cargo clippy --all-targets --all-features -- -D warnings -A clippy::redundant-closure-call`: PASS

## 6. Possible improvements

- 릴리스 전 `docs/RELEASE_CHECKLIST.md`에 따라 Windows 10/11, 100/125/150/200% DPI, 다중 모니터,
  작업 표시줄 자동 숨김, Explorer 재시작, 실제 관리 프로필 로그인/로그아웃/삭제를 수동 확인해야
  합니다.
