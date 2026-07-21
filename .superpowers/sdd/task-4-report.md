# Task 4 네이티브 Windows 애플리케이션 보고서

## 1. 문제 분석

Task 1~3의 도메인, 안전한 app-server 공급자, 설정/진단/폴링/업데이트 서비스를 변경하지 않고 네이티브 Windows 실행 파일로 조합해야 했다. 핵심 위험은 Win32 핸들 소유권, Explorer 재시작과 작업 표시줄 연결 실패, DPI/모니터 좌표 변환, 레지스트리 명령 인용, UI 스레드에서의 장시간 RPC/네트워크 작업, 그리고 인증·프록시·RPC 원문이 UI나 진단에 노출되는 것이었다.

## 2. 가정과 위험

- 업데이트 저장소 메타데이터는 아직 `Cargo.toml`에 없으므로 Task 5 정책대로 네트워크 확인은 비활성 상태다. 유효한 GitHub 저장소 URL이 나중에 추가되면 동일 코드가 24시간 제한과 검증된 태그 URL 정책을 적용한다.
- 작업 표시줄 알림 영역은 알려진 `TrayNotifyWnd` 또는 `ClockButton` 직접 자식을 대상으로 한다. 다른 Explorer 구조에서는 임의 위치에 붙이지 않고 부동 위젯으로 안전하게 대체한다.
- Windows 10/11, 자동 숨김, 실제 다중 모니터, Explorer 재시작의 수동 시각 검증은 수행하지 않았다. 이는 Task 6 릴리스 검증에서 기록해야 하며 이 보고서에서는 수행했다고 주장하지 않는다.

## 3. 해결 설계

- `app.rs`만 `PollingService`, `SettingsStore`, `DiagnosticLogger`, `UpdateChecker`를 소유한다.
- Windows 계층은 `WidgetViewModel`, `UiSettings` 불변 복사본을 읽고 `UiAction`만 보낸다. 인증 파일 내용, RPC 원문, 프록시 값은 경계를 통과하지 않는다.
- 순수 계층에서 메뉴 ID 매핑, 실행 모드, 최초 표시 정책, DPI 좌표 왕복, 부동 창 제한, 작업 표시줄 배치, 자동 시작 명령/백엔드를 검증한다.
- Win32 계층은 이름 있는 뮤텍스, Per-Monitor V2, 숨은 메시지 창, 트레이 아이콘, GDI 위젯, 작업 표시줄 연결/복구를 한 UI 스레드에서 관리한다.
- 네트워크 업데이트와 메뉴 진단은 백그라운드 스레드에서 실행한다. 업데이트는 검사기가 반환한 정확한 GitHub 태그 페이지만 다시 검증해 연다.

## 4. 구현 및 RED/GREEN 기록

### RED 1

`tests/windows_app.rs`에 다음 7개 동작 테스트를 먼저 추가했다.

- 모든 메뉴 ID의 형식화된 동작 매핑
- 엄격한 `--startup`/`--diagnose` 인자 처리
- 96/120/144/192 DPI 부동 레이아웃 및 영역 비중첩
- 화면 밖 위치 제한
- 주/보조 작업 표시줄 배치, 세로/공간 부족 거부
- 자동 시작 명령의 정확한 인용과 레지스트리 왕복 검증
- 따옴표가 든 실행 경로의 기록 전 거부

첫 실행 `cargo test --test windows_app`은 `codex_usage_monitor::windows`가 없어 컴파일 실패했다. 이는 기능 부재로 인한 기대한 RED였다. 순수 모듈 구현 후 같은 테스트 7개가 통과했다.

### RED 2

자체 검토에서 자동 시작 트레이 전용 상태가 저장된 `widget_visible`을 오염시킬 수 있고, 음수/고 DPI 화면 좌표 반올림이 잘못될 수 있음을 발견했다. 최초 표시 정책과 150% DPI 양방향 좌표 테스트를 먼저 추가했고, API 부재로 기대한 RED를 확인했다. 구현 후 집중 테스트 9개가 모두 통과했다.

### GREEN 구현 범위

- Windows subsystem 진입점과 엄격한 실행 모드
- Per-Monitor DPI Awareness V2, 단일 인스턴스 뮤텍스, 숨은 메시지 창, 메시지 루프
- 동적 단색 미터 트레이 아이콘, 정확한 아이콘/메뉴 정리, `TaskbarCreated` 복원
- 모든 요구 메뉴 동작과 한국어/영어 체크 메뉴
- 380x112 논리 픽셀 부동 GDI 위젯, Segoe UI, 두 사용량 행, 상태/마지막 성공, 퍼센트, 막대, 수준별 색상+형태
- 드래그 이동, 논리 좌표/모니터 저장, 저장 모니터 복구와 작업 영역 제한, 항상 위, `WM_DPICHANGED`
- DPI 스케일된 380x48 논리 작업 표시줄 위젯과 두 줄 압축 렌더링
- 주/보조 작업 표시줄 검색, 저장 모니터 우선, 세로/공간 부족 거부, 알림 영역 회피, `SetParent`/`GetParent` 검증
- 1초 복구 타이머, 잘못된 부모/Explorer 재시작/연결 실패 시 부동 창 대체
- HKCU Run `REG_SZ` 기록/정확한 재읽기 검증/삭제와 상승 권한 없는 구현
- 부모 콘솔 연결 진단, 설정/프록시 존재 여부/인증 경로와 존재 여부/작업 표시줄/CLI-app-server-login-response 검사
- 인증 파일 미열람, 프록시 값 미출력, 안정 코드만 기록
- 백그라운드 업데이트 검사와 검증된 GitHub 태그 페이지 전용 `ShellExecuteW`

## 5. 사용 및 테스트 예

```powershell
cargo build
target\debug\codex-usage-monitor.exe
target\debug\codex-usage-monitor.exe --startup
target\debug\codex-usage-monitor.exe --diagnose
```

최종 검증 결과:

- `cargo test --all-targets`: 85개 통과, 실패 0
- `cargo fmt --all -- --check`: 통과
- `cargo clippy --all-targets --all-features -- -D warnings`: 통과
- `cargo build`: 통과
- `Start-Process ... --diagnose -Wait`: 종료 코드 0
- `git diff --check`: 통과

## 6. 가능한 개선

- Task 6에서 Windows 10/11, 100/125/150/200% 실제 DPI, 다중 모니터, 세로/자동 숨김 작업 표시줄, Explorer 재시작을 수동 검증한다.
- 실제 Explorer 빌드에서 알림 영역이 더 깊게 중첩된 사례가 확인되면 클래스 이름을 추측하기보다 검증된 자식 열거 규칙을 추가한다.
- 갱신 간격 또는 자동 인증 정책 변경 시 현재 폴러를 재시작하므로 진행 중 RPC가 최대 제한 시간까지 UI 동작 완료를 늦출 수 있다. 이후 `PollingService`에 형식화된 재구성 명령을 추가하면 UI 스레드 대기를 제거할 수 있다.
- Task 5에서 원본 리소스 아이콘, 버전 리소스, 저장소 메타데이터와 패키징을 추가한다.
