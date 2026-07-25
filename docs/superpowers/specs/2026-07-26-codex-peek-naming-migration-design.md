# CodexPeek 이름 마이그레이션 설계

## 목적

저장소 이름이 CodexPeek로 변경된 뒤 남아 있는 기존 패키지·실행 파일·배포 자산 이름을
`codex-peek`로 통일한다. 기존 설치 사용자의 설정과 자동 시작 상태는
잃지 않도록 호환성을 유지한다.

## 공개 이름 변경

| 대상 | 기존 이름 | 새 이름 |
| --- | --- | --- |
| Cargo 패키지 | 이전 패키지 이름 | `codex-peek` |
| Windows 실행 파일 | 이전 실행 파일 이름 | `codex-peek.exe` |
| Portable ZIP | 이전 Portable ZIP 이름 | `codex-peek-v<version>-windows-x86_64-portable.zip` |
| 설치 프로그램 | 이전 설치 프로그램 이름 | `CodexPeek-Setup-v<version>-x64.exe` |
| 진단 로그 | 이전 진단 로그 이름 | `%TEMP%\codex-peek.log` |
| Inno Setup 소스 | `packaging/windows/CodexUsageMonitor.iss` | `packaging/windows/CodexPeek.iss` |

`Cargo.toml` 버전과 `Cargo.lock`의 루트 패키지 버전은 `0.1.3`으로 올린다. 이미 공개된
`v0.1.2` 태그와 자산은 수정하거나 교체하지 않는다.

## 유지하는 호환성 식별자

다음 값은 기존 설치와 설정을 이어받기 위해 변경하지 않는다.

- 화면과 Windows 앱 목록의 표시명 `Codex Usage Monitor`
- 설정 디렉터리 `%APPDATA%\CodexUsageMonitor`
- 기본 설치 디렉터리 `%LOCALAPPDATA%\Programs\CodexUsageMonitor`
- Inno Setup AppId `{B4A07110-2028-46C9-9268-02C9322E48EA}`
- 자동 시작 값 이름 `CodexUsageMonitor`
- 단일 인스턴스 mutex `Local\CodexUsageMonitor.SingleInstance.v1`
- Win32 창 클래스와 내부 RPC client name

Rust 통합 테스트와 `src/main.rs`가 사용하는 라이브러리 crate 이름은
`codex_usage_monitor`로 유지한다. Cargo 패키지만 `codex-peek`로 변경하고
`Cargo.toml`의 `[lib] name = "codex_usage_monitor"`로 이 경계를 명시한다.

## Installer 업그레이드

동일한 AppId를 사용하므로 새 installer는 기존 설치를 업그레이드한다.

- 설치 전에 기존 mutex를 확인해 실행 중인 앱을 종료하도록 안내한다.
- 설치 디렉터리에 남은 이전 이름의 실행 파일만 제거한다.
- 새 `codex-peek.exe`와 문서를 설치하고 시작 메뉴 바로 가기를 새 실행 파일로 만든다.
- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexUsageMonitor` 값이 존재하면
  새 설치 경로의 `"codex-peek.exe" --startup` 명령으로 갱신한다.
- 자동 시작 값이 없으면 새로 만들지 않는다.
- 제거 시 기존과 동일하게 자동 시작 값만 삭제하고 설정과 로그는 보존한다.

Portable 업데이트에서는 앱을 종료하고 새 ZIP을 별도 폴더에 푸는 방식을 권장한다.
기존 폴더에 덮어 푸는 경우 남은 이전 이름의 실행 파일을 사용자가 직접 제거해야
함을 설치 문서에 명시한다.

## 빌드와 릴리스 계약

패키징 스크립트, Inno Setup 정의, installer smoke test, release workflow와 문서가 다음
세 자산 이름을 동일하게 사용해야 한다.

```text
CodexPeek-Setup-v0.1.3-x64.exe
SHA256SUMS.txt
codex-peek-v0.1.3-windows-x86_64-portable.zip
```

Portable ZIP에는 `codex-peek.exe`가 들어간다. 패키징 실패 시 부분 자산을 남기지 않고,
기존 자산을 덮어쓰지 않는 현재 정책을 유지한다.

## 보안과 데이터 경계

- `%USERPROFILE%\.codex\auth.json`의 내용은 읽거나 마이그레이션하지 않는다.
- installer는 기존 자동 시작 값의 존재 여부만 호환 처리에 사용한다.
- 자동 시작 값이 존재할 때만 앱이 소유한 고정 이름과 새 설치 경로로 값을 교체한다.
- 설정 스키마나 설정 파일 내용은 변경하지 않는다.
- 이전 진단 로그는 삭제하지 않으며 새 버전부터 `%TEMP%\codex-peek.log`에 기록한다.

## 검증

- 이름 계약 테스트를 먼저 새 이름으로 바꾸고 실패를 확인한다.
- Cargo metadata에서 패키지명 `codex-peek`, 버전 `0.1.3`, 라이브러리 crate
  `codex_usage_monitor`를 확인한다.
- 전체 Rust 테스트, Clippy, release 빌드를 실행한다.
- 실제 Inno Setup 컴파일과 격리된 installer 설치·업데이트·제거 smoke test를 실행한다.
- 기존 실행 파일 제거와 자동 시작 값 마이그레이션을 테스트한다.
- 저장소 전체에서 공개 legacy 이름이 호환 테스트·마이그레이션 코드 외에 남지 않았는지
  검색한다.
- `git diff --check`로 공백 오류를 검사한다.
