use codex_usage_monitor::{localized_text, Language, LocalizationKey};

#[test]
fn every_required_localization_key_has_nonempty_korean_and_english_text() {
    let required_keys = [
        LocalizationKey::Polling,
        LocalizationKey::Refreshing,
        LocalizationKey::Stale,
        LocalizationKey::Unavailable,
        LocalizationKey::MenuRefresh,
        LocalizationKey::MenuRefreshInterval,
        LocalizationKey::MenuAutostart,
        LocalizationKey::MenuStartupView,
        LocalizationKey::MenuStartupWidget,
        LocalizationKey::MenuStartupTrayOnly,
        LocalizationKey::MenuAuthRefresh,
        LocalizationKey::MenuLanguage,
        LocalizationKey::MenuDiagnostics,
        LocalizationKey::MenuUpdateCheck,
        LocalizationKey::MenuSettings,
        LocalizationKey::MenuExit,
        LocalizationKey::MenuShowWidget,
        LocalizationKey::MenuHideWidget,
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
    ];

    assert_eq!(LocalizationKey::ALL.len(), required_keys.len());
    for required_key in required_keys {
        assert!(LocalizationKey::ALL.contains(&required_key));
    }
    for key in LocalizationKey::ALL {
        for language in [Language::Korean, Language::English] {
            assert!(
                !localized_text(*key, language).trim().is_empty(),
                "{key:?} {language:?}"
            );
        }
    }
}
