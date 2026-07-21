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

## 마무리 보완 RED/GREEN 증거

- 업데이트 HTTP 계약: fake client가 요청 URL, 고정 User-Agent, 10초 timeout, 구성한 최대 본문 크기를 모두
  기록하도록 확장했다. `release_url_must_be_the_exact_tag_page`는 이전의 접두사 기반 검증으로
  `.../releases/tag/v2.0.0/assets`를 업데이트로 잘못 보고하여 RED로 실패했다. 태그 이름과 정확히 일치하는
  HTTPS GitHub 태그 페이지 비교로 바꾼 뒤 GREEN으로 통과했다. 같은 런타임 테스트는 dotted 저장소,
  동등/이전 버전, malformed·초과 크기·비-2xx 응답 및 다운로드/외부 URL도 다룬다.
- 지역화 완전성: 요구된 36개 키(상태, 표시 모드, 갱신 간격, 자동 시작/시작 화면, 인증 갱신, 항상 위,
  언어, 위치 초기화, 진단, 업데이트, 창/주·보조 창 레이블, CLI/RPC/login/settings/proxy/taskbar 진단)를
  명시적으로 검사하도록 했다. `LocalizationKey::ALL`에서 taskbar 진단 키를 제외한 RED 실행은
  `left: 35, right: 36`으로 실패했고, 키를 복원한 GREEN 실행은 한글·영문 비어 있지 않은 문구까지 통과했다.
- 문서: `UsageProvider::fetch(bool)`의 인증 갱신 인자와 오류/동시성 제약, `AppServerUsageProvider::new()`이
  더 이상 인증 정책을 저장하지 않는다는 점을 수정했다. 새 설정·진단·폴러·업데이트 공개 API 문서에는
  인자, 반환값, I/O 또는 안전 제약을 보강했다.

## 최종 검증 갱신

- `cargo test --all-targets`: 단위 38건, 런타임 통합 32건, 총 70건 통과.
- `cargo fmt --check`: 통과.
- `cargo clippy --all-targets --all-features -- -D warnings`: 통과.
