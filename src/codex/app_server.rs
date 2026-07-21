use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{CodexUsage, UsageError, UsageWindow, WindowKind};

use super::{
    locator::locate_cli,
    process::{ChildTransport, ProcessGuard},
};

const CLIENT_NAME: &str = "codex_usage_monitor";
const CLIENT_TITLE: &str = "Codex Usage Monitor";
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);

/// Codex 사용량을 가져오는 동기식 제공자입니다.
pub trait UsageProvider: Send + Sync {
    /// 현재 Codex 사용량을 조회하고, 실패하면 안전한 오류 종류만 반환합니다.
    fn fetch_usage(&self) -> Result<CodexUsage, UsageError>;
}

/// 로컬 Codex app-server RPC를 이용하는 사용량 제공자입니다.
#[derive(Clone, Debug)]
pub struct AppServerUsageProvider {
    allow_auth_refresh: bool,
}

impl AppServerUsageProvider {
    /// 인증 갱신 허용 여부를 지정하여 제공자를 생성합니다.
    pub const fn new(allow_auth_refresh: bool) -> Self {
        Self { allow_auth_refresh }
    }
}

impl Default for AppServerUsageProvider {
    fn default() -> Self {
        Self::new(true)
    }
}

impl UsageProvider for AppServerUsageProvider {
    fn fetch_usage(&self) -> Result<CodexUsage, UsageError> {
        let candidate = locate_cli()?;
        let mut guard = ProcessGuard::start(candidate)?;
        let transport = guard.take_transport()?;
        let allow_auth_refresh = self.allow_auth_refresh;
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let mut transport = ProcessJsonlTransport { transport };
            let _ = sender.send(run_jsonl_session(
                &mut transport,
                allow_auth_refresh,
                PROVIDER_TIMEOUT,
            ));
        });

        let result = match receiver.recv_timeout(PROVIDER_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                guard.terminate_tree();
                Err(UsageError::RpcTimeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(UsageError::RequestFailed),
        };
        let _ = worker.join();
        guard.shutdown();
        result
    }
}

trait JsonlTransport {
    fn write_line(&mut self, line: &str) -> Result<(), TransportError>;
    fn read_line(&mut self) -> Result<Option<String>, TransportError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportError {
    Failed,
    #[cfg(test)]
    Timeout,
}

struct ProcessJsonlTransport {
    transport: ChildTransport,
}

impl JsonlTransport for ProcessJsonlTransport {
    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        self.transport
            .write_line(line)
            .map_err(|_| TransportError::Failed)
    }

    fn read_line(&mut self) -> Result<Option<String>, TransportError> {
        self.transport
            .read_line()
            .map_err(|_| TransportError::Failed)
    }
}

#[derive(Serialize)]
struct Request<'a, P: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    method: &'a str,
    params: P,
}

#[derive(Deserialize)]
struct ResponseHeader {
    id: Option<u64>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i64,
}

#[derive(Deserialize)]
struct ResultEnvelope<T> {
    result: T,
}

