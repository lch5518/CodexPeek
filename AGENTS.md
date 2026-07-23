# AGENTS.md

## 프로젝트 개요

이 저장소는 **Codex Usage Monitor**의 소스 코드입니다. 설치된 Codex CLI의
`codex app-server --stdio` JSONL RPC를 통해 사용량 제한 창을 조회하고, 이를 Windows
작업 표시줄 위젯, 플로팅 위젯, 시스템 트레이에 표시하는 Rust 2021 네이티브 앱입니다.

- 대상 플랫폼: Windows 10/11 x64
- 도구 체인: Rust 1.85 이상, MSVC, Windows SDK
- 패키지/실행 파일: `codex-usage-monitor`
- 설정: `%APPDATA%\\CodexUsageMonitor\\settings.json`
- 안전 진단 로그: `%TEMP%\\codex-usage-monitor.log`

UI를 열지 않고 설치 상태와 연결을 점검하려면 다음을 사용합니다.

```powershell
.\target\release\codex-usage-monitor.exe --diagnose
```

## 구조와 책임

```text
src/
  main.rs                 실행 진입점 및 오류 종료 코드
  app.rs                  런타임 조립, UI 액션 처리, 업데이트 확인 시작
  domain.rs               사용량 제한 창 도메인 모델 및 표시 규칙
  poller.rs               단일 요청 폴링 상태 기계, 재시도/백오프/초기화 시각 처리
  config.rs               설정 검증, 원자적 저장, 비동기 설정 기록기
  diagnostics.rs          민감 정보 없이 수행하는 진단과 순환 로그
  localization.rs         한국어/영어 UI 문자열
  update_check.rs         제한된 GitHub 릴리스 메타데이터 확인과 표시 상태
  errors.rs               사용자에게 안전하게 노출할 수 있는 오류 분류
  codex/
    locator.rs             지원되는 Codex CLI 탐색과 버전 확인
    process.rs             app-server 자식 프로세스, 파이프, Job Object 수명 관리
    app_server.rs          시간/크기가 제한된 JSONL RPC와 사용량 DTO 변환
  windows/
    native/                Win32 UI 구현
    widget.rs              플로팅 위젯 기하와 DPI 변환
    taskbar*.rs            작업 표시줄 배치와 실패 시 복구 규칙
    tray.rs                트레이 메뉴와 플랫폼 구현
    autostart.rs           HKCU Run 등록
    lifecycle.rs           Explorer/창 수명 복구 상태
tests/
  *_runtime.rs             공개 API와 상태 기계를 사용하는 통합 성격 테스트
  build_resources.rs       아이콘 및 빌드 리소스 검증
build.rs, build_support.rs Windows 아이콘, 매니페스트, 버전 리소스 생성
```

`src/lib.rs`에 노출된 타입은 테스트와 플랫폼 경계에서 사용하는 공개 계약입니다. UI 코드에
도메인 규칙을 복제하지 말고, 표시 규칙은 `domain.rs` 또는 `localization.rs`에 둡니다.

## 반드시 지켜야 할 동작 및 보안 경계

1. `%USERPROFILE%\\.codex\\auth.json`의 **내용을 읽거나 파싱하지 않습니다**. 존재 여부만
   안전 진단에서 확인할 수 있습니다. 사용량과 로그인 상태는 설치된 Codex CLI의
   `app-server` RPC만 사용합니다.
2. 토큰, 계정 ID, 이메일, 인증 파일 내용, 프록시 URL/자격 증명, 원본 RPC payload를
   구조체·로그·오류 메시지·테스트 fixture에 보관하지 않습니다. 필요한 필드만 즉시 역직렬화하고
   `UsageError`와 `SafeDiagnostic`의 안정적인 분류로 변환합니다.
3. `app-server` 작업은 단일 요청만 허용하고, 시간 제한·JSONL 프레임 크기 제한·자식 프로세스
   정리를 유지합니다. 새 RPC를 추가할 때도 `ProcessGuard`와 Job Object를 우회하지 않습니다.
4. 폴링 실패 시 마지막 정상 사용량을 유지합니다. 현재 백오프는 1/2/4/8/15분이며, 수동 새로
   고침은 10초 쿨다운을 가집니다. 이 정책을 변경하면 `PollState` 단위 테스트와 런타임 테스트를
   함께 갱신합니다.
5. 설정은 허용된 값만 저장하고 임시 파일 후 원자적으로 교체합니다. 스키마 변경에는 명시적인
   마이그레이션 또는 안전한 기본값 복구와 손상 파일 백업이 필요합니다.
6. 작업 표시줄 연결은 선택 기능입니다. Explorer 재시작, DPI 변경, 다중 모니터, 자동 숨김에서
   실패하더라도 트레이와 플로팅 위젯이 계속 동작하도록 복구 경로를 보존합니다.
7. 업데이트 확인은 HTTPS, 응답 크기/시간 제한, 검증된 정확한 GitHub 태그 URL만 허용합니다.
   업데이트 파일을 다운로드·교체·실행하지 않습니다. 현재 공식 빌드에서는 Cargo 메타데이터에
   HTTPS `repository`가 없으면 확인 기능이 비활성화됩니다.

## 구현 규칙

- 단순한 표준 라이브러리 기반 구현을 우선합니다. 새 의존성은 기존 `windows`, `serde`, `ureq`
  등으로 해결할 수 없는 경우에만 추가하고, 목적과 공급망 영향을 PR 설명에 남깁니다.
