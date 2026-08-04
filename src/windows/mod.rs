//! Windows 애플리케이션의 형식화된 UI 경계와 플랫폼 구현입니다.

pub mod autostart;
pub mod design;
pub mod lifecycle;
pub mod native;
pub mod profile_dialog;
pub mod taskbar;
pub mod taskbar_widget;
pub(crate) mod theme;
pub(crate) mod time;
pub mod tray;
pub mod widget;

use crate::{Language, LanguagePreference, StartupView, TaskbarDisplayMode, UsageProfileId};

/// 즉시 갱신 메뉴 식별자입니다.
pub const MENU_REFRESH: u16 = 100;
/// 1분 갱신 간격 메뉴 식별자입니다.
pub const MENU_INTERVAL_1: u16 = 121;
/// 5분 갱신 간격 메뉴 식별자입니다.
pub const MENU_INTERVAL_5: u16 = 125;
/// 10분 갱신 간격 메뉴 식별자입니다.
pub const MENU_INTERVAL_10: u16 = 130;
/// 15분 갱신 간격 메뉴 식별자입니다.
pub const MENU_INTERVAL_15: u16 = 135;
/// 30분 갱신 간격 메뉴 식별자입니다.
pub const MENU_INTERVAL_30: u16 = 150;
/// 자동 시작 메뉴 식별자입니다.
pub const MENU_AUTOSTART: u16 = 160;
/// 위젯 시작 화면 메뉴 식별자입니다.
pub const MENU_STARTUP_WIDGET: u16 = 170;
/// 트레이 전용 시작 화면 메뉴 식별자입니다.
pub const MENU_STARTUP_TRAY: u16 = 171;
/// 강제 인증 갱신 메뉴 식별자입니다.
pub const MENU_AUTH_REFRESH: u16 = 180;
/// ChatGPT 로그인 시작 메뉴 식별자입니다.
pub const MENU_LOGIN: u16 = 182;
/// 자동 인증 갱신 메뉴 식별자입니다.
pub const MENU_AUTO_AUTH_REFRESH: u16 = 181;
/// 자동 언어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_AUTO: u16 = 200;
/// 한국어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_KOREAN: u16 = 201;
/// 영어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_ENGLISH: u16 = 202;
/// 스페인어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_SPANISH: u16 = 203;
/// 브라질 포르투갈어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_PORTUGUESE_BRAZIL: u16 = 204;
/// 인도네시아어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_INDONESIAN: u16 = 205;
/// 일본어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_JAPANESE: u16 = 206;
/// 힌디어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_HINDI: u16 = 207;
/// 독일어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_GERMAN: u16 = 208;
/// 프랑스어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_FRENCH: u16 = 209;
/// 베트남어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_VIETNAMESE: u16 = 210;
/// 터키어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_TURKISH: u16 = 211;
/// 아랍어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_ARABIC: u16 = 212;
/// 진단 메뉴 식별자입니다.
pub const MENU_DIAGNOSTICS: u16 = 220;
/// 업데이트 확인 메뉴 식별자입니다.
pub const MENU_UPDATE_CHECK: u16 = 230;
/// 위젯 표시 전환 메뉴 식별자입니다.
pub const MENU_WIDGET_VISIBLE: u16 = 240;
/// 사용량 프로필 추가 메뉴 식별자입니다.
pub const MENU_ADD_USAGE_PROFILE: u16 = 260;
/// 사용량 프로필 관리 메뉴 식별자입니다.
pub const MENU_MANAGE_USAGE_PROFILES: u16 = 261;
/// 사용량 소진 예측 전환 메뉴 식별자입니다.
pub const MENU_USAGE_FORECAST_TOGGLE: u16 = 270;
/// 사용량 소진 예측 기록 삭제 메뉴 식별자입니다.
pub const MENU_USAGE_FORECAST_CLEAR_HISTORY: u16 = 271;
/// 사용량 소진 예측 하위 메뉴 식별자입니다.
pub const MENU_USAGE_FORECAST: u16 = 272;
/// 남은 한도 표시 토글 메뉴 식별자입니다.
pub const MENU_SHOW_REMAINING: u16 = 241;
/// 모든 모니터 작업표시줄 표시 메뉴 식별자입니다.
pub const MENU_TASKBAR_ALL: u16 = 242;
/// 주 모니터 작업표시줄만 표시하는 메뉴 식별자입니다.
pub const MENU_TASKBAR_PRIMARY: u16 = 243;
/// 종료 메뉴 식별자입니다.
pub const MENU_EXIT: u16 = 250;

