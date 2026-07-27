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
    /// 즉시 새로 고침 메뉴입니다.
    MenuRefreshNow,
    /// 갱신 간격 메뉴입니다.
    MenuRefreshInterval,
    /// 자동 시작 메뉴입니다.
    MenuAutostart,
    /// 시작 화면 메뉴입니다.
    MenuStartupView,
    /// 시작 시 위젯 표시 옵션입니다.
    MenuStartupWidget,
    /// 시작 시 트레이 전용 옵션입니다.
    MenuStartupTrayOnly,
    /// 자동 인증 갱신 메뉴입니다.
    MenuAuthRefresh,
    /// Codex 로그인 메뉴입니다.
    MenuLogin,
    /// 인증을 즉시 갱신하는 메뉴입니다.
    MenuAuthRefreshNow,
    /// 언어 메뉴입니다.
    MenuLanguage,
    /// 진단 메뉴입니다.
    MenuDiagnostics,
    /// 업데이트 확인 메뉴입니다.
    MenuUpdateCheck,
    /// 설정 메뉴입니다.
    MenuSettings,
    /// 종료 메뉴입니다.
    MenuExit,
    /// 위젯 표시 메뉴입니다.
    MenuShowWidget,
    /// 위젯 숨김 메뉴입니다.
    MenuHideWidget,
    /// 시작 시 위젯 표시 선택 메뉴입니다.
    MenuStartupWidgetChoice,
    /// 시작 시 트레이 전용 선택 메뉴입니다.
    MenuStartupTrayOnlyChoice,
    /// 모든 모니터 작업표시줄 표시 메뉴입니다.
    MenuTaskbarAll,
    /// 주 모니터 작업표시줄 표시 메뉴입니다.
    MenuTaskbarPrimary,
    /// 업데이트 가능 알림입니다.
    UpdateAvailable,
    /// 최신 상태 알림입니다.
    UpdateCurrent,
    /// 업데이트 확인 진행 상태 알림입니다.
    UpdateChecking,
    /// 업데이트 확인 실패 상태 알림입니다.
    UpdateFailed,
    /// 기본 창 제목입니다.
    WindowTitle,
    /// 설정 창 제목입니다.
    SettingsTitle,
    /// 진단 창 제목입니다.
    DiagnosticsTitle,
    /// 주 사용량 창 레이블입니다.
    PrimaryWindowLabel,
    /// 보조 사용량 창 레이블입니다.
    SecondaryWindowLabel,
    /// CLI 진단 문구입니다.
    DiagnosticCli,
    /// RPC 진단 문구입니다.
    DiagnosticRpc,
    /// 로그인 진단 문구입니다.
    DiagnosticLogin,
    /// 설정 진단 문구입니다.
    DiagnosticSettings,
    /// 프록시 진단 문구입니다.
    DiagnosticProxy,
    /// 작업 표시줄 진단 문구입니다.
    DiagnosticTaskbar,
    /// 남은 사용량 표시 전환 메뉴입니다.
    MenuShowRemaining,
    /// 주간 사용량 표시 전환 메뉴입니다.
    MenuShowWeekly,
}

impl LocalizationKey {
    /// 모든 문구 키를 빠짐없이 반환합니다.
    pub const ALL: &'static [Self] = &[
        Self::Polling,
        Self::Refreshing,
        Self::Stale,
        Self::Unavailable,
        Self::MenuRefresh,
        Self::MenuRefreshNow,
        Self::MenuRefreshInterval,
        Self::MenuAutostart,
        Self::MenuStartupView,
        Self::MenuStartupWidget,
        Self::MenuStartupTrayOnly,
        Self::MenuAuthRefresh,
        Self::MenuLogin,
        Self::MenuAuthRefreshNow,
        Self::MenuLanguage,
        Self::MenuDiagnostics,
        Self::MenuUpdateCheck,
        Self::MenuSettings,
        Self::MenuExit,
        Self::MenuShowWidget,
        Self::MenuHideWidget,
        Self::MenuStartupWidgetChoice,
        Self::MenuStartupTrayOnlyChoice,
        Self::MenuTaskbarAll,
        Self::MenuTaskbarPrimary,
        Self::UpdateAvailable,
        Self::UpdateCurrent,
        Self::UpdateChecking,
        Self::UpdateFailed,
        Self::WindowTitle,
        Self::SettingsTitle,
        Self::DiagnosticsTitle,
        Self::PrimaryWindowLabel,
        Self::SecondaryWindowLabel,
        Self::DiagnosticCli,
        Self::DiagnosticRpc,
        Self::DiagnosticLogin,
        Self::DiagnosticSettings,
        Self::DiagnosticProxy,
        Self::DiagnosticTaskbar,
        Self::MenuShowRemaining,
        Self::MenuShowWeekly,
    ];
}

