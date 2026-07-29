# Codex Usage Monitor 설치 가이드

[README로 돌아가기](translations/README.ko.md)

이 문서는 소스 코드를 직접 빌드하지 않고 Windows x64용 설치 프로그램 또는 Portable
ZIP으로 Codex Usage Monitor를 설치하는 방법을 설명합니다.

## 1. 설치 전 확인

다음 항목이 필요합니다.

- Windows 10 또는 Windows 11 x64
- `account/read`, `account/rateLimits/read` RPC를 지원하는 Codex CLI
- Codex CLI에 로그인된 상태

PowerShell에서 다음 명령으로 Codex CLI 설치와 로그인 상태를 확인하세요.

```powershell
codex --version
codex login status
```

Codex CLI가 없으면 [Codex CLI 공식 저장소](https://github.com/openai/codex)의 설치
안내를 따르세요. 로그인이 필요하면 다음 명령을 실행합니다.

```powershell
codex login
```

Codex Usage Monitor는 `%USERPROFILE%\.codex\auth.json` 내용을 직접 읽지 않습니다.
로그인과 사용량 조회는 설치된 Codex CLI를 통해서만 수행합니다.

## 2. 배포 파일 다운로드

[최신 GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest)에서 다음
파일을 다운로드할 수 있습니다.

| 파일 | 용도 |
| --- | --- |
| `CodexPeek-Setup-v<version>-x64.exe` | 일반 사용자에게 권장하는 설치 프로그램 |
| `codex-peek-v<version>-windows-x86_64-portable.zip` | 설치 없이 압축을 풀어 실행하는 Portable 버전 |
| `SHA256SUMS.txt` | 두 배포 파일의 SHA-256 무결성 확인 |

`<version>`은 실제 릴리스 번호로 표시됩니다.

## 3. 설치 프로그램으로 설치하기

설치 프로그램은 관리자 권한 없이 현재 Windows 사용자 계정에 설치됩니다.

1. `CodexPeek-Setup-v<version>-x64.exe`와 `SHA256SUMS.txt`를 같은 폴더에
   다운로드합니다.
2. 아래의 [SHA-256 확인](#5-sha-256-확인) 절차로 파일을 검증합니다.
3. 설치 프로그램을 실행하고 안내에 따라 설치합니다.
4. 설치가 끝나면 **Codex Usage Monitor 실행**을 선택하거나 시작 메뉴에서
   **Codex Usage Monitor**를 실행합니다.

기본 설치 경로는 다음과 같습니다.

```text
%LOCALAPPDATA%\Programs\CodexUsageMonitor
```

설치 프로그램은 시작 메뉴 바로 가기를 만들지만 바탕 화면 바로 가기는 만들지 않습니다.
Windows 자동 시작도 기본으로 활성화하지 않습니다.

### SmartScreen 경고가 나타나는 경우

초기 릴리스는 코드 서명되지 않아 Microsoft Defender SmartScreen이
**Windows의 PC 보호** 또는 **인식할 수 없는 앱** 경고를 표시할 수 있습니다.

1. 파일을 이 저장소의 공식 GitHub Release에서 받았는지 확인합니다.
2. `SHA256SUMS.txt`와 파일 해시가 일치하는지 확인합니다.
3. 두 조건을 확인한 경우에만 SmartScreen의 **추가 정보**에서 실행 여부를 결정합니다.

출처나 해시를 확인할 수 없다면 실행하지 마세요.

## 4. Portable 버전 사용하기

Portable 버전은 설치 권한이 없거나 원하는 폴더에서 직접 실행하려는 사용자에게
적합합니다.

1. `codex-peek-v<version>-windows-x86_64-portable.zip`과
   `SHA256SUMS.txt`를 다운로드합니다.
2. 아래 절차로 ZIP의 SHA-256 해시를 검증합니다.
3. ZIP을 쓰기 가능한 폴더에 완전히 압축 해제합니다.
4. 압축을 푼 폴더에서 `codex-peek.exe`를 실행합니다.

ZIP 안에서 실행하지 마세요. 예를 들어 다음과 같은 사용자 폴더에 압축을 풀 수 있습니다.

```text
%LOCALAPPDATA%\Programs\CodexPeekPortable
```

Portable은 앱 파일을 설치하거나 시작 메뉴 바로 가기를 만들지 않습니다. 다만 설정을
실행 파일 옆에 저장하는 완전 무설치 방식은 아닙니다. Installer와 동일한 사용자 설정
경로를 사용합니다.

## 5. SHA-256 확인

배포 파일과 `SHA256SUMS.txt`를 같은 폴더에 둔 뒤 PowerShell을 여세요. 다음 예제의
`<version>`을 다운로드한 파일의 실제 버전으로 바꿉니다.

```powershell
$file = "CodexPeek-Setup-v<version>-x64.exe"
$checksumLine = Get-Content .\SHA256SUMS.txt |
    Where-Object { $_ -match "  $([regex]::Escape($file))$" }

if ($null -eq $checksumLine) {
    throw "SHA256SUMS.txt에서 파일을 찾을 수 없습니다."
}

$expected = $checksumLine.Substring(0, 64)
$actual = (Get-FileHash -LiteralPath ".\$file" -Algorithm SHA256).Hash.ToLowerInvariant()

if ($actual -ne $expected) {
    throw "SHA-256 해시가 일치하지 않습니다."
}

"SHA-256 확인 완료: $file"
```

Portable ZIP을 확인하려면 `$file` 값만 다음과 같이 바꿉니다.

```powershell
$file = "codex-peek-v<version>-windows-x86_64-portable.zip"
```

해시가 일치하지 않으면 파일을 실행하지 말고 삭제한 뒤 공식 Release에서 다시
다운로드하세요.

## 6. 첫 실행과 설정

앱을 실행하면 작업 표시줄 위젯, 플로팅 위젯 또는 시스템 트레이 아이콘으로 상태가
표시됩니다. 작업 표시줄 연결에 실패해도 트레이와 플로팅 위젯은 계속 사용할 수 있습니다.

트레이 메뉴에서 다음 항목을 설정할 수 있습니다.

- 사용량 새로 고침 및 자동 갱신 간격
- 사용량 프로필 추가·선택·관리
- 모든 모니터 또는 주 모니터만 표시
- Windows 시작 시 실행
- 시작 화면과 언어
- 인증 갱신과 진단

기본값에서는 Windows 로캘이 지원 언어와 일치할 때 UI 언어를 자동으로 따릅니다.
언어는 트레이 메뉴에서 직접 선택할 수도 있습니다. 지원 언어는 한국어, 영어,
스페인어, 브라질 포르투갈어, 인도네시아어, 일본어, 힌디어, 독일어, 프랑스어,
베트남어, 터키어, 아랍어입니다.

Installer와 Portable은 다음 설정과 로그를 공유합니다.

```text
설정: %APPDATA%\CodexPeek\settings.json
로그: %TEMP%\codex-peek.log
```

UI를 열지 않고 연결 상태를 확인하려면 다음 명령을 실행합니다.

Installer:

```powershell
& "$env:LOCALAPPDATA\Programs\CodexUsageMonitor\codex-peek.exe" --diagnose
```

Portable:

```powershell
.\codex-peek.exe --diagnose
```

### 여러 계정의 사용량 프로필 설정

기존 로그인은 삭제할 수 없는 **기본 Codex 계정** 시스템 프로필로 표시됩니다. 이 프로필은
CodexPeek 시작 시 상속한 `CODEX_HOME`을 사용하고, 환경 변수가 없으면 Codex CLI 기본 홈을
사용합니다. 추가하는 관리 프로필은 각각 다음 위치 아래에 분리된 Codex 홈을 사용합니다.

```text
%APPDATA%\CodexPeek\profiles\profile-<number>\codex-home
```

시스템 프로필을 포함해 전체 8개까지 사용할 수 있습니다. 추가와 선택 방법은 다음과 같습니다.

1. 트레이 메뉴에서 **사용량 프로필 → 계정 추가…**를 선택합니다.
2. 1~40자의 구분하기 쉬운 표시명을 입력합니다. CodexPeek은 이메일이나 계정 ID를 읽지 않으므로
   표시명과 실제 계정의 대응은 사용자가 관리해야 합니다.
3. 안내를 확인한 뒤 브라우저가 열리면 사용할 ChatGPT 계정이 맞는지 직접 확인하고 로그인합니다.
   브라우저에 이미 로그인된 계정이 의도한 계정과 다를 수 있습니다.
4. 로그인이 완료되면 새 프로필의 사용량을 조회해 표시합니다. 취소·오프라인·실패 시 프로필은
   **로그인 필요** 상태로 남으므로 프로필 관리에서 다시 로그인하거나 명시적으로 삭제할 수 있습니다.
5. 이후 **사용량 프로필** 메뉴에서 표시할 프로필을 직접 선택합니다.

프로필 선택은 CodexPeek의 사용량 조회와 위젯 표시만 바꿉니다. 터미널, IDE, Codex 앱, WSL,
Remote SSH, Dev Containers 또는 이후 시작하는 Codex CLI의 로그인은 변경하지 않습니다.
CodexPeek은 한도에 따라 프로필을 자동 선택·순환하지 않으며 Codex 작업을 프로필로 라우팅하지
않습니다.

이름 변경은 표시명만 바꾸고, 로그아웃과 다시 로그인은 해당 관리 프로필에만 적용됩니다. 삭제할
때는 로컬 프로필 데이터와 그 안에 별도로 저장된 CLI 인증 정보를 복구할 수 없다는 확인 안내를
읽으세요. 삭제에 실패하면 프로필과 원래 디렉터리를 보존해 다시 시도할 수 있으며, 완료된 삭제의
내부 정리 항목은 다음 시작 시 안전하게 복구·정리됩니다. 기본 Codex 계정 프로필은 이름을 바꾸거나
삭제할 수 없습니다.

CodexPeek은 시스템 또는 관리 프로필의 `auth.json`을 읽거나 파싱하거나 복사하지 않습니다. 관리
프로필에 대응하는 자식 `codex app-server` 프로세스에만 해당 `CODEX_HOME`과 파일 인증 저장소
설정을 적용합니다. Windows 사용자·시스템 환경, 기본 Codex 홈, 터미널 및 IDE 설정은 수정하지
않습니다. `--diagnose`와 진단 로그는 프로필별 이름·경로·계정 정보 대신 설정 및 결과 개수만
집계합니다.

파일별 저장 내용, 기존 경로 마이그레이션, 삭제 복구 순서와 파일 저장소의 보안 한계는
[계정 및 인증 정보 저장 구조](ACCOUNT_STORAGE.md)를 참고하세요.

## 7. 업데이트

앱은 새 릴리스 메타데이터를 확인하고 사용자가 선택한 경우 GitHub Release 페이지를
브라우저로 엽니다. 업데이트 파일을 자동으로 다운로드하거나 실행 파일을 교체하지 않습니다.

### Installer 업데이트

1. 트레이 메뉴에서 앱을 종료합니다.
2. 새 버전의 설치 프로그램과 `SHA256SUMS.txt`를 다운로드합니다.
3. SHA-256을 확인합니다.
4. 새 설치 프로그램을 실행해 기존 위치에 설치합니다.

### Portable 업데이트

1. 트레이 메뉴에서 앱을 종료합니다.
2. 새 Portable ZIP과 `SHA256SUMS.txt`를 다운로드하고 검증합니다.
3. 새 폴더에 압축을 풀거나 기존 앱 파일을 새 파일로 교체합니다.
4. `codex-peek.exe`를 다시 실행합니다.

두 방식 모두 `%APPDATA%\CodexPeek`의 설정을 사용합니다. 이전 버전의
`%APPDATA%\CodexUsageMonitor` 데이터는 새 경로가 아직 없을 때 시작 과정에서 디렉터리
전체를 새 경로로 이동합니다.

## 8. 제거

Installer 버전은 Windows **설정 → 앱 → 설치된 앱**에서 **Codex Usage Monitor**를 찾아
제거합니다.

제거되는 항목:

- 설치된 실행 파일과 포함 문서
- 시작 메뉴 바로 가기
- 앱 및 기능의 제거 항목
- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexUsageMonitor` 자동 시작 값

보존되는 항목:

- `%APPDATA%\CodexPeek`의 사용자 설정
- `%TEMP%\codex-peek.log` 진단 로그

Portable 버전은 앱을 종료한 뒤 압축을 푼 폴더를 삭제하면 됩니다. 사용자 설정과 로그까지
완전히 제거하려면 위 보존 경로를 별도로 확인한 뒤 직접 삭제해야 합니다.

## 9. 문제 해결

| 증상 | 확인할 내용 |
| --- | --- |
| `codex` 명령을 찾을 수 없음 | `codex --version`과 `where.exe codex`를 실행하고 Codex CLI가 `PATH`에 있는지 확인하세요. |
| 로그인되지 않음 또는 인증 만료 | `codex login status`를 확인하고 필요하면 `codex login`을 실행한 뒤 트레이 메뉴에서 **인증 갱신**을 선택하세요. |
| SmartScreen 경고 | 공식 Release 출처와 SHA-256을 먼저 확인하세요. 확인할 수 없다면 실행하지 마세요. |
| 설치 프로그램이 앱 종료를 요청함 | 실행 중인 트레이 앱을 정상 종료한 뒤 설치 또는 업데이트를 다시 시작하세요. |
| 작업 표시줄 위젯이 보이지 않음 | 트레이 또는 플로팅 위젯을 사용하고, Explorer 재시작 후 원하는 모니터 모드를 다시 선택하세요. |
| 자세한 진단이 필요함 | 위의 `--diagnose` 명령 또는 트레이 메뉴의 **진단**을 사용하세요. |
| 제거 후 설정이 남아 있음 | 설정과 로그를 보존하는 것이 의도된 동작입니다. 완전 제거가 필요하면 보존 경로를 직접 확인하세요. |

문제가 계속되면 토큰, `auth.json`, 계정 정보 또는 민감한 로그를 공개 이슈에 첨부하지
마세요. 데이터 처리와 신고 방법은 [보안 정책](../SECURITY.md)을 참고하세요.