/// 명령줄에서 선택한 실행 방식입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    /// 저장된 일반 시작 설정을 사용합니다.
    Normal,
    /// Windows 자동 시작 규칙을 적용합니다.
    Startup,
    /// 진단만 실행하고 UI를 시작하지 않습니다.
    Diagnose,
}

/// 정상 시작에서 부작용이 일어나는 순서를 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupStep {
    /// 단일 인스턴스 소유권을 먼저 획득합니다.
    AcquireSingleInstance,
    /// 설정을 읽습니다.
    LoadSettings,
    /// 폴링 작업자를 시작합니다.
    StartPoller,
    /// 백그라운드 업데이트 확인을 예약합니다.
    StartUpdateCheck,
    /// 네이티브 UI를 실행합니다.
    RunUi,
    /// 부작용 없는 진단 모드를 실행합니다.
    RunDiagnostics,
}

const NORMAL_STARTUP: &[StartupStep] = &[
    StartupStep::AcquireSingleInstance,
    StartupStep::LoadSettings,
    StartupStep::StartPoller,
    StartupStep::StartUpdateCheck,
    StartupStep::RunUi,
];
const DIAGNOSTIC_STARTUP: &[StartupStep] = &[StartupStep::RunDiagnostics];

/// 실행 모드별 부작용 순서를 반환합니다.
pub const fn startup_plan(mode: LaunchMode) -> &'static [StartupStep] {
    match mode {
        LaunchMode::Diagnose => DIAGNOSTIC_STARTUP,
        LaunchMode::Normal | LaunchMode::Startup => NORMAL_STARTUP,
    }
}

/// 저장된 선택과 Windows UI 언어 정보로 실제 표시 언어를 결정합니다.
pub fn resolve_windows_language(
    preference: LanguagePreference,
    ui_language: Option<u16>,
    locale_name: Option<&str>,
) -> Language {
    match preference {
        LanguagePreference::Korean => Language::Korean,
        LanguagePreference::English => Language::English,
        LanguagePreference::Spanish => Language::Spanish,
        LanguagePreference::PortugueseBrazil => Language::PortugueseBrazil,
        LanguagePreference::Indonesian => Language::Indonesian,
        LanguagePreference::Japanese => Language::Japanese,
        LanguagePreference::Hindi => Language::Hindi,
        LanguagePreference::German => Language::German,
        LanguagePreference::French => Language::French,
        LanguagePreference::Vietnamese => Language::Vietnamese,
        LanguagePreference::Turkish => Language::Turkish,
        LanguagePreference::Arabic => Language::Arabic,
        LanguagePreference::Auto => ui_language
            .and_then(language_from_langid)
            .or_else(|| locale_name.and_then(language_from_locale_name))
            .unwrap_or(Language::English),
    }
}

fn language_from_langid(language: u16) -> Option<Language> {
    let primary_language = language & 0x03ff;
    match primary_language {
        0x01 => Some(Language::Arabic),
        0x07 => Some(Language::German),
        0x09 => Some(Language::English),
        0x0a => Some(Language::Spanish),
        0x0c => Some(Language::French),
        0x11 => Some(Language::Japanese),
        0x12 => Some(Language::Korean),
        0x16 if language == 0x0416 => Some(Language::PortugueseBrazil),
        0x1f => Some(Language::Turkish),
        0x21 => Some(Language::Indonesian),
        0x2a => Some(Language::Vietnamese),
        0x39 => Some(Language::Hindi),
        _ => None,
    }
}

fn language_from_locale_name(locale: &str) -> Option<Language> {
    let locale = locale.to_ascii_lowercase().replace('_', "-");
    let primary = locale.split('-').next().unwrap_or_default();
    match primary {
        "ar" => Some(Language::Arabic),
        "de" => Some(Language::German),
        "en" => Some(Language::English),
        "es" => Some(Language::Spanish),
        "fr" => Some(Language::French),
        "hi" => Some(Language::Hindi),
        "id" => Some(Language::Indonesian),
        "ja" => Some(Language::Japanese),
        "ko" => Some(Language::Korean),
        "pt" if locale == "pt-br" || locale.starts_with("pt-br-") => {
            Some(Language::PortugueseBrazil)
        }
        "tr" => Some(Language::Turkish),
        "vi" => Some(Language::Vietnamese),
        _ => None,
    }
}

