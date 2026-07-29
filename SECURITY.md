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

## Network and updates / 네트워크 및 업데이트

Codex account and usage access is delegated to the installed Codex CLI. The monitor does
not send raw OpenAI HTTP requests itself; the CLI may contact OpenAI services according
to the CLI's own authentication, configuration, and network policy.

Official builds check release metadata at most once per day through
`https://api.github.com/repos/lch5518/CodexPeek/releases/latest`. The request enforces
HTTPS and response size/time limits. The monitor can open only an exact validated
`https://github.com/lch5518/CodexPeek/releases/tag/<tag>` page. A user-initiated check always
shows its result, and the page opens only after the user confirms the update prompt. It never
downloads, replaces, or executes an update. Proxy diagnostics report presence only; they never
log proxy URLs, credentials, or environment-variable values.

공식 빌드는 하루에 한 번 이하로 위 GitHub API에서 릴리스 메타데이터만 확인합니다.
HTTPS와 응답 크기·시간 제한을 적용합니다. 수동 확인 결과는 항상 대화상자로 알리고, 사용자가
업데이트 안내에서 열기를 다시 확인한 경우에만 검증된 정확한 GitHub 릴리스 페이지를 브라우저로
엽니다. 업데이트 파일을 다운로드·교체·실행하지 않습니다.

## Distribution integrity / 배포 파일 무결성

Initial Windows release files are not code-signed and may trigger Microsoft Defender
SmartScreen. Official GitHub Releases include `SHA256SUMS.txt` for the portable ZIP and
installer. Verify the SHA-256 hash after downloading; published assets are never silently
replaced. A correction is issued as a new patch version.

초기 Windows 릴리스 파일은 코드 서명되지 않아 Microsoft Defender SmartScreen 경고가
나타날 수 있습니다. 공식 GitHub Release에는 Portable ZIP과 설치 프로그램의 SHA-256
해시를 담은 `SHA256SUMS.txt`가 포함됩니다. 다운로드 후 해시를 확인하세요. 공개한 파일을
조용히 교체하지 않으며 수정은 새 패치 버전으로 배포합니다.
