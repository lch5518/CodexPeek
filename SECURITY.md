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
  needed for display. Settings are stored under `%APPDATA%\CodexUsageMonitor`; a bounded,
  rotating diagnostic log is stored at `%TEMP%\codex-peek.log`.
- The program launches `codex app-server --stdio` hidden and exchanges bounded JSONL
  messages over local pipes. The child is assigned to a Windows Job Object so the child
  process tree is terminated on timeout or monitor shutdown. It never invokes
  `codex exec` and does not start a user task.

## Usage-profile isolation / 사용량 프로필 격리

- The non-removable system profile preserves the Codex home inherited by CodexPeek at
  startup, or the Codex CLI default when `CODEX_HOME` is absent. Managed profiles use
  application-owned Codex homes below `%APPDATA%\CodexUsageMonitor\profiles`; arbitrary
  external paths are not accepted. At most eight profiles, including the system profile,
  are allowed.
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

- 삭제할 수 없는 시스템 프로필은 CodexPeek 시작 시 상속한 Codex 홈을 유지하며,
  `CODEX_HOME`이 없으면 Codex CLI 기본값을 사용합니다. 관리 프로필은
  `%APPDATA%\CodexUsageMonitor\profiles` 아래의 앱 전용 Codex 홈만 사용하고 임의의 외부 경로는
  받지 않습니다. 시스템 프로필을 포함한 전체 한도는 8개입니다.
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

## Network and updates / 네트워크 및 업데이트

Codex account and usage access is delegated to the installed Codex CLI. The monitor does
not send raw OpenAI HTTP requests itself; the CLI may contact OpenAI services according
to the CLI's own authentication, configuration, and network policy.

Official builds check release metadata at most once per day through
`https://api.github.com/repos/lch5518/CodexPeek/releases/latest`. The request enforces
HTTPS and response size/time limits. The monitor can open only an exact validated
`https://github.com/lch5518/CodexPeek/releases/tag/<tag>` page, and only after an explicit
user action. It never downloads, replaces, or executes an update. Proxy diagnostics report
presence only; they never log proxy URLs, credentials, or environment-variable values.

공식 빌드는 하루에 한 번 이하로 위 GitHub API에서 릴리스 메타데이터만 확인합니다.
HTTPS와 응답 크기·시간 제한을 적용하며, 사용자가 명시적으로 선택한 경우에만 검증된
정확한 GitHub 릴리스 페이지를 브라우저로 엽니다. 업데이트 파일을 다운로드·교체·실행하지
않습니다.

## Distribution integrity / 배포 파일 무결성

Initial Windows release files are not code-signed and may trigger Microsoft Defender
SmartScreen. Official GitHub Releases include `SHA256SUMS.txt` for the portable ZIP and
installer. Verify the SHA-256 hash after downloading; published assets are never silently
replaced. A correction is issued as a new patch version.

초기 Windows 릴리스 파일은 코드 서명되지 않아 Microsoft Defender SmartScreen 경고가
나타날 수 있습니다. 공식 GitHub Release에는 Portable ZIP과 설치 프로그램의 SHA-256
해시를 담은 `SHA256SUMS.txt`가 포함됩니다. 다운로드 후 해시를 확인하세요. 공개한 파일을
조용히 교체하지 않으며 수정은 새 패치 버전으로 배포합니다.