/// URL이 추가 경로가 없는 정확한 GitHub 릴리스 태그 페이지인지 확인합니다.
pub fn is_exact_github_tag_page(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    if url.contains(['?', '#', '@', '\r', '\n', '\0']) {
        return false;
    }
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 5
        && valid_github_segment(parts[0])
        && valid_github_segment(parts[1])
        && parts[2] == "releases"
        && parts[3] == "tag"
        && valid_github_segment(parts[4])
}

fn valid_github_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

impl LaunchMode {
    /// 프로그램 이름을 제외한 명령줄 인자를 엄격하게 해석합니다.
    ///
    /// 알 수 없는 인자나 둘 이상의 모드가 주어지면 오류를 반환합니다.
    pub fn parse<I, S>(arguments: I) -> Result<Self, &'static str>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut mode = Self::Normal;
        for argument in arguments {
            let next = match argument.as_ref() {
                "--startup" => Self::Startup,
                "--diagnose" => Self::Diagnose,
                _ => return Err("unsupported argument"),
            };
            if mode != Self::Normal {
                return Err("multiple launch modes");
            }
            mode = next;
        }
        Ok(mode)
    }
}

/// 실행 모드와 저장된 설정으로 최초 위젯 표시 여부를 계산합니다.
///
/// 자동 시작의 트레이 전용 선택은 현재 실행만 숨기며 저장된 표시 선호를 변경하지 않습니다.
pub const fn initial_widget_visible(
    mode: LaunchMode,
    startup_view: StartupView,
    saved_visible: bool,
) -> bool {
    saved_visible
        && !(matches!(mode, LaunchMode::Startup) && matches!(startup_view, StartupView::TrayOnly))
}

/// Win32 UI가 애플리케이션 계층으로 전달하는 형식화된 동작입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    /// 즉시 사용량을 갱신합니다.
    Refresh,
    /// 자동 갱신 간격을 분 단위로 변경합니다.
    SetRefreshInterval(u32),
    /// Windows 자동 시작을 전환합니다.
    ToggleAutostart,
    /// 자동 시작 화면을 변경합니다.
    SetStartupView(StartupView),
    /// 인증 갱신을 강제한 뒤 사용량을 갱신합니다.
    RefreshWithAuth,
    /// ChatGPT 브라우저 로그인을 시작합니다.
    Login,
    /// 자동 인증 갱신 정책을 전환합니다.
    ToggleAutoAuthRefresh,
    /// 표시 언어를 변경합니다.
    SetLanguage(LanguagePreference),
    /// 안전 진단을 실행합니다.
    RunDiagnostics,
    /// 업데이트를 확인합니다.
    CheckForUpdates,
    /// 위젯 표시 여부를 전환합니다.
    ToggleWidget,
    /// 남은 한도 표시 여부를 전환합니다.
    ToggleShowRemaining,
    /// 사용량 소진 예측 기록과 계산을 전환합니다.
    ToggleUsageForecast,
    /// 저장된 사용량 소진 예측 기록을 삭제합니다.
    ClearUsageHistory,
    /// 작업표시줄 위젯을 표시할 모니터 범위를 변경합니다.
    SetTaskbarDisplayMode(TaskbarDisplayMode),
    /// 새 사용량 프로필 입력 UI를 열도록 요청합니다.
    OpenAddUsageProfile,
    /// 사용량 프로필 관리 UI를 열도록 요청합니다.
    OpenManageUsageProfiles,
    /// 표시할 사용량 프로필을 선택합니다.
    SelectUsageProfile(UsageProfileId),
    /// 검증된 표시 이름으로 사용량 프로필을 추가합니다.
    AddUsageProfile(String),
    /// 지정한 사용량 프로필의 표시 이름을 변경합니다.
    RenameUsageProfile(UsageProfileId, String),
    /// 지정한 사용량 프로필의 브라우저 로그인을 시작합니다.
    LoginUsageProfile(UsageProfileId),
    /// 지정한 사용량 프로필에서 로그아웃합니다.
    LogoutUsageProfile(UsageProfileId),
    /// 지정한 관리 사용량 프로필을 삭제합니다.
    DeleteUsageProfile(UsageProfileId),
    /// 프로그램을 종료합니다.
    Exit,
}

