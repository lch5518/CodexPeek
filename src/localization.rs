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
    /// 표시 모드 메뉴입니다.
    MenuDisplayMode,
    /// 작업 표시줄 표시 모드입니다.
    MenuTaskbarMode,
    /// 플로팅 표시 모드입니다.
    MenuFloatingMode,
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
    /// 인증 갱신 메뉴입니다.
    MenuAuthRefresh,
    /// 항상 위 메뉴입니다.
    MenuAlwaysOnTop,
    /// 언어 메뉴입니다.
    MenuLanguage,
    /// 위치 초기화 메뉴입니다.
    MenuPositionReset,
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
}

impl LocalizationKey {
    /// 모든 문구 키를 빠짐없이 반환합니다.
    pub const ALL: &'static [Self] = &[
        Self::Polling,
        Self::Refreshing,
        Self::Stale,
        Self::Unavailable,
        Self::MenuRefresh,
        Self::MenuDisplayMode,
        Self::MenuTaskbarMode,
        Self::MenuFloatingMode,
        Self::MenuRefreshInterval,
        Self::MenuAutostart,
        Self::MenuStartupView,
        Self::MenuStartupWidget,
        Self::MenuStartupTrayOnly,
        Self::MenuAuthRefresh,
        Self::MenuAlwaysOnTop,
        Self::MenuLanguage,
        Self::MenuPositionReset,
        Self::MenuDiagnostics,
        Self::MenuUpdateCheck,
        Self::MenuSettings,
        Self::MenuExit,
        Self::MenuShowWidget,
        Self::MenuHideWidget,
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
        (LocalizationKey::MenuDisplayMode, Language::Korean) => "표시 모드",
        (LocalizationKey::MenuDisplayMode, Language::English) => "Display mode",
        (LocalizationKey::MenuTaskbarMode, Language::Korean) => "작업 표시줄",
        (LocalizationKey::MenuTaskbarMode, Language::English) => "Taskbar",
        (LocalizationKey::MenuFloatingMode, Language::Korean) => "플로팅 창",
        (LocalizationKey::MenuFloatingMode, Language::English) => "Floating window",
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
        (LocalizationKey::MenuAlwaysOnTop, Language::Korean) => "항상 위에 표시",
        (LocalizationKey::MenuAlwaysOnTop, Language::English) => "Always on top",
        (LocalizationKey::MenuLanguage, Language::Korean) => "언어",
        (LocalizationKey::MenuLanguage, Language::English) => "Language",
        (LocalizationKey::MenuPositionReset, Language::Korean) => "위치 초기화",
        (LocalizationKey::MenuPositionReset, Language::English) => "Reset position",
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
    }
}

#[cfg(test)]
mod tests {
    use super::{localized_text, Language, LocalizationKey};

    #[test]
    fn every_key_has_a_nonempty_translation() {
        for key in LocalizationKey::ALL {
            for language in [Language::Korean, Language::English] {
                assert!(!localized_text(*key, language).trim().is_empty());
            }
        }
    }
}
