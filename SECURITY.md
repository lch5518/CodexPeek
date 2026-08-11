# Security policy / 보안 정책

## Supported versions / 지원 버전

Security fixes are provided for the latest published release only. Development builds
and older releases may receive fixes at the maintainers' discretion.

보안 수정은 최신 공개 릴리스에만 제공됩니다. 개발 빌드와 이전 릴리스의 수정 여부는
유지관리자가 별도로 판단합니다.

## Reporting a vulnerability / 취약점 신고

No dedicated security-reporting address has been designated yet. Do not place tokens,
`auth.json`, account details, logs containing private data, or exploit details in a
public issue. Use GitHub private vulnerability reporting if it is available for this
repository; otherwise contact the maintainer through an existing private channel.

전용 보안 신고 주소는 아직 지정되지 않았습니다. 토큰, `auth.json`, 계정 정보, 개인
정보가 포함된 로그 또는 악용 세부 정보를 공개 이슈에 올리지 마세요. 이 저장소에서
GitHub 비공개 취약점 신고를 사용할 수 있으면 해당 기능을 사용하고, 그렇지 않으면
유지관리자에게 기존 비공개 채널로 연락하세요.

## Data handling / 데이터 처리

- Raw RPC payloads are handled only transiently for bounded parsing. They are not retained,
  copied to durable storage, persisted, or logged; only the required typed fields are
  deserialized. Authentication tokens, account IDs, email addresses, authentication-file
  contents, and proxy values are not deserialized into application data, persisted, or logged.
- Diagnostics inspect only whether `%USERPROFILE%\.codex\auth.json` and proxy-related
  environment variables exist; their contents and values are not read into diagnostics.
- The UI consumes only the login kind and the primary/secondary rate-limit window fields
  needed for display. Settings are stored under `%APPDATA%\CodexPeek`; a bounded,
  rotating diagnostic log is stored at `%TEMP%\codex-peek.log`.
- When usage forecasting is enabled, a separate `%APPDATA%\CodexPeek\usage-history.json`
  file retains only successful observations: the internal profile ID, `Primary` or `Secondary`,
  usage percent, an optional reset timestamp, and the observation timestamp. It never retains
  email addresses, account IDs, display labels, profile roots, tokens, authentication-file
  contents, conversations or prompts, proxy settings, or raw RPC payloads. This history stays
  on the local machine; it is never uploaded or synchronized. Forecasts are estimates and do
  not guarantee or alter OpenAI's limit policy.
- Forecasting is enabled by default but can be disabled from the tray's **Usage forecasting**
  menu. **Clear usage forecast history** removes all samples, and deleting a managed profile
  removes that profile's samples. History is bounded to 30 days and 1,000 samples per
  profile/window; duplicate values and observations less than five minutes apart are skipped to
  reduce writes. A corrupt or unsupported history file is quarantined or reset, while usage
  display continues using the latest successful poll.
- The installer and Portable build preserve `%APPDATA%\CodexPeek` when uninstalled, so the
  history file can remain after the application is removed. Clear it from the tray before
  uninstalling or remove the file/folder manually for complete local cleanup.
- The program launches `codex app-server --stdio` hidden and exchanges bounded JSONL
  messages over local pipes. The child is assigned to a Windows Job Object so the child
  process tree is terminated on timeout or monitor shutdown. It never invokes
  `codex exec` and does not start a user task.

## Usage-profile isolation / 사용량 프로필 격리

- The non-removable system profile preserves the Codex home inherited by CodexPeek at
  startup, or the Codex CLI default when `CODEX_HOME` is absent. Managed profiles use
  application-owned Codex homes below `%APPDATA%\CodexPeek\profiles`; arbitrary
  external paths are not accepted. At most eight profiles, including the system profile,
  are allowed. Its user-provided display label may be renamed, but it cannot be logged out
  or deleted; the label is not an account identity and does not alter `CODEX_HOME` or
  authentication files.
- Only the `codex app-server` child launched for a managed profile receives that profile's
  `CODEX_HOME` and the `cli_auth_credentials_store="file"` configuration override. The
  system profile does not receive the override. CodexPeek does not change its own process
  environment, Windows user or system environment, `PATH`, the default Codex home,
  terminal or IDE settings, or Codex CLI sign-in.
- CodexPeek never opens, reads, parses, imports, exports, or copies any system or managed
  profile `auth.json`. Authentication files are created and consumed only by the Codex CLI
  inside the selected child-process context. Profile labels are user-provided because the
  monitor does not inspect account email addresses or IDs.