/// 지정한 언어와 키에 해당하는 정적 사용자 문구를 반환합니다.
///
/// 매개변수 `key`는 표시할 문구 식별자이고 `language`는 반환 언어입니다.
/// 반환값은 프로그램 전체에서 재사용 가능한 정적 문자열입니다.
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
        (LocalizationKey::MenuRefreshNow, Language::Korean) => "지금 갱신",
        (LocalizationKey::MenuRefreshNow, Language::English) => "Refresh now",
        (LocalizationKey::MenuRefreshInterval, Language::Korean) => "갱신 간격",
        (LocalizationKey::MenuRefreshInterval, Language::English) => "Refresh interval",
        (LocalizationKey::MenuAutostart, Language::Korean) => "Windows 시작 시 실행",
        (LocalizationKey::MenuAutostart, Language::English) => "Start with Windows",
        (LocalizationKey::MenuStartupView, Language::Korean) => "시작 화면",
        (LocalizationKey::MenuStartupView, Language::English) => "Startup view",
        (LocalizationKey::MenuStartupWidget, Language::Korean) => "위젯 표시",
        (LocalizationKey::MenuStartupWidget, Language::English) => "Show widget",
        (LocalizationKey::MenuStartupTrayOnly, Language::Korean) => "트레이에만 표시",
        (LocalizationKey::MenuStartupTrayOnly, Language::English) => "Tray only",
        (LocalizationKey::MenuAuthRefresh, Language::Korean) => "자동 인증 갱신",
        (LocalizationKey::MenuAuthRefresh, Language::English) => "Automatic authentication refresh",
        (LocalizationKey::MenuLogin, Language::Korean) => "Codex 로그인",
        (LocalizationKey::MenuLogin, Language::English) => "Sign in to Codex",
        (LocalizationKey::MenuAuthRefreshNow, Language::Korean) => "인증 갱신",
        (LocalizationKey::MenuAuthRefreshNow, Language::English) => "Refresh authentication",
        (LocalizationKey::MenuLanguage, Language::Korean) => "언어",
        (LocalizationKey::MenuLanguage, Language::English) => "Language",
        (LocalizationKey::MenuDiagnostics, Language::Korean) => "진단",
        (LocalizationKey::MenuDiagnostics, Language::English) => "Diagnostics",
        (LocalizationKey::MenuUpdateCheck, Language::Korean) => "업데이트 확인",
        (LocalizationKey::MenuUpdateCheck, Language::English) => "Check for updates",
        (LocalizationKey::MenuSettings, Language::Korean) => "설정",
        (LocalizationKey::MenuSettings, Language::English) => "Settings",
        (LocalizationKey::MenuExit, Language::Korean) => "종료",
        (LocalizationKey::MenuExit, Language::English) => "Exit",
        (LocalizationKey::MenuShowWidget, Language::Korean) => "위젯 표시",
        (LocalizationKey::MenuShowWidget, Language::English) => "Show widget",
        (LocalizationKey::MenuHideWidget, Language::Korean) => "위젯 숨기기",
        (LocalizationKey::MenuHideWidget, Language::English) => "Hide widget",
        (LocalizationKey::MenuStartupWidgetChoice, Language::Korean) => "시작: 위젯 표시",
        (LocalizationKey::MenuStartupWidgetChoice, Language::English) => "Startup: show widget",
        (LocalizationKey::MenuStartupTrayOnlyChoice, Language::Korean) => "시작: 트레이만",
        (LocalizationKey::MenuStartupTrayOnlyChoice, Language::English) => "Startup: tray only",
        (LocalizationKey::MenuTaskbarAll, Language::Korean) => "위젯: 모든 모니터",
        (LocalizationKey::MenuTaskbarAll, Language::English) => "Widget: all monitors",
        (LocalizationKey::MenuTaskbarPrimary, Language::Korean) => "위젯: 주 모니터만",
        (LocalizationKey::MenuTaskbarPrimary, Language::English) => "Widget: primary monitor only",
        (LocalizationKey::UpdateAvailable, Language::Korean) => "새 업데이트를 사용할 수 있습니다",
        (LocalizationKey::UpdateAvailable, Language::English) => "An update is available",
        (LocalizationKey::UpdateCurrent, Language::Korean) => "최신 버전입니다",
        (LocalizationKey::UpdateCurrent, Language::English) => "You are up to date",
        (LocalizationKey::UpdateChecking, Language::Korean) => "업데이트를 확인하는 중입니다",
        (LocalizationKey::UpdateChecking, Language::English) => "Checking for updates",
        (LocalizationKey::UpdateFailed, Language::Korean) => "업데이트 확인에 실패했습니다",
        (LocalizationKey::UpdateFailed, Language::English) => "Update check failed",
        (LocalizationKey::WindowTitle, Language::Korean) => "Codex 사용량 모니터",
        (LocalizationKey::WindowTitle, Language::English) => "Codex Usage Monitor",
        (LocalizationKey::SettingsTitle, Language::Korean) => "Codex 사용량 모니터 설정",
        (LocalizationKey::SettingsTitle, Language::English) => "Codex Usage Monitor Settings",
        (LocalizationKey::DiagnosticsTitle, Language::Korean) => "Codex 사용량 모니터 진단",
        (LocalizationKey::DiagnosticsTitle, Language::English) => "Codex Usage Monitor Diagnostics",
        (LocalizationKey::PrimaryWindowLabel, Language::Korean) => "주 사용량 창",
        (LocalizationKey::PrimaryWindowLabel, Language::English) => "Primary window",
        (LocalizationKey::SecondaryWindowLabel, Language::Korean) => "보조 사용량 창",
        (LocalizationKey::SecondaryWindowLabel, Language::English) => "Secondary window",
        (LocalizationKey::DiagnosticCli, Language::Korean) => "Codex CLI를 확인할 수 없습니다",
        (LocalizationKey::DiagnosticCli, Language::English) => "Codex CLI could not be verified",
        (LocalizationKey::DiagnosticRpc, Language::Korean) => "Codex 서비스 요청에 실패했습니다",
        (LocalizationKey::DiagnosticRpc, Language::English) => "Codex service request failed",
        (LocalizationKey::DiagnosticLogin, Language::Korean) => "로그인 상태를 확인할 수 없습니다",
        (LocalizationKey::DiagnosticLogin, Language::English) => {
            "Login status could not be verified"
        }
        (LocalizationKey::DiagnosticSettings, Language::Korean) => {
            "설정을 읽거나 검증할 수 없습니다"
        }
        (LocalizationKey::DiagnosticSettings, Language::English) => {
            "Settings could not be read or validated"
        }
        (LocalizationKey::DiagnosticProxy, Language::Korean) => "프록시 사용 여부를 확인했습니다",
        (LocalizationKey::DiagnosticProxy, Language::English) => "Proxy presence was checked",
        (LocalizationKey::DiagnosticTaskbar, Language::Korean) => {
            "작업 표시줄 상태를 확인할 수 없습니다"
        }
        (LocalizationKey::DiagnosticTaskbar, Language::English) => {
            "Taskbar status could not be verified"
        }
        (LocalizationKey::MenuShowRemaining, Language::Korean) => "남은 사용량 표시",
        (LocalizationKey::MenuShowRemaining, Language::English) => "Show remaining usage",
        (LocalizationKey::MenuShowWeekly, Language::Korean) => "주간 사용량 표시",
        (LocalizationKey::MenuShowWeekly, Language::English) => "Show weekly usage",
    }
}

/// 갱신 간격 값을 트레이 메뉴에 표시할 지역화 문구로 만듭니다.
///
/// `minutes`는 지원되는 자동 갱신 간격이며, 호출자가 메뉴 동작 검증을 별도로 수행합니다.
pub(crate) fn localized_refresh_interval_menu_text(minutes: u32, language: Language) -> String {
    match language {
        Language::Korean => format!("갱신 간격: {minutes}분"),
        Language::English => format!("Refresh interval: {minutes} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::{localized_refresh_interval_menu_text, localized_text, Language, LocalizationKey};

    #[test]
    fn every_key_has_a_nonempty_translation() {
        for key in LocalizationKey::ALL {
            for language in [Language::Korean, Language::English] {
                assert!(!localized_text(*key, language).trim().is_empty());
            }
        }
    }

    #[test]
    fn refresh_interval_menu_text_includes_value_and_unit() {
        assert_eq!(
            localized_refresh_interval_menu_text(15, Language::Korean),
            "갱신 간격: 15분"
        );
        assert_eq!(
            localized_refresh_interval_menu_text(15, Language::English),
            "Refresh interval: 15 min"
        );
    }
}
