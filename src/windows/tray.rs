//! 알림 영역 아이콘과 메뉴의 Windows 구현입니다.

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::{AsyncTrayIcon, TrayIcon, TRAY_CALLBACK};

use crate::{Language, LanguagePreference, StartupView, TaskbarDisplayMode};

use super::{
    UiSettings, MENU_AUTH_REFRESH, MENU_AUTOSTART, MENU_AUTO_AUTH_REFRESH, MENU_DIAGNOSTICS,
    MENU_EXIT, MENU_INTERVAL_1, MENU_INTERVAL_10, MENU_INTERVAL_15, MENU_INTERVAL_30,
    MENU_INTERVAL_5, MENU_LANGUAGE_ARABIC, MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH,
    MENU_LANGUAGE_FRENCH, MENU_LANGUAGE_GERMAN, MENU_LANGUAGE_HINDI, MENU_LANGUAGE_INDONESIAN,
    MENU_LANGUAGE_JAPANESE, MENU_LANGUAGE_KOREAN, MENU_LANGUAGE_PORTUGUESE_BRAZIL,
    MENU_LANGUAGE_SPANISH, MENU_LANGUAGE_TURKISH, MENU_LANGUAGE_VIETNAMESE, MENU_LOGIN,
    MENU_REFRESH, MENU_SHOW_REMAINING, MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET, MENU_TASKBAR_ALL,
    MENU_TASKBAR_PRIMARY, MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
};

/// 트레이 메뉴에 표시할 순수 항목 모델입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayMenuEntry {
    /// 선택 가능한 명령 항목입니다.
    Command(TrayMenuCommand),
    /// 한 단계 아래에 표시할 하위 메뉴입니다.
    Submenu(TraySubmenu),
    /// 메뉴 구획을 나누는 구분선입니다.
    Separator,
}

