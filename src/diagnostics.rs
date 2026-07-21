use std::{
    fs, io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;

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
}

/// 민감 정보를 제거한 로컬 진단 로그 기록기입니다.
#[derive(Clone, Debug)]
pub struct DiagnosticLogger {
    path: PathBuf,
}

impl DiagnosticLogger {
    /// 기본 임시 디렉터리의 진단 로그 기록기를 만듭니다.
    pub fn new() -> Self {
        Self::for_path(std::env::temp_dir().join("codex-usage-monitor.log"))
    }

    /// 지정 경로를 사용하는 진단 로그 기록기를 만듭니다.
    pub fn for_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 안정적인 코드와 통제된 설명을 한 줄로 기록합니다.
    pub fn record(&self, code: DiagnosticCode, description: &str) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let line = format!(
            "{} {} {}\n",
            unix_now(),
            code.as_str(),
            sanitize(description)
        );
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
        }
    }
}

impl Default for DiagnosticLogger {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize(value: &str) -> String {
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