- Managed-profile creation and deletion derive the exact path from a validated internal
  profile ID and the application-owned root. Path separators, traversal, arbitrary
  absolute paths, and reparse points are rejected. Deletion quiesces profile work, moves
  only the exact validated directory to an internal tombstone, then saves settings. A save
  failure rolls the directory back; a post-save cleanup failure leaves a validated
  tombstone for recovery on the next startup.
- Deleting a managed profile permanently removes its local profile data, including CLI
  credentials stored inside that isolated home. The UI requires explicit confirmation.
  Selection and deletion never alter terminal, IDE, Codex app, WSL, Remote SSH, or Dev
  Container sign-in, and profiles are never selected or rotated automatically.
- Diagnostics expose only bounded aggregate counts such as configured, healthy,
  login-required, and request-failed profiles. They do not record labels, internal IDs,
  managed paths, account details, authentication-file contents, or raw RPC payloads.

The complete storage layout, migration policy, and at-rest limitations are documented in
[Account and credential storage](docs/ACCOUNT_STORAGE.md).

- 삭제할 수 없는 시스템 프로필은 CodexPeek 시작 시 상속한 Codex 홈을 유지하며,
  `CODEX_HOME`이 없으면 Codex CLI 기본값을 사용합니다. 관리 프로필은
  `%APPDATA%\CodexPeek\profiles` 아래의 앱 전용 Codex 홈만 사용하고 임의의 외부 경로는
  받지 않습니다. 시스템 프로필을 포함한 전체 한도는 8개입니다. 사용자가 지정한 표시명은 바꿀 수
  있지만 로그아웃하거나 삭제할 수 없으며, 계정 식별자가 아니고 `CODEX_HOME`이나 인증 파일을
  바꾸지 않습니다.
- 관리 프로필의 자식 `codex app-server`에만 해당 `CODEX_HOME`과
  `cli_auth_credentials_store="file"` 설정 오버라이드를 적용합니다. 시스템 프로필에는 적용하지
  않습니다. CodexPeek 프로세스와 Windows 사용자·시스템 환경, `PATH`, 기본 Codex 홈, 터미널·IDE
  설정, Codex CLI 로그인은 변경하지 않습니다.
- CodexPeek은 시스템 또는 관리 프로필의 `auth.json`을 열거나 읽거나 파싱하거나 가져오기·내보내기·
  복사하지 않습니다. 인증 파일은 선택한 자식 프로세스 문맥 안에서 Codex CLI만 생성하고 사용합니다.
  계정 이메일이나 ID를 확인하지 않으므로 프로필 표시명은 사용자가 직접 지정합니다.
- 관리 프로필 생성·삭제 경로는 검증된 내부 ID와 앱 전용 루트에서만 계산합니다. 경로 구분자, 상위
  경로 이동, 임의 절대 경로, reparse point를 거절합니다. 삭제는 작업을 중단한 뒤 정확히 검증된
  디렉터리만 내부 tombstone으로 이동하고 설정을 저장합니다. 저장 실패 시 원래 위치로 되돌리며,
  저장 후 정리만 실패하면 검증된 tombstone을 다음 시작 때 복구 정리합니다.
- 삭제하면 격리된 홈 안의 CLI 인증 정보를 포함한 로컬 프로필 데이터를 복구할 수 없으므로 UI에서
  명시적으로 확인합니다. 선택·삭제는 터미널, IDE, Codex 앱, WSL, Remote SSH, Dev Containers의
  로그인을 바꾸지 않으며 프로필을 자동 선택·순환하지 않습니다.
- 진단은 설정됨·정상·로그인 필요·요청 실패 같은 제한된 집계 개수만 노출합니다. 표시명, 내부 ID,
  관리 경로, 계정 정보, 인증 파일 내용, 원본 RPC payload는 기록하지 않습니다.

### 로컬 사용량 소진 예측 기록

- 사용량 소진 예측을 켜면 별도 파일인 `%APPDATA%\CodexPeek\usage-history.json`에 성공한
  조회의 최소 정보만 저장합니다. 내부 프로필 ID, `Primary` 또는 `Secondary` 창 종류,
  사용률, 선택적인 초기화 시각, 성공한 조회 시각만 포함합니다. 이메일, 계정 ID, 사용자 지정
  프로필 이름, 프로필 루트 경로, 토큰, 인증 파일 내용, 대화·프롬프트 내용, 프록시 설정값,
  원본 RPC payload는 저장하지 않습니다. 이 기록은 로컬에만 있고 업로드하거나 동기화하지
  않습니다. 예측은 추정치이며 OpenAI 한도 정책을 보장하거나 바꾸지 않습니다.
