use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, OnceLock, Weak},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    codex::{OperationCancellation, ProfileAccountProvider},
    PollSnapshot, ProfileExecutionContext, Settings, SettingsStore, UsageError, UsageProfileId,
    UsageProfileRoot,
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;
static LOGGER_GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

/// 기록 가능한 안정적인 진단 코드입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// Codex CLI 확인 실패입니다.
    CliUnavailable,
    /// RPC 요청 실패입니다.
    RpcFailed,
    /// 로그인 상태 문제입니다.
    LoginUnavailable,
    /// 설정 파일 문제입니다.
    SettingsInvalid,
    /// 프록시 존재 여부 확인 결과입니다.
    ProxyPresence,
    /// 작업 표시줄 호환성 확인 결과입니다.
    TaskbarCheck,
    /// 작업 표시줄 창 합성 단계의 결과입니다.
    TaskbarRender,
    /// 사용량 프로필의 집계 상태입니다.
    Profiles,
}

impl DiagnosticCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::CliUnavailable => "cli_unavailable",
            Self::RpcFailed => "rpc_failed",
            Self::LoginUnavailable => "login_unavailable",
            Self::SettingsInvalid => "settings_invalid",
            Self::ProxyPresence => "proxy_presence",
            Self::TaskbarCheck => "taskbar_check",
            Self::TaskbarRender => "taskbar_render",
            Self::Profiles => "profiles",
        }
    }
}

/// 민감하지 않은 진단 이벤트 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SafeDiagnostic {
    /// CLI 파일 경로의 존재 여부입니다.
    Cli { path: PathBuf, exists: bool },
    /// RPC의 안정적인 오류 코드입니다.
    Rpc { code: DiagnosticCode },
    /// 인증 파일 경로와 존재 여부입니다.
    Login { auth_path: PathBuf, exists: bool },
    /// 설정 파일 처리 결과입니다.
    Settings { valid: bool },
    /// 프록시가 구성되었는지만 나타냅니다.
    Proxy { present: bool },
    /// 작업 표시줄 점검의 성공 여부입니다.
    Taskbar { available: bool },
    /// 작업 표시줄 합성 단계와 민감정보가 없는 운영체제 오류 코드입니다.
    TaskbarRender {
        stage: &'static str,
        error_code: Option<i32>,
    },
    /// 설정 유효성과 프로필 상태별 개수만 포함하는 집계입니다.
    ///
    /// 각 값은 기록 시 지원 최대치인 8로 제한되며 이름, 식별자, 경로 또는 계정 정보는 포함할
    /// 수 없습니다.
    Profiles {
        /// 설정 파일이 유효한지 나타냅니다.
        settings_valid: bool,
        /// 시스템 프로필을 포함해 구성된 프로필 수입니다.
        configured: u8,
        /// 마지막 조회가 정상인 프로필 수입니다.
        ok: u8,
        /// 로그인이 필요한 프로필 수입니다.
        login_required: u8,
        /// 안전하게 분류된 요청 실패 상태의 프로필 수입니다.
        request_failed: u8,
    },
}

/// 민감 정보를 제거한 로컬 진단 로그 기록기입니다.
#[derive(Clone, Debug)]
pub struct DiagnosticLogger {
    path: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl DiagnosticLogger {
    /// 기본 임시 디렉터리의 진단 로그 기록기를 만듭니다.
    pub fn new() -> Self {
        Self::for_path(std::env::temp_dir().join("codex-peek.log"))
    }

    /// 지정 경로를 사용하는 진단 로그 기록기를 만듭니다.
    ///
    /// `path`의 부모 디렉터리는 첫 기록 시 생성됩니다. 반환된 기록기는 동일 경로의 기록을 프로세스 내에서
    /// 직렬화하고, 1 MiB를 넘기기 전에 `.log.1`로 한 번 회전합니다.
    pub fn for_path(path: impl Into<PathBuf>) -> Self {
        let path = normalized_path(path.into());
        Self {
            gate: shared_gate(&path),
            path,
        }
    }

    /// 안정적인 코드와 통제된 설명을 한 줄로 기록합니다.
    fn record(&self, code: DiagnosticCode, description: &str) -> io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut line = format!(
            "{} {} {}\n",
            unix_now(),
            code.as_str(),
            sanitize(description)
        );
        line.truncate(line.trim_end_matches('\n').len());
        truncate_at_char_boundary(&mut line, (MAX_LOG_BYTES.saturating_sub(1)) as usize);
        line.push('\n');
        let existing = fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing.saturating_add(line.len() as u64) > MAX_LOG_BYTES && self.path.exists() {
            let backup = self.path.with_extension("log.1");
            let _ = fs::remove_file(&backup);
            fs::rename(&self.path, backup)?;
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())
    }