#[derive(Deserialize)]
struct AccountResult {
    account: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct RateLimitsResult {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitDto,
}

#[derive(Deserialize)]
struct RateLimitDto {
    primary: Option<UsageWindowDto>,
    secondary: Option<UsageWindowDto>,
}

#[derive(Deserialize)]
struct UsageWindowDto {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<u64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

fn run_jsonl_session<T: JsonlTransport>(
    transport: &mut T,
    allow_auth_refresh: bool,
    timeout: Duration,
) -> Result<CodexUsage, UsageError> {
    let deadline = Instant::now() + timeout;
    let mut next_id = 1;
    let initialize = Request {
        id: Some(next_id),
        method: "initialize",
        params: InitializeParams {
            client_info: ClientInfo {
                name: CLIENT_NAME,
                title: CLIENT_TITLE,
                version: env!("CARGO_PKG_VERSION"),
            },
        },
    };
    send_request(transport, &initialize, deadline)?;
    receive_result::<_, serde::de::IgnoredAny>(transport, next_id, deadline)?;
    send_notification(transport, "initialized", EmptyParams {}, deadline)?;

    next_id += 1;
    if !read_account(transport, next_id, false, deadline)? {
        return Err(UsageError::NotLoggedIn);
    }

    next_id += 1;
    match read_rate_limits(transport, next_id, deadline) {
        Ok(usage) => Ok(usage),
        Err(UsageError::RequestFailed) if allow_auth_refresh => {
            next_id += 1;
            if !read_account(transport, next_id, true, deadline)? {
                return Err(UsageError::AuthenticationExpired);
            }
            next_id += 1;
            read_rate_limits(transport, next_id, deadline)
        }
        Err(error) => Err(error),
    }
}

#[derive(Serialize)]
struct ClientInfo<'a> {
    name: &'a str,
    title: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct InitializeParams<'a> {
    #[serde(rename = "clientInfo")]
    client_info: ClientInfo<'a>,
}

#[derive(Serialize)]
struct EmptyParams {}

#[derive(Serialize)]
struct AccountParams {
    #[serde(rename = "refreshToken")]
    refresh_token: bool,
}

fn read_account<T: JsonlTransport>(
    transport: &mut T,
    id: u64,
    refresh_token: bool,
    deadline: Instant,
) -> Result<bool, UsageError> {
    send_request(
        transport,
        &Request {
            id: Some(id),
            method: "account/read",
            params: AccountParams { refresh_token },
        },
        deadline,
    )?;
    Ok(receive_result::<_, AccountResult>(transport, id, deadline)?
        .account
        .is_some())
}

fn read_rate_limits<T: JsonlTransport>(
    transport: &mut T,
    id: u64,
    deadline: Instant,
) -> Result<CodexUsage, UsageError> {
    send_request(
        transport,
        &Request {
            id: Some(id),
            method: "account/rateLimits/read",
            params: EmptyParams {},
        },
        deadline,
    )?;
    let dto = receive_result::<_, RateLimitsResult>(transport, id, deadline)?.rate_limits;
    let primary = dto
        .primary
        .map(|window| into_usage_window(window, WindowKind::Primary))
        .transpose()?;
    let secondary = dto
        .secondary
        .map(|window| into_usage_window(window, WindowKind::Secondary))
        .transpose()?;
    if primary.is_none() && secondary.is_none() {
        return Err(UsageError::RateLimitUnavailable);
    }
    Ok(CodexUsage {
        primary,
        secondary,
        fetched_at: SystemTime::now(),
    })
}

fn into_usage_window(dto: UsageWindowDto, kind: WindowKind) -> Result<UsageWindow, UsageError> {
    let resets_at = dto
        .resets_at
        .filter(|seconds| *seconds >= 0)
        .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64)));
    UsageWindow::new(kind, dto.used_percent, dto.window_duration_mins, resets_at)
}

fn send_request<T: JsonlTransport, P: Serialize>(
    transport: &mut T,
    request: &Request<'_, P>,
    deadline: Instant,
) -> Result<(), UsageError> {
    check_deadline(deadline)?;
    let request = serde_json::to_string(request).map_err(|_| UsageError::RequestFailed)?;
    transport.write_line(&request).map_err(map_transport_error)
}

fn send_notification<T: JsonlTransport, P: Serialize>(
    transport: &mut T,
    method: &str,
    params: P,
    deadline: Instant,
) -> Result<(), UsageError> {
    send_request(
        transport,
        &Request {
            id: None,
            method,
            params,
        },
        deadline,
    )
}

fn receive_result<T: JsonlTransport, R: for<'de> Deserialize<'de>>(
    transport: &mut T,
    expected_id: u64,
    deadline: Instant,
) -> Result<R, UsageError> {
    loop {
        check_deadline(deadline)?;
        let Some(line) = transport.read_line().map_err(map_transport_error)? else {
            return Err(UsageError::InvalidResponse);
        };
        let header: ResponseHeader =
            serde_json::from_str(&line).map_err(|_| UsageError::InvalidResponse)?;
        if header.id != Some(expected_id) {
            continue;
        }
        if let Some(error) = header.error {
            return Err(map_rpc_error(error.code));
        }
        return serde_json::from_str::<ResultEnvelope<R>>(&line)
            .map(|response| response.result)
            .map_err(|_| UsageError::InvalidResponse);
    }
}

fn check_deadline(deadline: Instant) -> Result<(), UsageError> {
    if Instant::now() >= deadline {
        Err(UsageError::RpcTimeout)
    } else {
        Ok(())
    }
}

fn map_transport_error(error: TransportError) -> UsageError {
    match error {
        TransportError::Failed => UsageError::RequestFailed,
        #[cfg(test)]
        TransportError::Timeout => UsageError::RpcTimeout,
    }
}

fn map_rpc_error(code: i64) -> UsageError {
    match code {
        -32601 => UsageError::UnsupportedCli,
        -32001 => UsageError::RpcOverloaded,
        _ => UsageError::RequestFailed,
    }
}

#[cfg(test)]
struct ScriptedTransport {
    responses: VecDeque<Result<String, TransportError>>,
    requests: Vec<String>,
}

