//! Windows 애플리케이션의 형식화된 UI 경계와 플랫폼 구현입니다.

pub mod autostart;
pub mod native;
pub mod taskbar;
pub mod tray;
pub mod widget;

use crate::{DisplayMode, Language, LanguagePreference, LogicalPosition, StartupView};

/// 즉시 갱신 메뉴 식별자입니다.
pub const MENU_REFRESH: u16 = 100;
/// 작업 표시줄 모드 메뉴 식별자입니다.
pub const MENU_DISPLAY_TASKBAR: u16 = 110;
/// 부동 창 모드 메뉴 식별자입니다.
pub const MENU_DISPLAY_FLOATING: u16 = 111;
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
/// 자동 인증 갱신 메뉴 식별자입니다.
pub const MENU_AUTO_AUTH_REFRESH: u16 = 181;
/// 항상 위 메뉴 식별자입니다.
pub const MENU_ALWAYS_ON_TOP: u16 = 190;
/// 자동 언어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_AUTO: u16 = 200;
/// 한국어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_KOREAN: u16 = 201;
/// 영어 메뉴 식별자입니다.
pub const MENU_LANGUAGE_ENGLISH: u16 = 202;
/// 위치 초기화 메뉴 식별자입니다.
pub const MENU_POSITION_RESET: u16 = 210;
/// 진단 메뉴 식별자입니다.
pub const MENU_DIAGNOSTICS: u16 = 220;
/// 업데이트 확인 메뉴 식별자입니다.
pub const MENU_UPDATE_CHECK: u16 = 230;
/// 위젯 표시 전환 메뉴 식별자입니다.
pub const MENU_WIDGET_VISIBLE: u16 = 240;
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
    /// 표시 방식을 변경합니다.
    SetDisplayMode(DisplayMode),
    /// 자동 갱신 간격을 분 단위로 변경합니다.
    SetRefreshInterval(u32),
    /// Windows 자동 시작을 전환합니다.
    ToggleAutostart,
    /// 자동 시작 화면을 변경합니다.
    SetStartupView(StartupView),
    /// 인증 갱신을 강제한 뒤 사용량을 갱신합니다.
    RefreshWithAuth,
    /// 자동 인증 갱신 정책을 전환합니다.
    ToggleAutoAuthRefresh,
    /// 항상 위 표시를 전환합니다.
    ToggleAlwaysOnTop,
    /// 표시 언어를 변경합니다.
    SetLanguage(LanguagePreference),
    /// 저장된 창 위치를 초기화합니다.
    ResetPosition,
    /// 이동이 끝난 부동 창의 논리 좌표와 모니터 장치를 저장합니다.
    SaveFloatingPosition {
        position: LogicalPosition,
        monitor_device: Option<String>,
    },
    /// 안전 진단을 실행합니다.
    RunDiagnostics,
    /// 업데이트를 확인합니다.
    CheckForUpdates,
    /// 위젯 표시 여부를 전환합니다.
    ToggleWidget,
    /// 프로그램을 종료합니다.
    Exit,
}

/// Win32 메뉴 식별자를 형식화된 UI 동작으로 변환합니다.
pub fn menu_action(menu_id: u16) -> Option<UiAction> {
    Some(match menu_id {
        MENU_REFRESH => UiAction::Refresh,
        MENU_DISPLAY_TASKBAR => UiAction::SetDisplayMode(DisplayMode::Taskbar),
        MENU_DISPLAY_FLOATING => UiAction::SetDisplayMode(DisplayMode::Floating),
        MENU_INTERVAL_1 => UiAction::SetRefreshInterval(1),
        MENU_INTERVAL_5 => UiAction::SetRefreshInterval(5),
        MENU_INTERVAL_10 => UiAction::SetRefreshInterval(10),
        MENU_INTERVAL_15 => UiAction::SetRefreshInterval(15),
        MENU_INTERVAL_30 => UiAction::SetRefreshInterval(30),
        MENU_AUTOSTART => UiAction::ToggleAutostart,
        MENU_STARTUP_WIDGET => UiAction::SetStartupView(StartupView::Widget),
        MENU_STARTUP_TRAY => UiAction::SetStartupView(StartupView::TrayOnly),
        MENU_AUTH_REFRESH => UiAction::RefreshWithAuth,
        MENU_AUTO_AUTH_REFRESH => UiAction::ToggleAutoAuthRefresh,
        MENU_ALWAYS_ON_TOP => UiAction::ToggleAlwaysOnTop,
        MENU_LANGUAGE_AUTO => UiAction::SetLanguage(LanguagePreference::Auto),
        MENU_LANGUAGE_KOREAN => UiAction::SetLanguage(LanguagePreference::Korean),
        MENU_LANGUAGE_ENGLISH => UiAction::SetLanguage(LanguagePreference::English),
        MENU_POSITION_RESET => UiAction::ResetPosition,
        MENU_DIAGNOSTICS => UiAction::RunDiagnostics,
        MENU_UPDATE_CHECK => UiAction::CheckForUpdates,
        MENU_WIDGET_VISIBLE => UiAction::ToggleWidget,
        MENU_EXIT => UiAction::Exit,
        _ => return None,
    })
}

/// 렌더러가 소비하는 민감하지 않은 단일 사용량 행입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageRowView {
    /// 기간 표시 문자열입니다.
    pub label: String,
    /// 원래 사용률이며 시각적 막대에서만 제한됩니다.
    pub used_percent: f64,
    /// 사용자에게 표시할 퍼센트 문자열입니다.
    pub percent_text: String,
    /// 초기화 시각 안내 문자열입니다.
    pub reset_text: String,
    /// 색상 외 형태 선택에 쓰는 수준입니다.
    pub level: crate::UsageLevel,
}

/// Windows UI가 렌더링하는 불변 상태 복사본입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetViewModel {
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
}

/// 메뉴 체크 상태와 창 정책에 필요한 비민감 설정 복사본입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSettings {
    /// 위젯 표시 방식입니다.
    pub display_mode: DisplayMode,
    /// 위젯 표시 여부입니다.
    pub widget_visible: bool,
    /// 자동 갱신 간격입니다.
    pub refresh_interval_minutes: u32,
    /// 항상 위 표시 여부입니다.
    pub always_on_top: bool,
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
    /// 저장된 부동 창 논리 좌표입니다.
    pub floating_position: Option<LogicalPosition>,
    /// 저장된 모니터 장치 이름입니다.
    pub monitor_device: Option<String>,
}

/// 플랫폼 메시지 루프가 애플리케이션 상태와 통신하는 최소 경계입니다.
pub trait UiBackend {
    /// 최신 렌더링 복사본을 반환합니다.
    fn snapshot(&self) -> WidgetViewModel;
    /// 현재 메뉴 및 창 설정 복사본을 반환합니다.
    fn settings(&self) -> UiSettings;
    /// UI 동작을 처리하고 갱신된 설정을 반환합니다.
    fn dispatch(&mut self, action: UiAction) -> UiSettings;
}
