# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex 사용량 모니터는 Codex 사용량을 빠르게 확인하는 Windows 네이티브 위젯입니다.
기본·보조 사용량 기간을 작업 표시줄, 플로팅 위젯, 시스템 트레이에 표시합니다.

![Codex 사용량 모니터 작업 표시줄 위젯](../images/taskbar-widget.png)

## 주요 기능

- Codex 기본·보조 사용량 기간과 초기화 시각을 표시합니다.
- 최근 성공 조회를 바탕으로 각 창의 소진 시점을 추정하고 사용량 상세와 작업 표시줄 툴팁에
  표시합니다(이번 릴리스의 새 기능).
- 인증 파일을 직접 파싱하지 않고, 설치된 Codex CLI의 `app-server` 인터페이스를 사용합니다.
- 최대 8개의 격리된 사용량 프로필 중 하나를 수동으로 선택할 수 있습니다.
- 다중 모니터 Windows 환경에서 모든 작업 표시줄 또는 주 모니터에만 위젯을 표시할 수 있습니다.
- 작업 표시줄에 안전하게 붙일 수 없을 때는 플로팅 위젯과 트레이 아이콘으로 동작합니다.
- 수동·자동 갱신, Windows 시작 시 실행, 진단, 지역화된 UI를 지원합니다.
- 시작 후 새 릴리스를 확인하고 체크섬을 검증한 자동 업데이트를 현재 위치에 적용할 수 있습니다.

## 작동 방식

모니터는 로컬 자식 프로세스로 `codex app-server --stdio`를 실행하고 표준 입출력으로 JSONL 메시지를 주고받습니다.
인증은 설치된 Codex CLI가 기존 설정과 네트워크 정책에 따라 처리하며, 필요하면 OpenAI와 통신할 수 있습니다.

모니터는 로그인 상태와 화면 표시에 필요한 사용량 기간만 요청합니다.
Codex 작업을 시작하거나 `codex exec`를 호출하지 않습니다.

## 사용량 프로필

삭제할 수 없는 **기본 Codex 계정** 프로필은 CodexPeek 시작 시 상속한 Codex 홈을 사용하며,
`CODEX_HOME`이 없으면 CLI 기본값을 사용합니다. 관리 프로필을 추가하면 각각
`%APPDATA%\CodexPeek\profiles` 아래의 분리된 Codex 홈을 사용합니다. 시스템 프로필을
포함해 전체 8개까지 만들 수 있습니다.

프로필 표시명은 사용자가 직접 지정합니다. CodexPeek은 계정 이메일이나 ID를 확인하지 않으므로
계정을 추가하거나 다시 로그인할 때 브라우저에서 사용할 ChatGPT 계정을 직접 확인하세요. 프로필
선택은 CodexPeek이 조회하고 표시하는 사용량만 바꿉니다. 터미널, IDE, Codex 앱, WSL,
Remote SSH, Dev Containers의 로그인은 바뀌지 않습니다.

선택은 항상 수동입니다. CodexPeek은 남은 한도에 따라 프로필을 자동 선택·순환하거나 Codex 작업을
특정 프로필로 라우팅하지 않습니다. 관리 프로필을 삭제하면 그 안에 별도로 저장된 CLI 인증 정보를
포함한 로컬 프로필 데이터를 복구할 수 없으므로 확인 안내를 주의 깊게 읽으세요.

CodexPeek은 어떤 프로필의 `auth.json`도 읽거나 파싱하거나 복사하지 않습니다. 관리 프로필의
자식 `app-server`에만 해당 `CODEX_HOME`과 파일 인증 저장소 설정을 적용하며, 진단에는 프로필
표시명·경로·계정 정보 없이 집계된 개수만 기록합니다.

### 프로필 관리자

시스템 프로필은 이름을 바꿀 수 있지만 로그아웃하거나 삭제할 수는 없습니다. 사용자 지정 시스템
프로필 이름은 CodexPeek에 표시되는 내용만 바꾸며 계정 식별자가 아닙니다. 기본 계정 표시는
프로필 관리자에서만 보입니다.

트레이의 **사용량 프로필** 하위 메뉴에서는 프로필을 선택하거나 **사용량 프로필 관리**를 열 수
있으며, 추가 명령은 없습니다. 새 프로필은 관리자 목록 아래의 `+`에서만 추가합니다. 창 아래에는
닫기 또는 추가 단추가 없으므로 창의 `X` 또는 Esc로 관리자를 닫습니다.

## 요구 사항