- 예측은 기본으로 켜져 있지만 트레이의 **사용량 소진 예측** 메뉴에서 끌 수 있습니다.
  **사용량 소진 예측 기록 삭제**는 모든 표본을 삭제하며, 관리 프로필을 삭제하면 해당
  프로필의 표본도 함께 삭제합니다. 보존 기간은 최대 30일, 프로필·창별 최대 1,000개이며,
  같은 값과 5분보다 짧은 간격의 조회는 디스크 쓰기를 줄이기 위해 생략합니다. 기록 파일이
  손상되거나 지원하지 않는 버전이면 격리하거나 초기화하지만 사용량 표시는 계속됩니다.
- Installer와 Portable 제거 과정은 `%APPDATA%\CodexPeek`를 보존하므로 앱을 제거한 뒤에도
  기록 파일이 남을 수 있습니다. 완전히 지우려면 제거 전에 트레이에서 기록을 삭제하거나,
  제거 후 해당 파일·폴더를 직접 삭제하세요.

## Network and updates / 네트워크 및 업데이트

Codex account and usage access is delegated to the installed Codex CLI. The monitor does
not send raw OpenAI HTTP requests itself; the CLI may contact OpenAI services according
to the CLI's own authentication, configuration, and network policy.

Official builds check release metadata at startup through
`https://api.github.com/repos/lch5518/CodexPeek/releases/latest`. The request enforces
HTTPS and response size/time limits. After startup, an available update is offered only after
the app is running. Skipping a version records that version locally and suppresses it until a
newer release appears. With explicit approval, the updater accepts only the expected raw Windows
x64 executable and `SHA256SUMS.txt` assets from the validated GitHub Release, checks the exact
manifest entry and SHA-256, stages the file, and uses a helper to replace only the running
executable before restarting. A failed download, verification, or replacement leaves the current
executable in place. Update state is stored with the other settings under `%APPDATA%\CodexPeek`;
the updater does not replace that directory. Proxy diagnostics report presence only; they never
log proxy URLs, credentials, or environment-variable values.

Only executables compiled by the official Release workflow with the embedded
`CODEX_PEEK_OFFICIAL_BUILD=1` marker enable in-app updates. Local and custom builds keep the
updater disabled and show a once-per-version warning because replacing them with an official
binary would omit locally compiled changes. This marker is a build-channel guard, not a
cryptographic signature or trust boundary.

공식 빌드는 시작할 때 위 GitHub API에서 릴리스 메타데이터를 확인합니다.
HTTPS와 응답 크기·시간 제한을 적용하며 앱이 실행된 뒤에만 새 버전을 안내합니다. 특정 버전을
건너뛰면 로컬 설정에 기록해 더 새 버전이 나올 때까지 다시 묻지 않습니다. 사용자가 명시적으로
동의한 경우에만 검증된 GitHub Release의 예상 Windows x64 원본 EXE와 `SHA256SUMS.txt`를 받고,
manifest의 정확한 항목과 SHA-256을 확인합니다. 검증한 파일은 임시 위치에 준비하고 별도 helper가
현재 실행 파일만 교체한 뒤 다시 시작합니다. 다운로드·검증·교체가 실패하면 기존 실행 파일을
유지합니다. 업데이트 상태는 `%APPDATA%\CodexPeek`의 기존 설정과 함께 저장하며 updater는 그
디렉터리를 교체하지 않습니다.

공식 Release workflow가 `CODEX_PEEK_OFFICIAL_BUILD=1` 표시를 포함해 빌드한 실행 파일에서만
인앱 업데이트를 활성화합니다. 로컬·커스텀 빌드는 updater를 비활성화하고, 공식 바이너리로
교체하면 직접 빌드한 변경이 포함되지 않는다는 경고를 앱 버전마다 한 번 표시합니다. 이 표시는
빌드 채널 구분용이며 암호학적 서명이나 신뢰 경계는 아닙니다.

## Distribution integrity / 배포 파일 무결성

Initial Windows release files are not code-signed and may trigger Microsoft Defender
SmartScreen. Official GitHub Releases include `SHA256SUMS.txt` for the Installer, Portable ZIP,
and raw self-update executable. SHA-256 verification detects a mismatch with that manifest but
does not provide the publisher identity guarantee of an Authenticode signature. Published assets
are never silently replaced; a correction is issued as a new patch version.

초기 Windows 릴리스 파일은 코드 서명되지 않아 Microsoft Defender SmartScreen 경고가
나타날 수 있습니다. 공식 GitHub Release에는 Installer, Portable ZIP과 자동 업데이트용 원본
EXE의 SHA-256 해시를 담은 `SHA256SUMS.txt`가 포함됩니다. SHA-256 검증은 manifest와 다른
파일을 탐지하지만 Authenticode 서명처럼 게시자 신원을 보장하지는 않습니다. 공개한 파일을
조용히 교체하지 않으며 수정은 새 패치 버전으로 배포합니다.
