//! 알림 영역 아이콘과 메뉴의 Windows 구현입니다.

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::{TrayIcon, TRAY_CALLBACK};
