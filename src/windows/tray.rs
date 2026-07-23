//! 알림 영역 아이콘과 메뉴의 Windows 구현입니다.

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::{TrayIcon, TRAY_CALLBACK};

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
pub fn language_menu_label(
    option: crate::LanguagePreference,
    resolved: crate::Language,
) -> &'static str {
    let korean_ui = matches!(resolved, crate::Language::Korean);
    match option {
        crate::LanguagePreference::Auto => {
            if korean_ui {
                "언어: 자동"
            } else {
                "Language: automatic"
            }
        }
        crate::LanguagePreference::Korean => {
            if korean_ui {
                "언어: 한국어"
            } else {
                "Language: 한국어"
            }
        }
        crate::LanguagePreference::English => {
            if korean_ui {
                "언어: English"
            } else {
                "Language: English"
            }
        }
    }
}