- Windows 10 또는 Windows 11, x64.
- `account/read`, `account/rateLimits/read` RPC를 지원하는 로그인된 [Codex CLI](https://github.com/openai/codex).

## 다운로드 및 실행

먼저 PowerShell에서 Codex CLI 설치와 로그인 상태를 확인하세요.

```powershell
codex --version
codex login status
```

### 설치 프로그램(권장)

1. [최신 GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest)에서
   `CodexPeek-Setup-v<version>-x64.exe`를 다운로드합니다.
2. 설치 프로그램을 실행하고 안내에 따라 설치합니다. 관리자 권한은 필요하지 않습니다.
3. 설치가 끝나면 시작 메뉴에서 **Codex Usage Monitor**를 실행합니다.

### Portable

1. 최신 Release에서
   `codex-peek-v<version>-windows-x86_64-portable.zip`을 다운로드합니다.
2. ZIP을 쓰기 가능한 폴더에 완전히 압축 해제합니다.
3. 압축을 푼 폴더에서 `codex-peek.exe`를 실행합니다.

### 소스에서 직접 빌드

Rust 1.85 이상, Visual Studio 2022 C++ Build Tools, Windows SDK가 필요합니다.
이 방법은 복제한 저장소에서 앱을 실행하며 시작 메뉴 바로 가기와 제거 프로그램을
만들지 않습니다.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
.\scripts\build-release.ps1
.\target\release\codex-peek.exe
```

PowerShell에서는 저장소 루트에서 스크립트를 실행합니다. 완전히 다시 빌드하려면
`-Clean`을, 빌드 후 바로 실행하려면 `-Run`을 함께 사용하세요.

```powershell
.\scripts\build-release.ps1
.\scripts\build-release.ps1 -Clean -Run
```

PowerShell 실행 정책으로 막히면 다음처럼 명시적으로 실행하세요.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
```

명령 프롬프트나 파일 탐색기에서는 `.cmd` 래퍼를 사용하세요.

```bat
scripts\build-release.cmd
scripts\build-release.cmd -Clean -Run
```

래퍼는 `target\release\codex-peek.exe`를 빌드하며, `-Run`을 지정하면 빌드 성공 후
해당 실행 파일을 시작합니다. `-Clean`을 사용하기 전에는 실행 중인 CodexPeek를 종료하세요.

UI를 열지 않고 빌드 결과와 Codex CLI 연결을 확인하려면 다음을 실행합니다.

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Codex에 설치 요청

아래 프롬프트를 그대로 Codex에 복사하세요. 검증된 Installer를 우선 사용하고, 호환되는
Release 파일이 없을 때만 소스 빌드로 전환합니다.

```text
이 Windows x64 컴퓨터에 CodexPeek를 설치하고 검증까지 완료해줘.

1. Windows x64 환경인지 확인하고 `codex --version`과 `codex login status`를 실행해줘.
2. 다음 공식 저장소와 그 Releases만 사용해줘.
   https://github.com/lch5518/CodexPeek
3. 최신 `CodexPeek-Setup-v<version>-x64.exe`를 우선 사용해줘. Installer와
   `SHA256SUMS.txt`를 함께 다운로드하고, 체크섬 파일에서 Installer의 정확한 항목을
   찾은 다음 직접 계산한 SHA-256과 비교해줘. 해시가 일치할 때만 진행하고, 보안 기능을
   끄거나 체크섬이 없거나 다른 파일을 실행하지 마.
4. 관리자 권한을 요청하지 말고 현재 사용자용으로 설치해줘. 기존 CodexPeek 설정은
   보존하고, 실행 중인 앱이나 관련 없는 프로세스를 임의로 종료하지 말아줘. 앱을
   종료해야 한다면 내가 직접 종료할 수 있도록 먼저 알려줘.
5. 호환되는 Release 파일을 사용할 수 없을 때만 공식 저장소를 쓰기 가능한 새 사용자
   폴더에 clone하고 `cargo build --release`를 실행해줘. Git, Rust 1.85 이상,
   Visual Studio 2022 C++ Build Tools 또는 Windows SDK 설치가 필요하면 무엇이
   변경되는지 먼저 설명하고 내 승인을 받아줘.
6. `%USERPROFILE%\.codex\auth.json`의 내용을 읽거나 출력하지 마. 인증은 설치된
   Codex CLI를 통해서만 처리해줘.
7. 설치 또는 빌드 후 생성된 `codex-peek.exe --diagnose`를 실행해줘. 진단에 성공하면
   CodexPeek를 실행해줘.
8. 사용한 설치 방식, 설치된 버전, 실행 파일 위치, 체크섬 결과와 진단 결과를 알려줘.
   실패하면 민감 정보를 노출하지 말고 안전하게 중단한 뒤 정확한 원인을 설명해줘.
```

Installer와 Portable은 `%APPDATA%\CodexPeek\settings.json`을 사용하므로 전환해도
설정을 공유합니다. Installer는 시작 메뉴 바로 가기를 만들지만 Windows 자동 시작은
기본으로 활성화하지 않습니다.

수동 실행과 Windows 자동 시작 모두 앱 화면이 먼저 열린 뒤 최신 GitHub Release를 확인합니다.
새 버전이 있으면 업데이트 여부를 묻습니다. **이번 버전 건너뛰기**를 선택하면 같은 버전은 다시
묻지 않고, 더 새 버전이 나왔을 때 다시 알립니다. **지금 업데이트**를 선택하면 릴리스의 Windows
x64 원본 EXE와 `SHA256SUMS.txt`를 내려받아 SHA-256을 확인한 뒤 현재 실행 파일 위치를 교체하고
CodexPeek을 다시 시작합니다. `%APPDATA%\CodexPeek`은 수정하지 않으므로 기존 설정과 프로필은
유지됩니다. 검증 또는 교체가 실패하면 기존 실행 파일을 보존하며 수동으로 업데이트할 수 있습니다.

helper는 다시 시작한 앱이 트레이와 초기 창을 만들 때까지 이전 EXE 백업을 보존합니다. 준비 확인이
실패하면 해당 프로세스의 종료를 확인한 뒤 이전 EXE를 복원해 다시 실행합니다. 종료를 확인할 수
없으면 백업을 보존하고 rollback이나 두 번째 실행을 하지 않습니다. Windows 자동 시작이었다면
`--startup` 모드도 유지합니다.
Installer 버전은 새 Setup을 설치하기 전까지 Windows 앱 목록에 기존 설치 버전이 표시될 수 있습니다.

소스에서 직접 빌드한 실행 파일은 공식 릴리스 빌드로 표시되지 않으므로 인앱 업데이트가
비활성화됩니다. 앱 버전마다 한 번, 공식 릴리스로 교체하면 직접 빌드한 변경이 포함되지 않는다는
경고를 표시합니다. 이런 빌드는 소스를 다시 받아 빌드하거나 공식 릴리스를 수동으로 설치하세요.

초기 릴리스와 자동 업데이트용 원본 EXE는 코드 서명되지 않아 Microsoft Defender SmartScreen
경고가 나타날 수 있습니다. 체크섬은 다운로드한 바이트가 릴리스 manifest와 일치하는지 확인하지만
Authenticode 게시자 서명을 대신하지는 않습니다. 공식 GitHub Release만 사용하고 자동 검증을
완료할 수 없으면 수동으로 확인하세요.

해시 확인, 업데이트, 제거와 문제 해결은 [상세 설치 가이드](../INSTALL.md)를
참고하세요.

## 사용 방법

트레이 메뉴에서 사용량을 새로 고치고 1/5/10/15/30분 갱신 간격을 선택하거나 위젯을 표시·숨길 수 있습니다.
Windows 시작, 시작 화면, 인증 갱신, 자동 인증 갱신, 언어, 진단도 여기서 설정합니다.
**위젯: 모든 모니터** 또는 **위젯: 주 모니터만**을 선택해 다중 모니터 표시 범위를 정할 수 있으며, 선택은 다시 시작해도 유지됩니다.

기본값에서는 Windows 로캘이 지원 언어와 일치할 때 UI 언어를 자동으로 따릅니다. 트레이 메뉴에서 언어를 직접 선택할 수도 있습니다. 지원 언어는 한국어, 영어, 스페인어, 브라질 포르투갈어, 인도네시아어, 일본어, 힌디어, 독일어, 프랑스어, 베트남어, 터키어, 아랍어입니다.

작업 표시줄 위젯의 글자색은 Windows의 밝은/어두운 시스템 테마를 따르고, 배경에는 실제 작업 표시줄 재질이 그대로 비칩니다.

사용량 요청은 한 번에 하나만 실행됩니다.
요청이 실패하면 간격을 늘려 재시도하며, 마지막으로 성공한 사용량은 계속 표시합니다.

Explorer 재시작이나 작업 표시줄 배치 변경으로 위젯을 붙이지 못하면 트레이 아이콘은 계속 사용할 수 있습니다.
모니터는 작업 표시줄 연결을 안전하게 다시 시도합니다.

예측은 기본으로 켜져 있으며 성공한 조회만 별도 로컬 파일
`%APPDATA%\CodexPeek\usage-history.json`에 저장합니다. 같은 프로필·창·초기화 주기의 최신
데이터가 충분히 쌓인 뒤에만 예상 소진 정보를 표시하며, 새 데이터 수집 중이거나 오래된
데이터는 현재 예측처럼 표시하지 않습니다. 트레이의 **사용량 소진 예측** 메뉴에서 끄거나
**사용량 소진 예측 기록 삭제**를 선택할 수 있고, 관리 프로필을 삭제하면 해당 기록도 함께
삭제됩니다. 예측은 로컬 추정치이며 OpenAI 한도 정책을 보장하지 않고 기록을 업로드하거나
동기화하지 않습니다.

위젯 좌측 상단 점은 현재 표시 중인 사용량 창의 소비 속도를 요약합니다. 초록은 여유, 주황은
보통, 빨강은 현재 속도라면 초기화 전에 한도를 소진할 수 있음을 뜻합니다. 호버 상세에는 최근
관측 시간, 사용량 증가폭과 대략적인 시간당 속도를 쉬운 문장으로 표시합니다. 로딩 또는 판단
불가는 회색, 조회 오류는 기존 빨간 느낌표를 사용합니다.

## 개인정보 및 보안

모니터는 `%USERPROFILE%\.codex\auth.json`의 내용을 읽거나 파싱하지 않습니다.
진단에서는 해당 경로의 존재 여부만 확인합니다.

원시 RPC 응답은 로그인 유형과 화면에 표시할 사용량 필드를 추출하는 동안에만 처리합니다.
토큰, 계정 ID, 이메일, 인증 파일 내용, 프록시 값은 저장하거나 로그에 기록하지 않습니다.

설정은 `%APPDATA%\CodexPeek\settings.json`에 저장합니다.
크기가 제한된 진단 로그는 `%TEMP%\codex-peek.log`에 저장합니다.

`usage-history.json`에는 내부 프로필 ID, `Primary` 또는 `Secondary` 창 종류, 사용률,
선택적인 초기화 시각, 성공 조회 시각만 저장합니다. 이메일, 계정 ID, 사용자 지정 프로필
이름·루트 경로, 토큰, 인증 파일 내용, 대화·프롬프트 내용, 프록시 설정값, 원시 RPC 응답은
저장하지 않습니다. 기록은 최대 30일, 프로필·창별 1,000개까지 보존하며 같은 값과 5분보다
짧은 간격의 조회는 디스크 쓰기를 줄이기 위해 건너뜁니다. 손상된 기록 파일은 격리하거나
초기화하지만 사용량 표시는 계속됩니다.

확인 후 **사용량 소진 예측 기록 삭제**를 실행하면 모든 표본을 지울 수 있습니다. Installer와
Portable 제거는 `%APPDATA%\CodexPeek`를 보존하므로 앱을 삭제한 뒤에도 기록이 남을 수 있습니다.
완전히 정리하려면 트레이에서 기록을 지우거나 파일·폴더를 직접 삭제하세요.

데이터 처리와 취약점 보고 안내는 [SECURITY.md](../../SECURITY.md)를 참고하세요.

## 문제 해결

| 문제 | 해결 방법 |
| --- | --- |
| Codex CLI를 찾을 수 없음 | `codex --version`, `where.exe codex`를 실행하고 Codex CLI가 `PATH`에 있는지 확인하세요. |
| 지원하지 않는 CLI | Codex CLI를 업데이트하세요. 표시된 버전보다 필요한 RPC 지원 여부가 중요합니다. |
| 로그아웃 또는 인증 만료 | Codex CLI에서 정상 로그인 절차를 완료한 뒤 트레이 메뉴의 **인증 갱신**을 선택하세요. |
| 위젯 표시 범위를 바꾸고 싶음 | 트레이 메뉴에서 **위젯: 모든 모니터** 또는 **위젯: 주 모니터만**을 선택하세요. |
| 작업 표시줄 위젯이 보이지 않음 | 플로팅 위젯이나 트레이 아이콘을 사용하고, 필요하면 Explorer를 다시 시작한 뒤 원하는 위젯 모니터 모드를 선택하세요. |
| 자세한 상태가 필요함 | `--diagnose` 또는 트레이 메뉴의 **진단**을 사용하세요. |

## 개발

소스 빌드에는 Rust 1.85 이상, Visual Studio 2022 C++ Build Tools, Windows SDK가
필요합니다. 저장소 루트에서 빌드하고 검사하세요.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

자동화된 검사는 [릴리스 체크리스트](../RELEASE_CHECKLIST.md)의 Windows, DPI, 다중 모니터, Explorer 복구 검증을 대체하지 않습니다.

## ❤️ 후원

CodexPeek가 시간을 절약해 드린다면 개발을 후원해 주세요.

- ⭐ 이 저장소에 Star 남기기
- ❤️ [GitHub에서 후원하기](https://github.com/sponsors/lch5518)

후원해 주실 때마다 프로젝트를 활발하게 유지하는 데 큰 도움이 됩니다.

## 라이선스

이 프로젝트는 [MIT License](../../LICENSE)로 제공됩니다.
서드파티 고지는 [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)를 참고하세요.
