use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::SettingsStore;

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
        Self::for_path(std::env::temp_dir().join("codex-usage-monitor.log"))
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
    use super::sanitize;

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
