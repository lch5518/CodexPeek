use crate::localization::Language;

/// 사용량을 조회하거나 해석하는 과정에서 발생할 수 있는 오류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// Codex CLI 실행 파일을 찾지 못했습니다.
    CliNotFound,
    /// 설치된 Codex CLI가 지원 범위를 벗어났습니다.
    UnsupportedCli,
    /// Codex 앱 서버를 시작하지 못했습니다.
    AppServerStartFailed,
    /// RPC 응답 대기 시간이 만료되었습니다.
    RpcTimeout,
    /// RPC 서버가 요청을 처리할 수 없을 정도로 혼잡합니다.
    RpcOverloaded,
    /// Codex 로그인이 필요합니다.
    NotLoggedIn,
    /// Codex 인증 정보가 만료되었습니다.
    AuthenticationExpired,
    /// Codex 응답 형식이 유효하지 않습니다.
    InvalidResponse,
    /// 사용량 한도 정보를 확인할 수 없습니다.
    RateLimitUnavailable,
    /// 사용량 요청이 완료되지 못했습니다.
    RequestFailed,
}

impl UsageError {
    /// 오류를 식별하기 위한 안정적인 진단 코드를 반환합니다.
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::CliNotFound => "cli_not_found",
            Self::UnsupportedCli => "unsupported_cli",
            Self::AppServerStartFailed => "app_server_start_failed",
            Self::RpcTimeout => "rpc_timeout",
            Self::RpcOverloaded => "rpc_overloaded",
            Self::NotLoggedIn => "not_logged_in",
            Self::AuthenticationExpired => "authentication_expired",
            Self::InvalidResponse => "invalid_response",
            Self::RateLimitUnavailable => "rate_limit_unavailable",
            Self::RequestFailed => "request_failed",
        }
    }

    /// 지정한 언어로 민감한 정보를 포함하지 않는 사용자 안내 문구를 반환합니다.
    pub const fn user_message(self, language: Language) -> &'static str {
        match (self, language) {
            (Self::CliNotFound, Language::Korean) => "Codex CLI를 찾을 수 없습니다.",
            (Self::CliNotFound, Language::English) => "Codex CLI was not found.",
            (Self::UnsupportedCli, Language::Korean) => "지원하지 않는 Codex CLI 버전입니다.",
            (Self::UnsupportedCli, Language::English) => {
                "The installed Codex CLI version is unsupported."
            }
            (Self::AppServerStartFailed, Language::Korean) => "Codex 앱 서버를 시작할 수 없습니다.",
            (Self::AppServerStartFailed, Language::English) => "Codex app server could not start.",
            (Self::RpcTimeout, Language::Korean) => "Codex 응답 시간이 초과되었습니다.",
            (Self::RpcTimeout, Language::English) => "Codex did not respond in time.",
            (Self::RpcOverloaded, Language::Korean) => {
                "Codex 요청이 혼잡합니다. 잠시 후 다시 시도하세요."
            }
            (Self::RpcOverloaded, Language::English) => "Codex is busy. Please try again shortly.",
            (Self::NotLoggedIn, Language::Korean) => "Codex에 로그인되어 있지 않습니다.",
            (Self::NotLoggedIn, Language::English) => "You are not signed in to Codex.",
            (Self::AuthenticationExpired, Language::Korean) => "Codex 인증이 만료되었습니다.",
            (Self::AuthenticationExpired, Language::English) => "Codex authentication has expired.",
            (Self::InvalidResponse, Language::Korean) => "Codex 응답이 올바르지 않습니다.",
            (Self::InvalidResponse, Language::English) => "Codex returned an invalid response.",
            (Self::RateLimitUnavailable, Language::Korean) => {
                "사용량 한도 정보를 사용할 수 없습니다."
            }
            (Self::RateLimitUnavailable, Language::English) => {
                "Usage limit information is unavailable."
            }
            (Self::RequestFailed, Language::Korean) => "사용량 요청에 실패했습니다.",
            (Self::RequestFailed, Language::English) => "The usage request failed.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UsageError;
    use crate::Language;

    #[test]
    fn every_error_has_a_stable_code_and_complete_localized_messages() {
        let cases = [
            (UsageError::CliNotFound, "cli_not_found"),
            (UsageError::UnsupportedCli, "unsupported_cli"),
            (UsageError::AppServerStartFailed, "app_server_start_failed"),
            (UsageError::RpcTimeout, "rpc_timeout"),
            (UsageError::RpcOverloaded, "rpc_overloaded"),
            (UsageError::NotLoggedIn, "not_logged_in"),
            (UsageError::AuthenticationExpired, "authentication_expired"),
            (UsageError::InvalidResponse, "invalid_response"),
            (UsageError::RateLimitUnavailable, "rate_limit_unavailable"),
            (UsageError::RequestFailed, "request_failed"),
        ];

        for (error, expected_code) in cases {
            assert_eq!(error.diagnostic_code(), expected_code);
            assert!(!error.user_message(Language::Korean).is_empty());
            assert!(!error.user_message(Language::English).is_empty());
        }
    }
}