/// Win32 메뉴 식별자를 형식화된 UI 동작으로 변환합니다.
pub fn menu_action(menu_id: u16) -> Option<UiAction> {
    Some(match menu_id {
        MENU_REFRESH => UiAction::Refresh,
        MENU_INTERVAL_1 => UiAction::SetRefreshInterval(1),
        MENU_INTERVAL_5 => UiAction::SetRefreshInterval(5),
        MENU_INTERVAL_10 => UiAction::SetRefreshInterval(10),
        MENU_INTERVAL_15 => UiAction::SetRefreshInterval(15),
        MENU_INTERVAL_30 => UiAction::SetRefreshInterval(30),
        MENU_AUTOSTART => UiAction::ToggleAutostart,
        MENU_STARTUP_WIDGET => UiAction::SetStartupView(StartupView::Widget),
        MENU_STARTUP_TRAY => UiAction::SetStartupView(StartupView::TrayOnly),
        MENU_AUTH_REFRESH => UiAction::RefreshWithAuth,
        MENU_LOGIN => UiAction::Login,
        MENU_AUTO_AUTH_REFRESH => UiAction::ToggleAutoAuthRefresh,
        MENU_LANGUAGE_AUTO => UiAction::SetLanguage(LanguagePreference::Auto),
        MENU_LANGUAGE_KOREAN => UiAction::SetLanguage(LanguagePreference::Korean),
        MENU_LANGUAGE_ENGLISH => UiAction::SetLanguage(LanguagePreference::English),
        MENU_LANGUAGE_SPANISH => UiAction::SetLanguage(LanguagePreference::Spanish),
        MENU_LANGUAGE_PORTUGUESE_BRAZIL => {
            UiAction::SetLanguage(LanguagePreference::PortugueseBrazil)
        }
        MENU_LANGUAGE_INDONESIAN => UiAction::SetLanguage(LanguagePreference::Indonesian),
        MENU_LANGUAGE_JAPANESE => UiAction::SetLanguage(LanguagePreference::Japanese),
        MENU_LANGUAGE_HINDI => UiAction::SetLanguage(LanguagePreference::Hindi),
        MENU_LANGUAGE_GERMAN => UiAction::SetLanguage(LanguagePreference::German),
        MENU_LANGUAGE_FRENCH => UiAction::SetLanguage(LanguagePreference::French),
        MENU_LANGUAGE_VIETNAMESE => UiAction::SetLanguage(LanguagePreference::Vietnamese),
        MENU_LANGUAGE_TURKISH => UiAction::SetLanguage(LanguagePreference::Turkish),
        MENU_LANGUAGE_ARABIC => UiAction::SetLanguage(LanguagePreference::Arabic),
        MENU_DIAGNOSTICS => UiAction::RunDiagnostics,
        MENU_UPDATE_CHECK => UiAction::CheckForUpdates,
        MENU_WIDGET_VISIBLE => UiAction::ToggleWidget,
        MENU_SHOW_REMAINING => UiAction::ToggleShowRemaining,
        MENU_USAGE_FORECAST_TOGGLE => UiAction::ToggleUsageForecast,
        MENU_USAGE_FORECAST_CLEAR_HISTORY => UiAction::ClearUsageHistory,
        MENU_TASKBAR_ALL => UiAction::SetTaskbarDisplayMode(TaskbarDisplayMode::All),
        MENU_TASKBAR_PRIMARY => UiAction::SetTaskbarDisplayMode(TaskbarDisplayMode::Primary),
        MENU_ADD_USAGE_PROFILE => UiAction::OpenAddUsageProfile,
        MENU_MANAGE_USAGE_PROFILES => UiAction::OpenManageUsageProfiles,
        MENU_EXIT => UiAction::Exit,
        _ => return None,
    })
}