- Rust 2021과 최소 Rust 1.85 호환성을 유지합니다. 새 API에는 필요한 가시성만 부여하고,
  `unsafe`는 Win32 호출을 감싼 작은 모듈에 한정하며 안전 전제를 주석으로 설명합니다.
- 모든 새/수정 public API, 복잡한 상태 전이, I/O 부작용에는 한국어 rustdoc(`///`)를 작성합니다.
  입력, 반환값, 부작용 또는 제약을 구체적으로 설명하되 자명한 내용을 반복하지 않습니다.
- 사용자 화면의 문구는 `localization.rs`에 한국어와 영어를 함께 추가합니다. 오류의 내부 원인을
  사용자에게 그대로 노출하거나 문자열 비교로 제어 흐름을 만들지 않습니다.
- 외부 I/O는 제한 시간, 실패 처리, 민감 정보 비노출을 갖춰야 합니다. 잘못된 입력은 명확히
  거절하고, 외부 의존성 실패는 기존 표시를 보존하는 방식으로 안전하게 실패합니다.
- UI 이벤트 처리에서 장시간 I/O를 하지 않습니다. 기존처럼 워커 스레드와 `UiBackend` 경계를
  사용하고, 종료 시 백그라운드 작업과 자식 프로세스가 정리되는지 확인합니다.
- `target/`과 로컬 설정/로그는 생성물입니다. 수정하거나 커밋하지 않습니다. 관련 없는 리팩터링,
  포맷 변경, 문구 변경을 기능 작업에 끼워 넣지 않습니다.

## 테스트와 검증

변경 범위에 맞는 테스트를 먼저 추가하거나 수정합니다. 시간·재시도·프로세스 종료·Windows API는
실제 시간이나 실제 계정에 의존하지 않는 결정적 테스트를 선호하고, trait 기반 backend/provider
대역을 사용합니다.

일반 변경 전 최소 검증은 다음과 같습니다.

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

배포 가능 여부를 확인하거나 빌드 스크립트/Windows 리소스를 변경했다면 추가로 실행합니다.

```powershell
cargo build --release
git diff --check
```

UI·작업 표시줄·DPI·자동 시작 변경은 자동 테스트만으로 완료하지 않습니다. Windows 10/11,
100/125/150/200% DPI, 다중 모니터, 작업 표시줄 자동 숨김, Explorer 재시작, Codex CLI 누락/로그아웃
시나리오를 [릴리스 체크리스트](docs/RELEASE_CHECKLIST.md) 기준으로 수동 확인합니다.

## 변경 작업 절차

1. 먼저 관련 도메인, 플랫폼, 테스트 모듈과 `README.md`/`SECURITY.md`를 읽어 기존 계약을
   확인합니다.
2. 입력·출력·스레드 경계·민감 정보·Windows 복구 영향이 있는지 짧게 분석합니다. 요구가
   불명확하거나 보안 경계를 넓히면 구현 전에 더 안전한 대안을 제시합니다.
3. 가장 작은 변경으로 구현하고, 변경한 동작을 검증하는 테스트를 추가합니다. 설정 또는 공개
   동작이 바뀌면 문서와 한국어/영어 문자열도 함께 갱신합니다.
4. 변경 범위에 맞는 검사와 `git diff --check`를 실행합니다. 실행하지 못한 검증과 수동 확인이
   필요한 항목은 결과에 분명히 남깁니다.

## 릴리스

- 버전의 단일 기준은 `Cargo.toml`이며 Semantic Versioning을 사용합니다.
- 릴리스 태그는 패키지 버전과 같은 `v<version>` 형식입니다.
- 태그 푸시 시 `.github/workflows/release.yml`이 포맷·테스트·Clippy·릴리스 빌드를 수행하고
  Windows x64 ZIP을 GitHub Release에 첨부합니다.
- 자세한 절차와 수동 검증은 `docs/RELEASE_CHECKLIST.md`를 따릅니다. 이미 게시한 릴리스
  자산을 조용히 교체하지 말고, 수정이 필요하면 새 패치 버전을 발행합니다.

## 커밋 메시지 규칙

커밋 제목은 반드시 `<접두사>: <간결한 설명>` 형식을 사용합니다. 콜론 앞에는 공백을 넣지 않고,
제목은 마침표 없이 현재형 또는 명령형으로 작성합니다.

| 접두사 | 사용 시점 | 예시 |
| --- | --- | --- |
| `feat:` | 사용자에게 보이는 기능 추가 | `feat: Add tray refresh interval menu` |
| `fix:` | 의도된 동작을 복구하는 버그 수정 | `fix: Preserve usage after RPC timeout` |
| `docs:` | 문서만 변경 | `docs: Describe app-server security boundary` |
| `test:` | 테스트 추가 또는 수정 | `test: Cover taskbar recovery after Explorer restart` |
| `refactor:` | 동작 변화 없는 코드 구조 개선 | `refactor: Extract poll scheduling helper` |
| `build:` | 빌드 스크립트, 의존성, 도구 체인 변경 | `build: Embed version resource in executable` |
| `ci:` | GitHub Actions 등 CI 변경 | `ci: Verify release tag version` |
| `chore:` | 위 분류에 속하지 않는 유지보수 | `chore: Update third-party notices` |

한 커밋에는 하나의 논리적 변경만 담습니다. 보안 경계, 설정 스키마, 공개 API처럼 호환성에 영향을
주는 변경은 본문에 영향·마이그레이션·검증 결과를 기록하고, 필요하면 `BREAKING CHANGE:`로 명시합니다.