    /// 안전 모델을 필요한 최소 정보로 기록합니다.
    ///
    /// `event`에서 허용한 경로·불리언·안정 코드만 기록하며 토큰, 계정 식별자, 이메일, 프록시 값과 RPC
    /// 원문은 기록하지 않습니다. 파일 I/O 실패는 호출자에게 반환합니다.
    pub fn record_safe(&self, event: SafeDiagnostic) -> io::Result<()> {
        match event {
            SafeDiagnostic::Cli { path, exists } => self.record(
                DiagnosticCode::CliUnavailable,
                &format!("path={} exists={exists}", path.display()),
            ),
            SafeDiagnostic::Rpc { code } => self.record(code, "request_failed"),
            SafeDiagnostic::Login { auth_path, exists } => self.record(
                DiagnosticCode::LoginUnavailable,
                &format!("auth_path={} exists={exists}", auth_path.display()),
            ),
            SafeDiagnostic::Settings { valid } => {
                self.record(DiagnosticCode::SettingsInvalid, &format!("valid={valid}"))
            }
            SafeDiagnostic::Proxy { present } => {
                self.record(DiagnosticCode::ProxyPresence, &format!("present={present}"))
            }
            SafeDiagnostic::Taskbar { available } => self.record(
                DiagnosticCode::TaskbarCheck,
                &format!("available={available}"),
            ),
            SafeDiagnostic::TaskbarRender { stage, error_code } => self.record(
                DiagnosticCode::TaskbarRender,
                &format!("stage={stage} error_code={error_code:?}"),
            ),
            SafeDiagnostic::Profiles {
                settings_valid,
                configured,
                ok,
                login_required,
                request_failed,
            } => self.record(
                DiagnosticCode::Profiles,
                &format!(
                    "settings_valid={settings_valid} configured={} ok={} login_required={} request_failed={}",
                    configured.min(8),
                    ok.min(8),
                    login_required.min(8),
                    request_failed.min(8),
                ),
            ),
        }
    }
}