/// 렌더러가 소비하는 민감하지 않은 단일 사용량 행입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageRowView {
    /// 기간 표시 문자열입니다.
    pub label: String,
    /// 위험 수준과 툴팁에 사용하는 원래 사용률입니다.
    pub used_percent: f64,
    /// 현재 표시 모드에 맞춰 진행 막대에 사용하는 비율입니다.
    pub display_percent: f64,
    /// 사용자에게 표시할 퍼센트 문자열입니다.
    pub percent_text: String,
    /// Windows 현지 날짜·요일·시각으로 구성한 초기화 안내 문자열입니다.
    pub reset_text: String,
    /// 색상 외 형태 선택에 쓰는 수준입니다.
    pub level: crate::UsageLevel,
    /// 계산 세부사항을 해석하지 않고 한 줄로 표시할 수 있는 소진 예측 상태입니다.
    pub forecast: ForecastView,
}

/// 사용량 행에 표시할 소진 예측의 지역화된 상태입니다.
///
/// 계산 엔진의 결과를 UI 렌더러에 노출하지 않고, 애플리케이션 계층이 만든 안전한 한 줄
/// 문구와 상태만 전달합니다. `line`은 작업 표시줄 상세 툴팁에 추가할 때 사용합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForecastView {
    /// 예측을 표시하지 않는 상태입니다.
    Hidden,
    /// 아직 표본을 수집 중인 상태입니다.
    Collecting { line: String },
    /// 관측 활동이 부족해 소진 시각을 계산할 수 없는 상태입니다.
    InsufficientActivity { line: String },
    /// 소진 예측을 표시할 수 있는 상태입니다.
    ForecastAvailable { line: String },
    /// 현재 사용량이 한도에 도달한 상태입니다.
    AlreadyExhausted { line: String },
    /// 마지막 표본이 오래되어 예측을 숨기거나 상태만 표시하는 상태입니다.
    Stale { line: String },
    /// 표본 검증에 실패해 예측할 수 없는 상태입니다.
    Invalid { line: String },
}

impl ForecastView {
    /// 툴팁에 추가할 지역화 문구를 반환합니다.
    pub fn line(&self) -> Option<&str> {
        match self {
            Self::Hidden => None,
            Self::Collecting { line }
            | Self::InsufficientActivity { line }
            | Self::ForecastAvailable { line }
            | Self::AlreadyExhausted { line }
            | Self::Stale { line }
            | Self::Invalid { line } => Some(line),
        }
    }
}

/// 프로필 목록에서 사용량 진행 상태의 표시 색상을 선택하는 기준입니다.
///
/// 입력 사용률은 원본 사용량 창의 값이며, 70%와 90% 경계에서 각각 경고와 위험 상태로
/// 전환됩니다. 이 기준은 전역 `UsageLevel`의 임계값을 변경하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileUsageStatus {
    /// 사용률이 70% 미만인 정상 상태입니다.
    Healthy,
    /// 사용률이 70% 이상 90% 미만인 주의 상태입니다.
    Warning,
    /// 사용률이 90% 이상인 위험 상태입니다.
    Critical,
}

