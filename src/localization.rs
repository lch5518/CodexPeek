/// 사용자에게 표시할 문구의 언어를 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// 한국어 문구를 사용합니다.
    Korean,
    /// 영어 문구를 사용합니다.
    English,
}

/// 지원하는 모든 정적 지역화 문구의 식별자입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalizationKey {
    /// 자동 갱신 상태입니다.
    Polling,
    /// 수동 갱신 상태입니다.
    Refreshing,
    /// 오래된 사용량 상태입니다.
    Stale,
    /// 사용량을 불러오지 못한 상태입니다.
    Unavailable,
    /// 새로 고침 메뉴입니다.
    MenuRefresh,
    /// 설정 메뉴입니다.
    MenuSettings,
    /// 종료 메뉴입니다.
    MenuExit,
    /// 위젯 표시 메뉴입니다.
    MenuShowWidget,
    /// 위젯 숨김 메뉴입니다.
    MenuHideWidget,
    /// 진단 메뉴입니다.
    MenuDiagnostics,
    /// 업데이트 가능 알림입니다.
    UpdateAvailable,
    /// 최신 상태 알림입니다.
    UpdateCurrent,
    /// 기본 창 제목입니다.
    WindowTitle,
    /// 설정 창 제목입니다.
    SettingsTitle,
    /// 진단 창 제목입니다.
    DiagnosticsTitle,
    /// 인증 오류 진단입니다.
    DiagnosticLogin,
    /// RPC 오류 진단입니다.
    DiagnosticRpc,
}

impl LocalizationKey {
    /// 모든 문구 키를 빠짐없이 반환합니다.
    pub const ALL: &'static [Self] = &[
        Self::Polling,
        Self::Refreshing,
        Self::Stale,
        Self::Unavailable,
        Self::MenuRefresh,
        Self::MenuSettings,
        Self::MenuExit,
        Self::MenuShowWidget,
        Self::MenuHideWidget,
        Self::MenuDiagnostics,
        Self::UpdateAvailable,
        Self::UpdateCurrent,
        Self::WindowTitle,
        Self::SettingsTitle,
        Self::DiagnosticsTitle,
        Self::DiagnosticLogin,
        Self::DiagnosticRpc,
    ];
}

/// 지정한 언어와 키에 해당하는 정적 사용자 문구를 반환합니다.
pub fn localized_text(key: LocalizationKey, language: Language) -> &'static str {
    match (key, language) {
        (LocalizationKey::Polling, Language::Korean) => "자동 갱신 중",
        (LocalizationKey::Polling, Language::English) => "Polling",
        (LocalizationKey::Refreshing, Language::Korean) => "새로 고치는 중",
        (LocalizationKey::Refreshing, Language::English) => "Refreshing",
        (LocalizationKey::Stale, Language::Korean) => "정보가 오래되었습니다",
        (LocalizationKey::Stale, Language::English) => "Usage data is stale",
        (LocalizationKey::Unavailable, Language::Korean) => "사용량 정보를 사용할 수 없습니다",
        (LocalizationKey::Unavailable, Language::English) => "Usage unavailable",
        (LocalizationKey::MenuRefresh, Language::Korean) => "새로 고침",
        (LocalizationKey::MenuRefresh, Language::English) => "Refresh",
        (LocalizationKey::MenuSettings, Language::Korean) => "설정",
        (LocalizationKey::MenuSettings, Language::English) => "Settings",
        (LocalizationKey::MenuExit, Language::Korean) => "종료",
        (LocalizationKey::MenuExit, Language::English) => "Exit",
        (LocalizationKey::MenuShowWidget, Language::Korean) => "위젯 표시",
        (LocalizationKey::MenuShowWidget, Language::English) => "Show widget",
        (LocalizationKey::MenuHideWidget, Language::Korean) => "위젯 숨기기",
        (LocalizationKey::MenuHideWidget, Language::English) => "Hide widget",
        (LocalizationKey::MenuDiagnostics, Language::Korean) => "진단",
        (LocalizationKey::MenuDiagnostics, Language::English) => "Diagnostics",
        (LocalizationKey::UpdateAvailable, Language::Korean) => "새 업데이트를 사용할 수 있습니다",
        (LocalizationKey::UpdateAvailable, Language::English) => "An update is available",
        (LocalizationKey::UpdateCurrent, Language::Korean) => "최신 버전입니다",
        (LocalizationKey::UpdateCurrent, Language::English) => "You are up to date",
        (LocalizationKey::WindowTitle, Language::Korean) => "Codex 사용량 모니터",
        (LocalizationKey::WindowTitle, Language::English) => "Codex Usage Monitor",
        (LocalizationKey::SettingsTitle, Language::Korean) => "Codex 사용량 모니터 설정",
        (LocalizationKey::SettingsTitle, Language::English) => "Codex Usage Monitor Settings",
        (LocalizationKey::DiagnosticsTitle, Language::Korean) => "Codex 사용량 모니터 진단",
        (LocalizationKey::DiagnosticsTitle, Language::English) => "Codex Usage Monitor Diagnostics",
        (LocalizationKey::DiagnosticLogin, Language::Korean) => "로그인 상태를 확인할 수 없습니다",
        (LocalizationKey::DiagnosticLogin, Language::English) => {
            "Login status could not be verified"
        }
        (LocalizationKey::DiagnosticRpc, Language::Korean) => "Codex 서비스 요청에 실패했습니다",
        (LocalizationKey::DiagnosticRpc, Language::English) => "Codex service request failed",
    }
}

#[cfg(test)]
mod tests {
    use super::{localized_text, Language, LocalizationKey};

    #[test]
    fn every_key_has_a_nonempty_translation() {
        for key in LocalizationKey::ALL {
            for language in [Language::Korean, Language::English] {
                assert!(!localized_text(*key, language).is_empty());
            }
        }
    }
}
