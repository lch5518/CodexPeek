use codex_usage_monitor::{localized_text, Language, LocalizationKey};

#[test]
fn every_required_localization_key_has_nonempty_text_for_every_language() {
    let required_keys = [
        LocalizationKey::Polling,
        LocalizationKey::Refreshing,
        LocalizationKey::Stale,
        LocalizationKey::Unavailable,
        LocalizationKey::MenuRefresh,
        LocalizationKey::MenuRefreshNow,
        LocalizationKey::MenuRefreshInterval,
        LocalizationKey::MenuAutostart,
        LocalizationKey::MenuStartupView,
        LocalizationKey::MenuStartupWidget,
        LocalizationKey::MenuStartupTrayOnly,
        LocalizationKey::MenuAuthRefresh,
        LocalizationKey::MenuLogin,
        LocalizationKey::MenuAuthRefreshNow,
        LocalizationKey::MenuLanguage,
        LocalizationKey::MenuLanguageAutomaticChoice,
        LocalizationKey::MenuDiagnostics,
        LocalizationKey::MenuUpdateCheck,
        LocalizationKey::MenuSettings,
        LocalizationKey::MenuExit,
        LocalizationKey::MenuShowWidget,
        LocalizationKey::MenuHideWidget,
        LocalizationKey::MenuStartupWidgetChoice,
        LocalizationKey::MenuStartupTrayOnlyChoice,
        LocalizationKey::MenuTaskbarAll,
        LocalizationKey::MenuTaskbarPrimary,
        LocalizationKey::MenuWidgetPlacement,
        LocalizationKey::MenuTaskbarAllChoice,
        LocalizationKey::MenuTaskbarPrimaryChoice,
        LocalizationKey::UpdateAvailable,
        LocalizationKey::UpdateCurrent,
        LocalizationKey::UpdateChecking,
        LocalizationKey::UpdateFailed,
        LocalizationKey::WindowTitle,
        LocalizationKey::SettingsTitle,
        LocalizationKey::DiagnosticsTitle,
        LocalizationKey::PrimaryWindowLabel,
        LocalizationKey::SecondaryWindowLabel,
        LocalizationKey::DiagnosticCli,
        LocalizationKey::DiagnosticRpc,
        LocalizationKey::DiagnosticLogin,
        LocalizationKey::DiagnosticSettings,
        LocalizationKey::DiagnosticProxy,
        LocalizationKey::DiagnosticTaskbar,
        LocalizationKey::MenuShowRemaining,
        LocalizationKey::MenuShowWeekly,
        LocalizationKey::MenuUsageProfiles,
        LocalizationKey::MenuAddUsageProfile,
        LocalizationKey::MenuManageUsageProfiles,
        LocalizationKey::UsageProfileSystem,
        LocalizationKey::UsageProfileDisplayed,
        LocalizationKey::UsageProfileCliUnchanged,
        LocalizationKey::UsageProfileLoginRequired,
        LocalizationKey::UsageProfileAddTitle,
        LocalizationKey::UsageProfileConfirmBrowserAccount,
        LocalizationKey::UsageProfileRename,
        LocalizationKey::UsageProfileLogin,
        LocalizationKey::UsageProfileLogout,
        LocalizationKey::UsageProfileDelete,
        LocalizationKey::UsageProfileDeleteConfirm,
        LocalizationKey::UsageProfileLimitReached,
        LocalizationKey::UsageProfileClose,
        LocalizationKey::UsageProfileName,
        LocalizationKey::UsageProfileInvalidLabel,
        LocalizationKey::UsageProfileCliIdeUnchanged,
        LocalizationKey::UsageProfileDeleteIrrecoverable,
        LocalizationKey::UsageProfileOperationFailed,
        LocalizationKey::UsageProfileCancel,
    ];
    let required_languages = [
        Language::Korean,
        Language::English,
        Language::Spanish,
        Language::PortugueseBrazil,
        Language::Indonesian,
        Language::Japanese,
        Language::Hindi,
        Language::German,
        Language::French,
        Language::Vietnamese,
        Language::Turkish,
        Language::Arabic,
    ];

    assert_eq!(LocalizationKey::ALL.len(), 68);
    assert_eq!(LocalizationKey::ALL.len(), required_keys.len());
    for required_key in required_keys {
        assert!(LocalizationKey::ALL.contains(&required_key));
    }
    assert_eq!(Language::ALL, required_languages);
    for key in LocalizationKey::ALL {
        for language in Language::ALL {
            assert!(
                !localized_text(*key, *language).trim().is_empty(),
                "{key:?} {language:?}"
            );
        }
    }
}

#[test]
fn korean_and_english_contracts_stay_stable() {
    assert_eq!(
        localized_text(LocalizationKey::Polling, Language::Korean),
        "자동 갱신 중"
    );
    assert_eq!(
        localized_text(LocalizationKey::Polling, Language::English),
        "Polling"
    );
    assert_eq!(
        localized_text(LocalizationKey::SettingsTitle, Language::Korean),
        "Codex 사용량 모니터 설정"
    );
    assert_eq!(
        localized_text(LocalizationKey::SettingsTitle, Language::English),
        "Codex Usage Monitor Settings"
    );
    assert_eq!(
        localized_text(LocalizationKey::DiagnosticSettings, Language::Korean),
        "설정을 읽거나 검증할 수 없습니다"
    );
    assert_eq!(
        localized_text(LocalizationKey::DiagnosticSettings, Language::English),
        "Settings could not be read or validated"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuShowWeekly, Language::Korean),
        "주간 사용량 표시"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuShowWeekly, Language::English),
        "Show weekly usage"
    );
    assert_eq!(
        localized_text(
            LocalizationKey::MenuLanguageAutomaticChoice,
            Language::Korean
        ),
        "자동"
    );
    assert_eq!(
        localized_text(
            LocalizationKey::MenuLanguageAutomaticChoice,
            Language::English
        ),
        "Automatic"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuWidgetPlacement, Language::Korean),
        "위젯 위치"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuWidgetPlacement, Language::English),
        "Widget placement"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuUsageProfiles, Language::Korean),
        "사용량 프로필"
    );
    assert_eq!(
        localized_text(LocalizationKey::MenuUsageProfiles, Language::English),
        "Usage profiles"
    );
    assert_eq!(
        localized_text(LocalizationKey::UsageProfileCliUnchanged, Language::English),
        "Codex CLI sign-in is unchanged"
    );
    assert_eq!(
        localized_text(LocalizationKey::UsageProfileCancel, Language::Korean),
        "취소"
    );
    assert_eq!(
        localized_text(LocalizationKey::UsageProfileCancel, Language::English),
        "Cancel"
    );
}