impl ProfileUsageStatus {
    /// 원본 사용률을 프로필 행의 표시 상태로 변환합니다.
    ///
    /// `used_percent`는 유효성이 검증된 사용량 창에서 받은 값이어야 하며, 반환값은 프로필
    /// 관리자 진행 표시의 색상 선택에만 사용합니다.
    pub fn from_used_percent(used_percent: f64) -> Self {
        if used_percent >= 90.0 {
            Self::Critical
        } else if used_percent >= 70.0 {
            Self::Warning
        } else {
            Self::Healthy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileUsageStatus;

    #[test]
    fn profile_usage_status_uses_the_design_system_thresholds() {
        assert_eq!(
            ProfileUsageStatus::from_used_percent(0.0),
            ProfileUsageStatus::Healthy
        );
        assert_eq!(
            ProfileUsageStatus::from_used_percent(69.99),
            ProfileUsageStatus::Healthy
        );
        assert_eq!(
            ProfileUsageStatus::from_used_percent(70.0),
            ProfileUsageStatus::Warning
        );
        assert_eq!(
            ProfileUsageStatus::from_used_percent(89.99),
            ProfileUsageStatus::Warning
        );
        assert_eq!(
            ProfileUsageStatus::from_used_percent(90.0),
            ProfileUsageStatus::Critical
        );
        assert_eq!(
            ProfileUsageStatus::from_used_percent(125.0),
            ProfileUsageStatus::Critical
        );
    }
}

/// UI에 노출할 수 있는 비민감 사용량 프로필 표시 정보입니다.
///
/// 내부 경로와 계정 식별 정보는 포함하지 않으며, `label`과 사용량 요약만 사용자 화면에
/// 표시합니다. `id`는 형식화된 UI 동작을 런타임 프로필에 연결하는 데만 사용합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageProfileView {
    /// 설정과 폴링 상태를 연결하는 안정적인 프로필 식별자입니다.
    pub id: UsageProfileId,
    /// 사용자에게 표시할 지역화 또는 검증된 프로필 이름입니다.
    pub label: String,
    /// 사용량 또는 로그인 상태를 요약한 지역화 문구입니다.
    pub summary: String,
    /// 리셋권·단기·주간 한도를 한 줄로 요약한 지역화 문구입니다.
    ///
    /// 사용량을 확인할 수 없거나 로그인이 필요한 경우에는 빈 문자열이며, 계정 식별자나 인증
    /// 정보는 포함하지 않습니다.
    pub details: String,
    /// 현재 위젯이 이 프로필의 사용량을 표시하는지 나타냅니다.
    pub selected: bool,
    /// 이 프로필에 Codex 로그인이 필요한지 나타냅니다.
    pub login_required: bool,
    /// 프로필 행의 진행 표시를 위한 반올림된 사용률입니다.
    ///
    /// 로그인 필요, 초기 로딩 또는 사용량 부재 상태에서는 가짜 진행 표시를 막기 위해 `None`입니다.
    pub used_percent: Option<u8>,
    /// 프로필 행의 진행 표시 색상에 사용하는 사용률 상태입니다.
    ///
    /// `used_percent`와 함께 제공되며, 진행 표시가 없을 때는 `None`입니다.
    pub usage_status: Option<ProfileUsageStatus>,
    /// 앱이 격리 저장소를 관리하는 프로필인지 나타냅니다.
    pub managed: bool,
}

/// 작업 표시줄이 표현하는 조회 상태입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetDataState {
    /// 첫 사용량을 불러오는 중입니다.
    Loading,
    /// 표시 가능한 최신 또는 보존 데이터가 있습니다.
    Ready,
    /// 최근 사용량 조회가 실패했습니다.
    Error,
}

/// Windows UI가 렌더링하는 불변 상태 복사본입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetViewModel {
    /// 플로팅 위젯에 항상 표시할 선택된 사용량 프로필 이름입니다.
    pub usage_profile_label: String,
    /// 주 사용량 행입니다.
    pub primary: Option<UsageRowView>,
    /// 보조 사용량 행입니다.
    pub secondary: Option<UsageRowView>,
    /// 새로 고침 또는 오류 상태 문자열입니다.
    pub status: String,
    /// 마지막 성공 시각 문자열입니다.
    pub last_success: String,
    /// 오래된 정보인지 나타냅니다.
    pub is_stale: bool,
    /// 작업 표시줄의 고정 주간 레이블입니다.
    pub taskbar_label: String,
    /// 작업 표시줄에 연결할 상세 툴팁 문구입니다.
    pub taskbar_tooltip: String,
    /// 트레이 메뉴와 툴팁에 표시할 리셋권 요약 문구입니다.
    pub reset_credits_text: Option<String>,
    /// 작업 표시줄의 로딩·정상·오류 표현 상태입니다.
    pub data_state: WidgetDataState,
}

