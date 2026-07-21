# Task 3 런타임 서비스 보고서

## RED/GREEN 증거

- 설정: `cargo test --test config_runtime`는 공개 설정 타입이 없어서 실패한 뒤, 기본값·왕복 저장·손상 파일 백업·좌표 검증 4건이 통과했다.
- 폴러: `cargo test --test poller_runtime`는 `PollState`/`PollTrigger` 미정의로 실패한 뒤, 수동 10초 제한·백오프·stale·리셋 중복 방지·강제 인증 4건이 통과했다. 첫 GREEN 실행에서 stale 및 중복 리셋 결함을 발견하여 수정한 뒤 통과했다.
- 진단: `cargo test --test diagnostics_runtime`는 진단 API 미정의로 실패한 뒤 통과했다. 첫 GREEN 실행에서 Bearer 다음 값이 남는 결함을 발견해 마스킹 후 통과했다.
- 업데이트: `cargo test --test update_check_runtime`는 검사 API 미정의로 실패한 뒤 유효하지 않은 메타데이터, 24시간 제한, 신규 버전, 비정상/과대/위험 URL 3건이 통과했다. 이후 실제 `UreqHttpClient`가 없어서 실패하는 HTTPS 거부 테스트를 추가하고 통과했다.
- 지역화: `cargo test --test localization_runtime`는 키/조회 함수 미정의로 실패한 뒤 모든 키와 두 언어의 비어 있지 않은 문구 검사가 통과했다.

## 구현 범위

- 버전 1 설정, 동일 디렉터리 임시 파일 동기화, Windows `MoveFileExW` 교체, 손상 파일 보관.
- 1 MiB 단일 회전 진단 로그와 토큰·계정·이메일·프록시 형태 방어적 마스킹.
- `UsageProvider::fetch(&self, allow_auth_refresh)` 및 공유 single-flight 보존.
- 순수 폴링 상태 기계와 채널 기반 워커, 쿨다운/백오프/stale/리셋 중복 방지.
- 주입 가능한 GitHub 최신 릴리스 검사와 엄격한 저장소/릴리스 URL 검증, HTTPS 전용 `UreqHttpClient`.
- 폴링·메뉴·진단·업데이트·창 레이블의 한/영 전체 문구.

## 최종 검증

`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets` 모두 통과했다. 전체 테스트는 단위 37건과 런타임 통합 15건이 통과했다.

## 자체 검토

- UI, 트레이, 작업 표시줄, 레지스트리 변경은 추가하지 않았다.
- 인증 파일 내용, RPC 원문, 토큰, 계정 식별자, 이메일, 프록시 값을 읽거나 기록하지 않는다.
- 업데이트 확인은 릴리스 페이지 URL만 반환하며 자산을 내려받거나 실행하지 않는다.
- 후속 작업에서는 실제 `ureq` 어댑터를 UI/앱 조립 단계에서 주입하고 Windows에서 원자 교체 실패 경로를 수동 점검해야 한다.
