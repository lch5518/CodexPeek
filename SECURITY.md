# Security policy / 보안 정책

## Supported versions / 지원 버전

Security fixes are provided for the latest published release only. Development builds
and older releases may receive fixes at the maintainers' discretion.

보안 수정은 최신 공개 릴리스에만 제공됩니다. 개발 빌드와 이전 릴리스의 수정 여부는
유지관리자가 별도로 판단합니다.

## Reporting a vulnerability / 취약점 신고

No official security-reporting address has been designated yet. Do not place tokens,
`auth.json`, account details, logs containing private data, or exploit details in a
public issue. Until an official repository and private reporting channel are published,
contact the party that supplied the binary through an existing private channel. Once an
official GitHub repository exists, this section will be updated with its private
vulnerability-reporting procedure.

공식 보안 신고 주소는 아직 지정되지 않았습니다. 토큰, `auth.json`, 계정 정보, 개인
정보가 포함된 로그 또는 악용 세부 정보를 공개 이슈에 올리지 마세요. 공식 저장소와
비공개 신고 채널이 공개되기 전에는 실행 파일을 제공한 주체에게 기존 비공개 채널로
연락하세요. 공식 GitHub 저장소가 정해지면 비공개 취약점 신고 절차를 이 문서에
추가합니다.

## Data handling / 데이터 처리

- Raw RPC payloads are handled only transiently for bounded parsing. They are not retained,
  copied to durable storage, persisted, or logged; only the required typed fields are
  deserialized. Authentication tokens, account IDs, email addresses, authentication-file
  contents, and proxy values are not deserialized into application data, persisted, or logged.
- Diagnostics inspect only whether `%USERPROFILE%\.codex\auth.json` and proxy-related
  environment variables exist; their contents and values are not read into diagnostics.
- The UI consumes only the login kind and the primary/secondary rate-limit window fields
  needed for display. Settings are stored under `%APPDATA%\CodexUsageMonitor`; a bounded,
  rotating diagnostic log is stored at `%TEMP%\codex-usage-monitor.log`.
- The program launches `codex app-server --stdio` hidden and exchanges bounded JSONL
  messages over local pipes. The child is assigned to a Windows Job Object so the child
  process tree is terminated on timeout or monitor shutdown. It never invokes
  `codex exec` and does not start a user task.

## Network and updates / 네트워크 및 업데이트

Codex account and usage access is delegated to the installed Codex CLI. The monitor does
not send raw OpenAI HTTP requests itself; the CLI may contact OpenAI services according
to the CLI's own authentication, configuration, and network policy.

Update checking is disabled in official builds until an HTTPS GitHub `repository` value
is added to Cargo package metadata. If enabled later, the monitor requests only
`https://api.github.com/repos/<owner>/<repo>/releases/latest`, enforces HTTPS and response
size/time limits, and can open only the exact validated
`https://github.com/<owner>/<repo>/releases/tag/<tag>` page in the default browser. It
does so only after an explicit user menu action and never from an automatic update worker.
It does not download, replace, or execute an update. Proxy diagnostics report presence only;
they never log proxy URLs, credentials, or environment-variable values.