/// Codex app-server가 제공한 ChatGPT 브라우저 로그인 URL인지 확인합니다.
///
/// HTTPS와 허용된 OpenAI 도메인만 허용하며, URL의 쿼리 문자열은 로컬 콜백 정보를 포함할 수 있어
/// 유지합니다. 인증 URL은 사용자에게 표시하거나 로그에 남기지 않아야 합니다.
pub fn is_valid_chatgpt_login_url(url: &str) -> bool {
    if url.len() > 8 * 1024 || url.contains(['\r', '\n', '\0']) {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains(['@', ':']) {
        return false;
    }
    matches!(
        authority.to_ascii_lowercase().as_str(),
        "chatgpt.com" | "auth.openai.com"
    )
}

/// 메뉴 체크 상태와 창 정책에 필요한 비민감 설정 복사본입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSettings {
    /// 위젯 표시 여부입니다.
    pub widget_visible: bool,
    /// 자동 갱신 간격입니다.
    pub refresh_interval_minutes: u32,
    /// Windows 자동 시작 여부입니다.
    pub start_with_windows: bool,
    /// 자동 시작 시 표시 방식입니다.
    pub startup_view: StartupView,
    /// 자동 인증 갱신 여부입니다.
    pub auto_auth_refresh: bool,
    /// 언어 선택입니다.
    pub language: LanguagePreference,
    /// 자동 선택을 해석한 실제 표시 언어입니다.
    pub resolved_language: Language,
    /// 작업 표시줄 논리 픽셀 오프셋입니다.
    pub taskbar_offset: i32,
    /// 다중 모니터에서 작업표시줄 위젯을 표시할 범위입니다.
    pub taskbar_display_mode: TaskbarDisplayMode,
    /// 사용자에게 표시할 업데이트 검사 상태입니다.
    pub update_status: crate::UpdatePresentationStatus,
    /// 위젯 숫자를 남은 한도(%)로 표시할지 여부입니다.
    pub show_remaining_percent: bool,
    /// 사용량 소진 예측 기록과 표시가 활성화되어 있는지 여부입니다.
    pub usage_forecast_enabled: bool,
    /// Codex 로그인이 필요해 트레이 메뉴에 로그인 동작을 표시할지 여부입니다.
    pub login_required: bool,
    /// 트레이와 프로필 관리 UI에 표시할 비민감 프로필 목록입니다.
    pub usage_profiles: Vec<UsageProfileView>,
    /// 프로필 설정 변경이 저장 워커에서 완료되기를 기다리는지 나타냅니다.
    pub usage_profile_mutation_pending: bool,
}

/// 작업 표시줄 상세 툴팁 앞에 선택 프로필과 CLI 격리 안내를 추가합니다.
///
/// `usage_profile_label`은 이미 검증되거나 지역화된 표시 이름이고 `details`는 기존 사용량
/// 툴팁입니다. 반환 문자열에는 프로필 경로나 계정 식별 정보가 추가되지 않습니다.
pub fn profile_taskbar_tooltip(
    usage_profile_label: &str,
    details: &str,
    language: Language,
) -> String {
    format!(
        "{}: {usage_profile_label}\n{}\n\n{details}",
        crate::localized_text(crate::LocalizationKey::MenuUsageProfiles, language),
        crate::localized_text(crate::LocalizationKey::UsageProfileCliUnchanged, language),
    )
}

/// 플랫폼 메시지 루프가 애플리케이션 상태와 통신하는 최소 경계입니다.
pub trait UiBackend {
    /// 최신 렌더링 복사본을 반환합니다.
    fn snapshot(&self) -> WidgetViewModel;
    /// 현재 메뉴 및 창 설정 복사본을 반환합니다.
    fn settings(&self) -> UiSettings;
    /// UI 스레드가 사용자에게 표시할 수동 업데이트 결과를 한 번 꺼냅니다.
    ///
    /// 백엔드는 네트워크 작업을 수행하지 않으며, 반환된 알림은 네이티브 소유 창에서 대화상자로
    /// 표시한 뒤 다시 반환하지 않아야 합니다.
    fn take_update_notice(&self) -> Option<crate::UpdateCheckNotice> {
        None
    }
    /// UI 동작을 처리하고 갱신된 설정을 반환합니다.
    fn dispatch(&mut self, action: UiAction) -> UiSettings;
    /// 사용자가 선택 프로필을 확인한 로그인 동작을 처리하고 갱신된 설정을 반환합니다.
    ///
    /// 네이티브 UI는 표시 이름과 CLI·IDE 비변경 안내를 보여 준 뒤에만 호출해야 합니다. 기본
    /// 구현은 기존 backend 호환성을 위해 `dispatch`에 위임합니다.
    fn dispatch_confirmed_profile_login(&mut self, action: UiAction) -> UiSettings {
        self.dispatch(action)
    }
}
