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