#[cfg(test)]
impl ScriptedTransport {
    fn new<const N: usize>(responses: [&str; N]) -> Self {
        Self {
            responses: responses
                .into_iter()
                .map(|line| Ok(line.to_owned()))
                .collect(),
            requests: Vec::new(),
        }
    }

    fn ready_and_logged_in(rate_limits: &str) -> Self {
        Self::new([
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt"}}}"#,
            &format!(r#"{{"jsonrpc":"2.0","id":3,"result":{{"rateLimits":{rate_limits}}}}}"#),
        ])
    }

    fn timeout() -> Self {
        Self {
            responses: [Err(TransportError::Timeout)].into_iter().collect(),
            requests: Vec::new(),
        }
    }

    fn requests(&self) -> &[String] {
        &self.requests
    }
}

#[cfg(test)]
impl JsonlTransport for ScriptedTransport {
    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        self.requests.push(line.to_owned());
        Ok(())
    }

    fn read_line(&mut self) -> Result<Option<String>, TransportError> {
        self.responses.pop_front().transpose()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{run_jsonl_session, ScriptedTransport};
    use crate::UsageError;

    #[test]
    fn session_ignores_sensitive_extra_fields_and_interleaved_notifications() {
        let mut transport = ScriptedTransport::new([
            r#"{"jsonrpc":"2.0","id":1,"result":{"serverInfo":{"name":"codex"},"accessToken":"never retain"}}"#,
            r#"{"jsonrpc":"2.0","method":"codex/event","params":{"refreshToken":"never retain"}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt","email":"never retain","id":"never retain"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"result":{"rateLimits":{"primary":{"usedPercent":125.5,"windowDurationMins":300,"resetsAt":1700000000,"accessToken":"never retain"},"secondary":{"usedPercent":25.0,"windowDurationMins":10080,"resetsAt":1700003600,"refreshToken":"never retain"}}}}"#,
        ]);

        let usage = run_jsonl_session(&mut transport, false, Duration::from_secs(1)).unwrap();

        assert_eq!(usage.primary.unwrap().used_percent, 125.5);
        assert_eq!(usage.secondary.unwrap().window_duration_mins, Some(10_080));
        assert!(
            usage
                .fetched_at
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                > 0
        );
        assert_eq!(
            transport.requests(),
            [
                r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_usage_monitor","title":"Codex Usage Monitor","version":"0.1.0"}}}"#,
                r#"{"method":"initialized","params":{}}"#,
                r#"{"id":2,"method":"account/read","params":{"refreshToken":false}}"#,
                r#"{"id":3,"method":"account/rateLimits/read","params":{}}"#,
            ]
        );
    }

    #[test]
    fn session_accepts_primary_or_secondary_alone_and_rejects_both_null() {
        let mut primary_only = ScriptedTransport::ready_and_logged_in(
            r#"{"primary":{"usedPercent":1.0,"windowDurationMins":60,"resetsAt":1},"secondary":null}"#,
        );
        let usage = run_jsonl_session(&mut primary_only, false, Duration::from_secs(1)).unwrap();
        assert!(usage.primary.is_some());
        assert!(usage.secondary.is_none());

        let mut secondary_only = ScriptedTransport::ready_and_logged_in(
            r#"{"primary":null,"secondary":{"usedPercent":2.0,"windowDurationMins":120,"resetsAt":2}}"#,
        );
        let usage = run_jsonl_session(&mut secondary_only, false, Duration::from_secs(1)).unwrap();
        assert!(usage.primary.is_none());
        assert!(usage.secondary.is_some());

        let mut neither =
            ScriptedTransport::ready_and_logged_in(r#"{"primary":null,"secondary":null}"#);
        assert_eq!(
            run_jsonl_session(&mut neither, false, Duration::from_secs(1)),
            Err(UsageError::RateLimitUnavailable)
        );
    }

    #[test]
    fn session_rejects_invalid_percent_but_preserves_window_on_invalid_timestamp() {
        let mut invalid_percent = ScriptedTransport::ready_and_logged_in(
            r#"{"primary":{"usedPercent":-1.0,"windowDurationMins":60,"resetsAt":1},"secondary":null}"#,
        );
        assert_eq!(
            run_jsonl_session(&mut invalid_percent, false, Duration::from_secs(1)),
            Err(UsageError::InvalidResponse)
        );

        let mut invalid_timestamp = ScriptedTransport::ready_and_logged_in(
            r#"{"primary":{"usedPercent":1.0,"windowDurationMins":60,"resetsAt":-1},"secondary":null}"#,
        );
        let usage =
            run_jsonl_session(&mut invalid_timestamp, false, Duration::from_secs(1)).unwrap();
        assert_eq!(usage.primary.unwrap().resets_at, None);
    }

    #[test]
    fn session_maps_malformed_json_eof_login_and_rpc_errors() {
        let mut malformed = ScriptedTransport::new(["not json"]);
        assert_eq!(
            run_jsonl_session(&mut malformed, false, Duration::from_secs(1)),
            Err(UsageError::InvalidResponse)
        );

        let mut eof = ScriptedTransport::new([] as [&str; 0]);
        assert_eq!(
            run_jsonl_session(&mut eof, false, Duration::from_secs(1)),
            Err(UsageError::InvalidResponse)
        );

        let mut missing_login = ScriptedTransport::new([
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"account":null}}"#,
        ]);
        assert_eq!(
            run_jsonl_session(&mut missing_login, false, Duration::from_secs(1)),
            Err(UsageError::NotLoggedIn)
        );

        let mut method_missing =
            ScriptedTransport::new([r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601}}"#]);
        assert_eq!(
            run_jsonl_session(&mut method_missing, false, Duration::from_secs(1)),
            Err(UsageError::UnsupportedCli)
        );

        let mut overloaded =
            ScriptedTransport::new([r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32001}}"#]);
        assert_eq!(
            run_jsonl_session(&mut overloaded, false, Duration::from_secs(1)),
            Err(UsageError::RpcOverloaded)
        );
    }