/// Win32 팝업 메뉴에 연결할 하위 메뉴의 표시 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraySubmenu {
    /// 상위 메뉴에 표시할 지역화 문구입니다.
    pub label: String,
    /// 하위 메뉴에 순서대로 표시할 항목입니다.
    pub entries: Vec<TrayMenuEntry>,
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
    let mut refresh_interval_entries = Vec::new();
    for (id, minutes) in [
        (MENU_INTERVAL_1, 1),
        (MENU_INTERVAL_5, 5),
        (MENU_INTERVAL_10, 10),
        (MENU_INTERVAL_15, 15),
        (MENU_INTERVAL_30, 30),
    ] {
        push_command(
            &mut refresh_interval_entries,
            id,
            crate::localization::localized_refresh_interval_choice_text(minutes, language),
            settings.refresh_interval_minutes == minutes,
        );
    }
    push_submenu(
        &mut entries,
        crate::localized_text(crate::LocalizationKey::MenuRefreshInterval, language),
        refresh_interval_entries,
    );
    entries.push(TrayMenuEntry::Separator);
    push_command(
        &mut entries,
        MENU_AUTOSTART,
        crate::localized_text(crate::LocalizationKey::MenuAutostart, language),
        settings.start_with_windows,
    );
    let mut startup_view_entries = Vec::new();
    push_command(
        &mut startup_view_entries,
        MENU_STARTUP_WIDGET,
        crate::localized_text(crate::LocalizationKey::MenuStartupWidget, language),
        settings.startup_view == StartupView::Widget,
    );
    push_command(
        &mut startup_view_entries,
        MENU_STARTUP_TRAY,
        crate::localized_text(crate::LocalizationKey::MenuStartupTrayOnly, language),
        settings.startup_view == StartupView::TrayOnly,
    );
    push_submenu(
        &mut entries,
        crate::localized_text(crate::LocalizationKey::MenuStartupView, language),
        startup_view_entries,
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
    let mut language_entries = Vec::new();
    push_command(
        &mut language_entries,
        MENU_LANGUAGE_AUTO,
        crate::localized_text(
            crate::LocalizationKey::MenuLanguageAutomaticChoice,
            language,
        ),
        settings.language == LanguagePreference::Auto,
    );
    for (id, preference) in LANGUAGE_MENU_OPTIONS {
        push_command(
            &mut language_entries,
            *id,
            language_menu_choice_label(*preference, language),
            settings.language == *preference,
        );
    }
    push_submenu(
        &mut entries,
        crate::localized_text(crate::LocalizationKey::MenuLanguage, language),
        language_entries,
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
    let mut widget_placement_entries = Vec::new();
    push_command(
        &mut widget_placement_entries,
        MENU_TASKBAR_ALL,
        crate::localized_text(crate::LocalizationKey::MenuTaskbarAllChoice, language),
        settings.taskbar_display_mode == TaskbarDisplayMode::All,
    );
    push_command(
        &mut widget_placement_entries,
        MENU_TASKBAR_PRIMARY,
        crate::localized_text(crate::LocalizationKey::MenuTaskbarPrimaryChoice, language),
        settings.taskbar_display_mode == TaskbarDisplayMode::Primary,
    );
    push_submenu(
        &mut entries,
        crate::localized_text(crate::LocalizationKey::MenuWidgetPlacement, language),
        widget_placement_entries,
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

fn push_submenu(
    entries: &mut Vec<TrayMenuEntry>,
    label: impl Into<String>,
    submenu_entries: Vec<TrayMenuEntry>,
) {
    entries.push(TrayMenuEntry::Submenu(TraySubmenu {
        label: label.into(),
        entries: submenu_entries,
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
pub fn language_menu_label(option: LanguagePreference, resolved: Language) -> String {
    match option {
        LanguagePreference::Auto => auto_language_menu_label(resolved).to_string(),
        LanguagePreference::Korean => language_menu_endonym_label(resolved, "한국어"),
        LanguagePreference::English => language_menu_endonym_label(resolved, "English"),
        LanguagePreference::Spanish => language_menu_endonym_label(resolved, "Español"),
        LanguagePreference::PortugueseBrazil => {
            language_menu_endonym_label(resolved, "Português (Brasil)")
        }
        LanguagePreference::Indonesian => language_menu_endonym_label(resolved, "Bahasa Indonesia"),
        LanguagePreference::Japanese => language_menu_endonym_label(resolved, "日本語"),
        LanguagePreference::Hindi => language_menu_endonym_label(resolved, "हिन्दी"),
        LanguagePreference::German => language_menu_endonym_label(resolved, "Deutsch"),
        LanguagePreference::French => language_menu_endonym_label(resolved, "Français"),
        LanguagePreference::Vietnamese => language_menu_endonym_label(resolved, "Tiếng Việt"),
        LanguagePreference::Turkish => language_menu_endonym_label(resolved, "Türkçe"),
        LanguagePreference::Arabic => language_menu_endonym_label(resolved, "العربية"),
    }
}

fn language_menu_choice_label(option: LanguagePreference, resolved: Language) -> &'static str {
    match option {
        LanguagePreference::Auto => crate::localized_text(
            crate::LocalizationKey::MenuLanguageAutomaticChoice,
            resolved,
        ),
        LanguagePreference::Korean => "한국어",
        LanguagePreference::English => "English",
        LanguagePreference::Spanish => "Español",
        LanguagePreference::PortugueseBrazil => "Português (Brasil)",
        LanguagePreference::Indonesian => "Bahasa Indonesia",
        LanguagePreference::Japanese => "日本語",
        LanguagePreference::Hindi => "हिन्दी",
        LanguagePreference::German => "Deutsch",
        LanguagePreference::French => "Français",
        LanguagePreference::Vietnamese => "Tiếng Việt",
        LanguagePreference::Turkish => "Türkçe",
        LanguagePreference::Arabic => "العربية",
    }
}

const LANGUAGE_MENU_OPTIONS: &[(u16, LanguagePreference)] = &[
    (MENU_LANGUAGE_KOREAN, LanguagePreference::Korean),
    (MENU_LANGUAGE_ENGLISH, LanguagePreference::English),
    (MENU_LANGUAGE_SPANISH, LanguagePreference::Spanish),
    (
        MENU_LANGUAGE_PORTUGUESE_BRAZIL,
        LanguagePreference::PortugueseBrazil,
    ),
    (MENU_LANGUAGE_INDONESIAN, LanguagePreference::Indonesian),
    (MENU_LANGUAGE_JAPANESE, LanguagePreference::Japanese),
    (MENU_LANGUAGE_HINDI, LanguagePreference::Hindi),
    (MENU_LANGUAGE_GERMAN, LanguagePreference::German),
    (MENU_LANGUAGE_FRENCH, LanguagePreference::French),
    (MENU_LANGUAGE_VIETNAMESE, LanguagePreference::Vietnamese),
    (MENU_LANGUAGE_TURKISH, LanguagePreference::Turkish),
    (MENU_LANGUAGE_ARABIC, LanguagePreference::Arabic),
];

fn auto_language_menu_label(resolved: Language) -> &'static str {
    match resolved {
        Language::Korean => "언어: 자동",
        Language::English => "Language: automatic",
        Language::Spanish => "Idioma: automático",
        Language::PortugueseBrazil => "Idioma: automático",
        Language::Indonesian => "Bahasa: otomatis",
        Language::Japanese => "言語: 自動",
        Language::Hindi => "भाषा: स्वतः",
        Language::German => "Sprache: automatisch",
        Language::French => "Langue : automatique",
        Language::Vietnamese => "Ngôn ngữ: tự động",
        Language::Turkish => "Dil: otomatik",
        Language::Arabic => "اللغة: تلقائي",
    }
}

fn language_menu_endonym_label(resolved: Language, endonym: &'static str) -> String {
    format!("{} {endonym}", language_menu_prefix(resolved))
}

fn language_menu_prefix(resolved: Language) -> &'static str {
    match resolved {
        Language::Korean => "언어:",
        Language::English => "Language:",
        Language::Spanish | Language::PortugueseBrazil => "Idioma:",
        Language::Indonesian => "Bahasa:",
        Language::Japanese => "言語:",
        Language::Hindi => "भाषा:",
        Language::German => "Sprache:",
        Language::French => "Langue :",
        Language::Vietnamese => "Ngôn ngữ:",
        Language::Turkish => "Dil:",
        Language::Arabic => "اللغة:",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        language_menu_label, tray_menu_entries, usage_mode_menu_text, TrayMenuEntry,
        LANGUAGE_MENU_OPTIONS,
    };
    use crate::windows::{
        UiSettings, MENU_LANGUAGE_ARABIC, MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH,
        MENU_LANGUAGE_FRENCH, MENU_LANGUAGE_GERMAN, MENU_LANGUAGE_HINDI, MENU_LANGUAGE_INDONESIAN,
        MENU_LANGUAGE_JAPANESE, MENU_LANGUAGE_KOREAN, MENU_LANGUAGE_PORTUGUESE_BRAZIL,
        MENU_LANGUAGE_SPANISH, MENU_LANGUAGE_TURKISH, MENU_LANGUAGE_VIETNAMESE,
    };
    use crate::{
        Language, LanguagePreference, StartupView, TaskbarDisplayMode, UpdatePresentationStatus,
    };

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

    #[test]
    fn language_menu_options_include_all_preferences_once_in_order() {
        let settings = tray_settings(LanguagePreference::Turkish, Language::English);
        let commands = language_commands(&settings);
        let expected = expected_language_options();

        assert_eq!(
            LANGUAGE_MENU_OPTIONS,
            &expected[1..],
            "Auto stays separate and concrete language order stays stable"
        );
        assert_eq!(
            commands.iter().map(|command| command.0).collect::<Vec<_>>(),
            expected.iter().map(|option| option.0).collect::<Vec<_>>()
        );
        assert_eq!(
            commands
                .iter()
                .map(|command| command.1.as_str())
                .collect::<Vec<_>>(),
            [
                "Automatic",
                "한국어",
                "English",
                "Español",
                "Português (Brasil)",
                "Bahasa Indonesia",
                "日本語",
                "हिन्दी",
                "Deutsch",
                "Français",
                "Tiếng Việt",
                "Türkçe",
                "العربية",
            ]
        );
        assert_eq!(
            commands
                .iter()
                .filter_map(|command| command.2.then_some(command.0))
                .collect::<Vec<_>>(),
            vec![MENU_LANGUAGE_TURKISH]
        );
    }

    #[test]
    fn language_menu_check_state_follows_each_preference() {
        for (id, preference) in expected_language_options() {
            let settings = tray_settings(preference, Language::Korean);
            let checked = language_commands(&settings)
                .into_iter()
                .filter_map(|command| command.2.then_some(command.0))
                .collect::<Vec<_>>();

            assert_eq!(checked, vec![id], "{preference:?}");
        }
    }

    #[test]
    fn auto_language_menu_label_uses_the_resolved_language() {
        let cases = [
            (Language::Korean, "언어: 자동"),
            (Language::English, "Language: automatic"),
            (Language::Spanish, "Idioma: automático"),
            (Language::PortugueseBrazil, "Idioma: automático"),
            (Language::Indonesian, "Bahasa: otomatis"),
            (Language::Japanese, "言語: 自動"),
            (Language::Hindi, "भाषा: स्वतः"),
            (Language::German, "Sprache: automatisch"),
            (Language::French, "Langue : automatique"),
            (Language::Vietnamese, "Ngôn ngữ: tự động"),
            (Language::Turkish, "Dil: otomatik"),
            (Language::Arabic, "اللغة: تلقائي"),
        ];

        for (language, expected) in cases {
            assert_eq!(
                language_menu_label(LanguagePreference::Auto, language),
                expected,
                "{language:?}"
            );
        }
    }

    fn expected_language_options() -> [(u16, LanguagePreference); 13] {
        [
            (MENU_LANGUAGE_AUTO, LanguagePreference::Auto),
            (MENU_LANGUAGE_KOREAN, LanguagePreference::Korean),
            (MENU_LANGUAGE_ENGLISH, LanguagePreference::English),
            (MENU_LANGUAGE_SPANISH, LanguagePreference::Spanish),
            (
                MENU_LANGUAGE_PORTUGUESE_BRAZIL,
                LanguagePreference::PortugueseBrazil,
            ),
            (MENU_LANGUAGE_INDONESIAN, LanguagePreference::Indonesian),
            (MENU_LANGUAGE_JAPANESE, LanguagePreference::Japanese),
            (MENU_LANGUAGE_HINDI, LanguagePreference::Hindi),
            (MENU_LANGUAGE_GERMAN, LanguagePreference::German),
            (MENU_LANGUAGE_FRENCH, LanguagePreference::French),
            (MENU_LANGUAGE_VIETNAMESE, LanguagePreference::Vietnamese),
            (MENU_LANGUAGE_TURKISH, LanguagePreference::Turkish),
            (MENU_LANGUAGE_ARABIC, LanguagePreference::Arabic),
        ]
    }

    fn tray_settings(language: LanguagePreference, resolved_language: Language) -> UiSettings {
        UiSettings {
            widget_visible: true,
            refresh_interval_minutes: 15,
            start_with_windows: false,
            startup_view: StartupView::Widget,
            auto_auth_refresh: false,
            language,
            resolved_language,
            taskbar_offset: 0,
            taskbar_display_mode: TaskbarDisplayMode::All,
            update_status: UpdatePresentationStatus::Idle,
            show_remaining_percent: false,
            login_required: false,
        }
    }

    fn language_commands(settings: &UiSettings) -> Vec<(u16, String, bool)> {
        let ids = expected_language_options()
            .into_iter()
            .map(|option| option.0)
            .collect::<Vec<_>>();

        fn collect(
            entries: &[TrayMenuEntry],
            ids: &[u16],
            commands: &mut Vec<(u16, String, bool)>,
        ) {
            for entry in entries {
                match entry {
                    TrayMenuEntry::Command(command) if ids.contains(&command.id) => {
                        commands.push((command.id, command.label.clone(), command.checked));
                    }
                    TrayMenuEntry::Submenu(submenu) => {
                        collect(&submenu.entries, ids, commands);
                    }
                    _ => {}
                }
            }
        }

        let entries = tray_menu_entries(settings);
        let mut commands = Vec::new();
        collect(&entries, &ids, &mut commands);
        commands
    }
}
