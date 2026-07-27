//! 알림 영역 아이콘과 메뉴의 Windows 구현입니다.

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::{AsyncTrayIcon, TrayIcon, TRAY_CALLBACK};

use crate::{Language, LanguagePreference, StartupView, TaskbarDisplayMode};

use super::{
    UiSettings, MENU_AUTH_REFRESH, MENU_AUTOSTART, MENU_AUTO_AUTH_REFRESH, MENU_DIAGNOSTICS,
    MENU_EXIT, MENU_INTERVAL_1, MENU_INTERVAL_10, MENU_INTERVAL_15, MENU_INTERVAL_30,
    MENU_INTERVAL_5, MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH, MENU_LANGUAGE_KOREAN, MENU_LOGIN,
    MENU_REFRESH, MENU_SHOW_REMAINING, MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET, MENU_TASKBAR_ALL,
    MENU_TASKBAR_PRIMARY, MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
};

/// 트레이 메뉴에 표시할 순수 항목 모델입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayMenuEntry {
    /// 선택 가능한 명령 항목입니다.
    Command(TrayMenuCommand),
    /// 메뉴 구획을 나누는 구분선입니다.
    Separator,
}

/// Win32 메뉴에 추가할 명령 항목의 표시 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuCommand {
    /// 플랫폼 메뉴 명령 식별자입니다.
    pub id: u16,
    /// 사용자에게 표시할 지역화 문구입니다.
    pub label: String,
    /// 메뉴 체크 표시 여부입니다.
    pub checked: bool,
}

/// 현재 설정에 맞는 트레이 메뉴 항목을 순서대로 반환합니다.
pub fn tray_menu_entries(settings: &UiSettings) -> Vec<TrayMenuEntry> {
    let language = settings.resolved_language;
    let mut entries = Vec::new();
    if settings.login_required {
        push_command(
            &mut entries,
            MENU_LOGIN,
            crate::localized_text(crate::LocalizationKey::MenuLogin, language),
            false,
        );
    }
    push_command(
        &mut entries,
        MENU_REFRESH,
        crate::localized_text(crate::LocalizationKey::MenuRefreshNow, language),
        false,
    );
    entries.push(TrayMenuEntry::Separator);
    for (id, minutes) in [
        (MENU_INTERVAL_1, 1),
        (MENU_INTERVAL_5, 5),
        (MENU_INTERVAL_10, 10),
        (MENU_INTERVAL_15, 15),
        (MENU_INTERVAL_30, 30),
    ] {
        push_command(
            &mut entries,
            id,
            crate::localization::localized_refresh_interval_menu_text(minutes, language),
            settings.refresh_interval_minutes == minutes,
        );
    }
    entries.push(TrayMenuEntry::Separator);
    push_command(
        &mut entries,
        MENU_AUTOSTART,
        crate::localized_text(crate::LocalizationKey::MenuAutostart, language),
        settings.start_with_windows,
    );
    push_command(
        &mut entries,
        MENU_STARTUP_WIDGET,
        crate::localized_text(crate::LocalizationKey::MenuStartupWidgetChoice, language),
        settings.startup_view == StartupView::Widget,
    );
    push_command(
        &mut entries,
        MENU_STARTUP_TRAY,
        crate::localized_text(crate::LocalizationKey::MenuStartupTrayOnlyChoice, language),
        settings.startup_view == StartupView::TrayOnly,
    );
    if !settings.login_required {
        push_command(
            &mut entries,
            MENU_AUTH_REFRESH,
            crate::localized_text(crate::LocalizationKey::MenuAuthRefreshNow, language),
            false,
        );
    }
    push_command(
        &mut entries,
        MENU_AUTO_AUTH_REFRESH,
        crate::localized_text(crate::LocalizationKey::MenuAuthRefresh, language),
        settings.auto_auth_refresh,
    );
    push_command(
        &mut entries,
        MENU_LANGUAGE_AUTO,
        language_menu_label(LanguagePreference::Auto, language),
        settings.language == LanguagePreference::Auto,
    );
    push_command(
        &mut entries,
        MENU_LANGUAGE_KOREAN,
        language_menu_label(LanguagePreference::Korean, language),
        settings.language == LanguagePreference::Korean,
    );
    push_command(
        &mut entries,
        MENU_LANGUAGE_ENGLISH,
        language_menu_label(LanguagePreference::English, language),
        settings.language == LanguagePreference::English,
    );
    push_command(
        &mut entries,
        MENU_SHOW_REMAINING,
        usage_mode_menu_text(settings.show_remaining_percent, language),
        false,
    );
    entries.push(TrayMenuEntry::Separator);
    push_command(
        &mut entries,
        MENU_DIAGNOSTICS,
        crate::localized_text(crate::LocalizationKey::MenuDiagnostics, language),
        false,
    );
    push_command(
        &mut entries,
        MENU_UPDATE_CHECK,
        update_menu_text(settings.update_status, language),
        false,
    );
    let widget_key = if settings.widget_visible {
        crate::LocalizationKey::MenuHideWidget
    } else {
        crate::LocalizationKey::MenuShowWidget
    };
    push_command(
        &mut entries,
        MENU_WIDGET_VISIBLE,
        crate::localized_text(widget_key, language),
        settings.widget_visible,
    );
    push_command(
        &mut entries,
        MENU_TASKBAR_ALL,
        crate::localized_text(crate::LocalizationKey::MenuTaskbarAll, language),
        settings.taskbar_display_mode == TaskbarDisplayMode::All,
    );
    push_command(
        &mut entries,
        MENU_TASKBAR_PRIMARY,
        crate::localized_text(crate::LocalizationKey::MenuTaskbarPrimary, language),
        settings.taskbar_display_mode == TaskbarDisplayMode::Primary,
    );
    entries.push(TrayMenuEntry::Separator);
    push_command(
        &mut entries,
        MENU_EXIT,
        crate::localized_text(crate::LocalizationKey::MenuExit, language),
        false,
    );
    entries
}

