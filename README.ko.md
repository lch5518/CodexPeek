# Codex 사용량 모니터

[English version](README.md)

## ❤️ 후원

CodexPeek가 시간을 절약해 드린다면 개발을 후원해 주세요.

- ⭐ 이 저장소에 Star 남기기
- ❤️ [GitHub에서 후원하기](https://github.com/sponsors/lch5518)

후원해 주실 때마다 프로젝트를 활발하게 유지하는 데 큰 도움이 됩니다.

Codex 사용량 모니터는 Codex 사용량을 빠르게 확인하는 Windows 네이티브 위젯입니다.
기본·보조 사용량 기간을 작업 표시줄, 플로팅 위젯, 시스템 트레이에 표시합니다.

![Codex 사용량 모니터 작업 표시줄 위젯](docs/images/taskbar-widget.png)

## 주요 기능

- Codex 기본·보조 사용량 기간과 초기화 시각을 표시합니다.
- 인증 파일을 직접 파싱하지 않고, 설치된 Codex CLI의 `app-server` 인터페이스를 사용합니다.
- 작업 표시줄에 안전하게 붙일 수 없을 때는 플로팅 위젯과 트레이 아이콘으로 동작합니다.
- 수동·자동 갱신, Windows 시작 시 실행, 진단, 한국어·영어 UI를 지원합니다.

## 작동 방식

모니터는 로컬 자식 프로세스로 `codex app-server --stdio`를 실행하고 표준 입출력으로 JSONL 메시지를 주고받습니다.
인증은 설치된 Codex CLI가 기존 설정과 네트워크 정책에 따라 처리하며, 필요하면 OpenAI와 통신할 수 있습니다.

모니터는 로그인 상태와 화면 표시에 필요한 사용량 기간만 요청합니다.
Codex 작업을 시작하거나 `codex exec`를 호출하지 않습니다.

## 요구 사항

- Windows 10 또는 Windows 11, x64.
- `account/read`, `account/rateLimits/read` RPC를 지원하는 로그인된 [Codex CLI](https://github.com/openai/codex).
- 소스 빌드 시 Rust 1.85 이상, Visual Studio 2022 C++ Build Tools, Windows SDK.

## 빌드 및 실행

현재 설치 프로그램과 WinGet 패키지는 제공하지 않습니다.
Codex CLI를 설치하고 로그인한 뒤 소스에서 빌드하세요.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release

Start-Process .\target\release\codex-usage-monitor.exe
```

UI를 열지 않고 CLI, app-server 연결, 로컬 설정을 점검하려면 다음 명령을 실행합니다.

```powershell
.\target\release\codex-usage-monitor.exe --diagnose
```

`--startup`은 트레이 메뉴에서 등록한 Windows 자동 시작 경로에서만 사용합니다.

## 사용 방법

트레이 메뉴에서 사용량을 새로 고치고 1/5/10/15/30분 갱신 간격을 선택하거나 위젯을 표시·숨길 수 있습니다.
Windows 시작, 시작 화면, 인증 갱신, 자동 인증 갱신, 언어, 진단도 여기서 설정합니다.

사용량 요청은 한 번에 하나만 실행됩니다.
요청이 실패하면 간격을 늘려 재시도하며, 마지막으로 성공한 사용량은 계속 표시합니다.

Explorer 재시작이나 작업 표시줄 배치 변경으로 위젯을 붙이지 못하면 트레이 아이콘은 계속 사용할 수 있습니다.
모니터는 작업 표시줄 연결을 안전하게 다시 시도합니다.

## 개인정보 및 보안

모니터는 `%USERPROFILE%\.codex\auth.json`의 내용을 읽거나 파싱하지 않습니다.
진단에서는 해당 경로의 존재 여부만 확인합니다.

원시 RPC 응답은 로그인 유형과 화면에 표시할 사용량 필드를 추출하는 동안에만 처리합니다.
토큰, 계정 ID, 이메일, 인증 파일 내용, 프록시 값은 저장하거나 로그에 기록하지 않습니다.

설정은 `%APPDATA%\CodexUsageMonitor\settings.json`에 저장합니다.
크기가 제한된 진단 로그는 `%TEMP%\codex-usage-monitor.log`에 저장합니다.

데이터 처리와 취약점 보고 안내는 [SECURITY.md](SECURITY.md)를 참고하세요.

## 문제 해결

| 문제 | 해결 방법 |
| --- | --- |
| Codex CLI를 찾을 수 없음 | `codex --version`, `where.exe codex`를 실행하고 Codex CLI가 `PATH`에 있는지 확인하세요. |
| 지원하지 않는 CLI | Codex CLI를 업데이트하세요. 표시된 버전보다 필요한 RPC 지원 여부가 중요합니다. |
| 로그아웃 또는 인증 만료 | Codex CLI에서 정상 로그인 절차를 완료한 뒤 트레이 메뉴의 **인증 갱신**을 선택하세요. |
| 작업 표시줄 위젯이 보이지 않음 | 플로팅 위젯이나 트레이 아이콘을 사용하고, 필요하면 Explorer를 다시 시작한 뒤 표시 모드를 다시 선택하세요. |
| 자세한 상태가 필요함 | `--diagnose` 또는 트레이 메뉴의 **진단**을 사용하세요. |

## 개발

소스 빌드를 공유하기 전에는 다음 검사를 실행하세요.

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

자동화된 검사는 [릴리스 체크리스트](docs/RELEASE_CHECKLIST.md)의 Windows, DPI, 다중 모니터, Explorer 복구 검증을 대체하지 않습니다.

## 라이선스

이 프로젝트는 [MIT License](LICENSE)로 제공됩니다.
서드파티 고지는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)를 참고하세요.