    #[test]
    fn session_maps_transport_deadline_to_timeout() {
        let mut transport = ScriptedTransport::timeout();

        assert_eq!(
            run_jsonl_session(&mut transport, false, Duration::from_secs(1)),
            Err(UsageError::RpcTimeout)
        );
    }

    #[test]
    fn rate_limit_method_errors_and_malformed_results_do_not_trigger_refresh() {
        let cases = [
            (
                r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601}}"#,
                UsageError::UnsupportedCli,
            ),
            (
                r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32001}}"#,
                UsageError::RpcOverloaded,
            ),
            ("not json", UsageError::InvalidResponse),
        ];
        for (rate_limit_response, expected) in cases {
            let mut transport = ScriptedTransport::new([
                r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
                r#"{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt"}}}"#,
                rate_limit_response,
            ]);

            assert_eq!(
                run_jsonl_session(&mut transport, true, Duration::from_secs(1)),
                Err(expected)
            );
            assert_eq!(
                transport.requests(),
                [
                    r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_usage_monitor","title":"Codex Usage Monitor","version":"0.1.0"}}}"#,
                    r#"{"method":"initialized","params":{}}"#,
                    r#"{"id":2,"method":"account/read","params":{"refreshToken":false}}"#,
                    r#"{"id":3,"method":"account/rateLimits/read","params":{}}"#,
                ]
            );
        }
    }

    #[test]
    fn session_forces_one_refresh_then_retries_rate_limits_once() {
        let mut transport = ScriptedTransport::new([
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32099}}"#,
            r#"{"jsonrpc":"2.0","id":4,"result":{"account":{"type":"chatgpt"}}}"#,
            r#"{"jsonrpc":"2.0","id":5,"result":{"rateLimits":{"primary":{"usedPercent":9.0,"windowDurationMins":60,"resetsAt":1},"secondary":null}}}"#,
        ]);

        let usage = run_jsonl_session(&mut transport, true, Duration::from_secs(1)).unwrap();

        assert_eq!(usage.primary.unwrap().used_percent, 9.0);
        assert_eq!(
            transport.requests(),
            [
                r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_usage_monitor","title":"Codex Usage Monitor","version":"0.1.0"}}}"#,
                r#"{"method":"initialized","params":{}}"#,
                r#"{"id":2,"method":"account/read","params":{"refreshToken":false}}"#,
                r#"{"id":3,"method":"account/rateLimits/read","params":{}}"#,
                r#"{"id":4,"method":"account/read","params":{"refreshToken":true}}"#,
                r#"{"id":5,"method":"account/rateLimits/read","params":{}}"#,
            ]
        );
    }

    #[test]
    fn missing_account_after_forced_refresh_is_authentication_expired() {
        let mut transport = ScriptedTransport::new([
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32099}}"#,
            r#"{"jsonrpc":"2.0","id":4,"result":{"account":null}}"#,
        ]);

        assert_eq!(
            run_jsonl_session(&mut transport, true, Duration::from_secs(1)),
            Err(UsageError::AuthenticationExpired)
        );
    }
}