fn push_command(
    entries: &mut Vec<TrayMenuEntry>,
    id: u16,
    label: impl Into<String>,
    checked: bool,
) {
    entries.push(TrayMenuEntry::Command(TrayMenuCommand {
        id,
        label: label.into(),
        checked,
    }));
}

/// 업데이트 검사 상태에 맞는 트레이 메뉴 문구를 반환합니다.
pub fn update_menu_text(
    status: crate::UpdatePresentationStatus,
    language: crate::Language,
) -> &'static str {
    let key = match status {
        crate::UpdatePresentationStatus::Idle => crate::LocalizationKey::MenuUpdateCheck,
        crate::UpdatePresentationStatus::Checking => crate::LocalizationKey::UpdateChecking,
        crate::UpdatePresentationStatus::Available => crate::LocalizationKey::UpdateAvailable,
        crate::UpdatePresentationStatus::Current => crate::LocalizationKey::UpdateCurrent,
        crate::UpdatePresentationStatus::Failed => crate::LocalizationKey::UpdateFailed,
    };
    crate::localized_text(key, language)
}

fn usage_mode_menu_text(show_remaining: bool, language: crate::Language) -> &'static str {
    let key = if show_remaining {
        crate::LocalizationKey::MenuShowWeekly
    } else {
        crate::LocalizationKey::MenuShowRemaining
    };
    crate::localized_text(key, language)
}

/// 언어 선택 메뉴 항목의 문구를 반환합니다.
///
/// 각 언어 이름은 현재 UI 언어와 무관하게 항상 해당 언어의 고유 표기(endonym)로
/// 표시합니다. 예를 들어 한국어 항목은 영어 모드에서도 "한국어"로, 영어 항목은
/// 한국어 모드에서도 "English"로 표시됩니다. 이렇게 하면 현재 UI 언어를 읽지
/// 못하는 사용자도 자기 언어 항목을 찾아 전환할 수 있습니다.
///
/// 접두어("언어:"/"Language:")와 "자동" 문구는 현재 UI 언어를 따릅니다.
///
/// `option`은 메뉴 항목이 나타내는 언어 설정이고, `resolved`는 현재 적용된
/// UI 언어입니다. 결과 메뉴 문구를 반환합니다.
pub fn language_menu_label(option: LanguagePreference, resolved: Language) -> &'static str {
    let korean_ui = matches!(resolved, Language::Korean);
    match option {
        LanguagePreference::Auto => {
            if korean_ui {
                "언어: 자동"
            } else {
                "Language: automatic"
            }
        }
        LanguagePreference::Korean => {
            if korean_ui {
                "언어: 한국어"
            } else {
                "Language: 한국어"
            }
        }
        LanguagePreference::English => {
            if korean_ui {
                "언어: English"
            } else {
                "Language: English"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::usage_mode_menu_text;
    use crate::Language;

    #[test]
    fn usage_mode_menu_describes_the_available_switch() {
        assert_eq!(
            usage_mode_menu_text(false, Language::Korean),
            "남은 사용량 표시"
        );
        assert_eq!(
            usage_mode_menu_text(true, Language::Korean),
            "주간 사용량 표시"
        );
        assert_eq!(
            usage_mode_menu_text(false, Language::English),
            "Show remaining usage"
        );
        assert_eq!(
            usage_mode_menu_text(true, Language::English),
            "Show weekly usage"
        );
    }
}