fn normalized_path(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

fn shared_gate(path: &Path) -> Arc<Mutex<()>> {
    let gates = LOGGER_GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates.lock().unwrap_or_else(|error| error.into_inner());
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(path).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(Mutex::new(()));
    gates.insert(path.to_path_buf(), Arc::downgrade(&gate));
    gate
}

fn truncate_at_char_boundary(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

impl Default for DiagnosticLogger {
    fn default() -> Self {
        Self::new()
    }
}

trait DiagnosticEventSink: Send + 'static {
    fn record(&self, event: SafeDiagnostic) -> io::Result<()>;
}

impl DiagnosticEventSink for DiagnosticLogger {
    fn record(&self, event: SafeDiagnostic) -> io::Result<()> {
        self.record_safe(event)
    }
}

enum DiagnosticWriterCommand {
    Event(SafeDiagnostic),
    Stop,
}

/// UI 스레드의 진단 이벤트를 제한된 큐로 받아 파일 기록 worker에서 처리합니다.
///
/// `enqueue`는 파일 시스템 I/O나 대기를 수행하지 않습니다. 큐가 가득 찼거나 worker가 종료됐으면
/// 이벤트를 안전하게 버리고 `false`를 반환합니다. `stop`은 앞선 이벤트를 drain한 뒤 worker를
/// join하며 종료 경로에서만 호출해야 합니다.
pub struct AsyncDiagnosticWriter {
    commands: mpsc::SyncSender<DiagnosticWriterCommand>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl AsyncDiagnosticWriter {
    /// 지정한 로거와 큐 용량으로 비동기 진단 기록기를 시작합니다.
    ///
    /// `capacity`는 최소 1로 보정됩니다. 반환 뒤 모든 파일 생성·회전·추가는 전용 worker에서만
    /// 수행되며 UI 호출자는 `enqueue`의 큐 제출 결과만 확인합니다.
    pub fn start(logger: DiagnosticLogger, capacity: usize) -> Self {
        Self::start_with_sink(logger, capacity)
    }

    /// 진단 이벤트를 기다리지 않고 제한된 worker 큐에 제출합니다.
    ///
    /// 성공하면 `true`, 큐 포화 또는 worker 종료 시 `false`를 반환합니다. 실패를 기록하기 위해
    /// 재귀적으로 다른 진단 이벤트를 만들지 않으며 파일 시스템에 접근하지 않습니다.
    pub fn enqueue(&self, event: SafeDiagnostic) -> bool {
        self.commands
            .try_send(DiagnosticWriterCommand::Event(event))
            .is_ok()
    }

    /// 제출된 이벤트를 모두 처리하고 worker 종료를 기다립니다.
    ///
    /// 종료 명령 제출과 join은 대기할 수 있으므로 애플리케이션 shutdown에서만 호출합니다. worker의
    /// 첫 파일 I/O 오류 또는 panic을 안전한 `io::Error`로 반환합니다.
    pub fn stop(mut self) -> io::Result<()> {
        let _ = self.commands.send(DiagnosticWriterCommand::Stop);
        join_diagnostic_worker(self.worker.take())
    }

    fn start_with_sink(sink: impl DiagnosticEventSink, capacity: usize) -> Self {
        let (commands, receiver) = mpsc::sync_channel(capacity.max(1));
        let worker = thread::spawn(move || diagnostic_writer_loop(sink, receiver));
        Self {
            commands,
            worker: Some(worker),
        }
    }
}

impl Drop for AsyncDiagnosticWriter {
    fn drop(&mut self) {
        let _ = self.commands.try_send(DiagnosticWriterCommand::Stop);
        drop(self.worker.take());
    }
}

fn diagnostic_writer_loop(
    sink: impl DiagnosticEventSink,
    receiver: mpsc::Receiver<DiagnosticWriterCommand>,
) -> io::Result<()> {
    let mut first_error = None;
    while let Ok(command) = receiver.recv() {
        match command {
            DiagnosticWriterCommand::Event(event) if first_error.is_none() => {
                if let Err(error) = sink.record(event) {
                    first_error = Some(error);
                }
            }
            DiagnosticWriterCommand::Event(_) => {}
            DiagnosticWriterCommand::Stop => break,
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn join_diagnostic_worker(worker: Option<JoinHandle<io::Result<()>>>) -> io::Result<()> {
    match worker {
        Some(worker) => worker
            .join()
            .map_err(|_| io::Error::other("diagnostic writer worker panicked"))?,
        None => Ok(()),
    }
}

/// 검증된 모든 프로필 진단의 민감하지 않은 집계와 시스템 프로필 결과입니다.
///
/// 프로필 이름, 내부 식별자, 관리 경로와 사용량 payload는 보관하지 않습니다. 각 분류 합계는 항상
/// `configured`와 같으며 `system_result`에는 기존 진단 요약에 필요한 안전한 오류 종류만 남깁니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDiagnosticRun {
    /// 시스템 프로필을 포함한 설정 프로필 수입니다.
    pub configured: u8,
    /// 정상 조회된 프로필 수입니다.
    pub ok: u8,
    /// 로그인이 필요한 프로필 수입니다.
    pub login_required: u8,
    /// RPC 실패 또는 시작 경로 검증 실패 프로필 수입니다.
    pub request_failed: u8,
    /// 기존 CLI·app-server·로그인 요약에 사용할 시스템 프로필 결과입니다.
    pub system_result: Result<(), UsageError>,
}

impl ProfileDiagnosticRun {
    /// 설정 유효성과 집계 개수만 포함하는 안전 진단 이벤트를 반환합니다.
    pub fn safe_diagnostic(self, settings_valid: bool) -> SafeDiagnostic {
        SafeDiagnostic::Profiles {
            settings_valid,
            configured: self.configured,
            ok: self.ok,
            login_required: self.login_required,
            request_failed: self.request_failed,
        }
    }
}

/// 시작 복구·검증을 통과한 프로필 컨텍스트를 하나의 provider로 순차 진단합니다.
///
/// `configured`에는 검증에서 제외된 프로필도 포함합니다. 함수는 `contexts` 순서대로 동기 조회하며
/// 각 호출에 새 취소 토큰을 사용합니다. 컨텍스트에 없는 구성 프로필은 `request_failed`로 집계해
/// 결과 합계를 일치시키고, 어떤 이름·경로·계정 정보나 원본 RPC 응답도 반환하지 않습니다.
pub fn diagnose_profile_contexts(
    provider: &dyn ProfileAccountProvider,
    configured: u8,
    contexts: &[ProfileExecutionContext],
) -> ProfileDiagnosticRun {
    let checked = contexts.len().min(crate::MAX_USAGE_PROFILES) as u8;
    let configured = configured.min(crate::MAX_USAGE_PROFILES as u8).max(checked);
    let mut run = ProfileDiagnosticRun {
        configured,
        ok: 0,
        login_required: 0,
        request_failed: configured.saturating_sub(checked),
        system_result: Err(UsageError::RequestFailed),
    };

    for context in contexts.iter().take(usize::from(checked)) {
        let result = provider.fetch_profile(context, false, OperationCancellation::default());
        if context.id() == UsageProfileId::System {
            run.system_result = result.as_ref().map(|_| ()).map_err(|error| *error);
        }
        match result {
            Ok(_) => run.ok = run.ok.saturating_add(1),
            Err(UsageError::NotLoggedIn | UsageError::AuthenticationExpired) => {
                run.login_required = run.login_required.saturating_add(1);
            }
            Err(_) => run.request_failed = run.request_failed.saturating_add(1),
        }
    }
    run
}

/// 한 프로필의 비민감 폴링 상태를 집계 진단 입력으로 전달합니다.
///
/// 프로필 이름, 관리 경로와 계정 식별 정보는 저장하지 않습니다. `snapshot`은 이미 안전하게
/// 분류된 사용량/오류 상태만 포함하며 `login_required`는 런타임 로그인 작업 결과입니다.
#[derive(Clone, Debug)]
pub struct ProfileDiagnosticSnapshot {
    /// 설정 catalog와 폴링 상태를 연결하는 숫자 기반 식별자입니다.
    pub id: UsageProfileId,
    /// 해당 프로필의 현재 독립 폴링 복사본입니다.
    pub snapshot: Option<PollSnapshot>,
    /// 최근 로그인 취소 또는 인증 실패 상태입니다.
    pub login_required: bool,
}

/// 설정 catalog와 프로필별 안전 스냅샷을 aggregate-only 진단 이벤트로 변환합니다.
///
/// `settings`의 프로필 이름과 `root`에서 파생되는 관리 경로는 유효한 실행 컨텍스트인지 확인하는
/// 동안만 사용하고 반환값에 복사하지 않습니다. `profiles`에 없는 구성 프로필은 상태 개수에
/// 포함하지 않으며 모든 출력 개수는 기록 단계에서도 최대 8로 제한됩니다.
pub fn aggregate_profile_diagnostics(
    settings_valid: bool,
    settings: &Settings,
    root: &UsageProfileRoot,
    profiles: &[ProfileDiagnosticSnapshot],
) -> SafeDiagnostic {
    let ids = std::iter::once(UsageProfileId::System).chain(
        settings
            .usage_profiles
            .managed()
            .iter()
            .map(|profile| profile.id()),
    );
    let mut ok = 0_u8;
    let mut login_required = 0_u8;
    let mut request_failed = 0_u8;
    for id in ids {
        let context_valid = match id {
            UsageProfileId::System => true,
            UsageProfileId::Managed(_) => ProfileExecutionContext::managed(root, id).is_ok(),
        };
        if !context_valid {
            request_failed = request_failed.saturating_add(1);
            continue;
        }
        let Some(profile) = profiles.iter().find(|profile| profile.id == id) else {
            continue;
        };
        let authentication_failed = profile.login_required
            || profile.snapshot.as_ref().is_some_and(|snapshot| {
                matches!(
                    snapshot.last_error,
                    Some(UsageError::NotLoggedIn | UsageError::AuthenticationExpired)
                )
            });
        if authentication_failed {
            login_required = login_required.saturating_add(1);
        } else if profile
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.last_error.is_some())
        {
            request_failed = request_failed.saturating_add(1);
        } else if profile
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.usage.is_some())
        {
            ok = ok.saturating_add(1);
        }
    }

    SafeDiagnostic::Profiles {
        settings_valid,
        configured: (settings.usage_profiles.managed().len() + 1).min(8) as u8,
        ok,
        login_required,
        request_failed,
    }
}

/// 설정 파일을 변경하지 않고 유효성을 검사해 안전한 진단 로그를 남깁니다.
///
/// `store`의 설정 파일이 없으면 기본 설정으로 유효하다고 판단합니다. 유효 여부는 `logger`에
/// 가능한 범위에서 기록하고 파일 읽기 오류를 반환합니다. 손상 파일을 복구하거나 이동하지 않습니다.
pub fn inspect_settings_for_diagnostics(
    store: &SettingsStore,
    logger: &DiagnosticLogger,
) -> io::Result<bool> {
    let valid = store.inspect_validity()?;
    let _ = logger.record_safe(SafeDiagnostic::Settings { valid });
    Ok(valid)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if [
        "bearer",
        "authorization",
        "token",
        "secret",
        "credential",
        "account",
        "email",
        "proxy",
    ]
    .iter()
    .any(|key| lower.contains(key))
    {
        return "[redacted]".to_owned();
    }
    let one_line = value.replace(['\r', '\n'], " ");
    let mut hide_next = false;
    one_line
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let redact = hide_next
                || lower == "bearer"
                || lower.starts_with("bearer=")
                || lower.contains("token=")
                || lower.contains("secret=")
                || lower.contains("credential=")
                || lower.contains("account=")
                || lower.contains("email=")
                || lower.contains("proxy=")
                || lower.contains('@');
            hide_next = lower == "bearer";
            if redact {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{mpsc, Arc, Mutex},
        thread,
        time::Duration,
    };

    use super::{
        sanitize, AsyncDiagnosticWriter, DiagnosticEventSink, DiagnosticLogger, SafeDiagnostic,
    };

    struct ThreadRecordingSink {
        caller: thread::ThreadId,
        called: mpsc::Sender<thread::ThreadId>,
    }

    impl DiagnosticEventSink for ThreadRecordingSink {
        fn record(&self, _event: SafeDiagnostic) -> io::Result<()> {
            let worker = thread::current().id();
            assert_ne!(worker, self.caller);
            self.called.send(worker).unwrap();
            Ok(())
        }
    }

    #[derive(Clone)]
    struct CountingSink {
        events: Arc<Mutex<Vec<SafeDiagnostic>>>,
    }

    impl DiagnosticEventSink for CountingSink {
        fn record(&self, event: SafeDiagnostic) -> io::Result<()> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    #[test]
    fn default_logger_uses_codex_peek_log_name() {
        assert_eq!(
            DiagnosticLogger::new().path,
            std::env::temp_dir().join("codex-peek.log")
        );
    }

    #[test]
    fn async_enqueue_never_calls_the_sink_on_the_ui_thread() {
        let (called, observed) = mpsc::channel();
        let writer = AsyncDiagnosticWriter::start_with_sink(
            ThreadRecordingSink {
                caller: thread::current().id(),
                called,
            },
            4,
        );

        assert!(writer.enqueue(SafeDiagnostic::Settings { valid: false }));
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        writer.stop().unwrap();
    }

    #[test]
    fn async_shutdown_drains_events_and_joins_the_worker() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let writer = AsyncDiagnosticWriter::start_with_sink(
            CountingSink {
                events: Arc::clone(&events),
            },
            8,
        );
        assert!(writer.enqueue(SafeDiagnostic::Settings { valid: true }));
        assert!(writer.enqueue(SafeDiagnostic::Proxy { present: false }));
        assert!(writer.enqueue(SafeDiagnostic::Taskbar { available: true }));

        writer.stop().unwrap();

        assert_eq!(events.lock().unwrap().len(), 3);
    }

    #[test]
    fn sanitizer_removes_json_camel_snake_colon_and_spaced_secrets() {
        let value = sanitize(
            r#"{"accessToken":"secret","account_id":"abc","email":"a@b.com","proxy" : "http://proxy"} authorization: Bearer token refresh_token = xyz"#,
        );
        for secret in ["secret", "abc", "a@b.com", "http://proxy", "xyz"] {
            assert!(!value.contains(secret), "{value}");
        }
    }
}
