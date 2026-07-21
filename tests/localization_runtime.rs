use codex_usage_monitor::{localized_text, Language, LocalizationKey};

#[test]
fn every_localization_key_has_nonempty_korean_and_english_text() {
    for key in LocalizationKey::ALL {
        for language in [Language::Korean, Language::English] {
            assert!(
                !localized_text(*key, language).trim().is_empty(),
                "{key:?} {language:?}"
            );
        }
    }
}
