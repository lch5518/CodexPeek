use std::io;

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, POINT},
        UI::{
            Shell::{
                Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIcon, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
                PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, MF_CHECKED, MF_SEPARATOR,
                MF_STRING, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_NULL,
            },
        },
    },
};

use crate::{DisplayMode, Language, LanguagePreference, StartupView};

use super::super::{
    UiSettings, MENU_ALWAYS_ON_TOP, MENU_AUTH_REFRESH, MENU_AUTOSTART, MENU_AUTO_AUTH_REFRESH,
    MENU_DIAGNOSTICS, MENU_DISPLAY_FLOATING, MENU_DISPLAY_TASKBAR, MENU_EXIT, MENU_INTERVAL_1,
    MENU_INTERVAL_10, MENU_INTERVAL_15, MENU_INTERVAL_30, MENU_INTERVAL_5, MENU_LANGUAGE_AUTO,
    MENU_LANGUAGE_ENGLISH, MENU_LANGUAGE_KOREAN, MENU_POSITION_RESET, MENU_REFRESH,
    MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET, MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
};

pub(crate) const TRAY_CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

/// 알림 영역 아이콘과 동적 미터 아이콘의 소유자입니다.
pub(crate) struct TrayIcon {
    owner: HWND,
    icon: HICON,
    added: bool,
}

impl TrayIcon {
    pub(crate) unsafe fn new(owner: HWND, percent: Option<f64>, tip: &str) -> io::Result<Self> {
        let icon = meter_icon(percent)?;
        let mut tray = Self {
            owner,
            icon,
            added: false,
        };
        tray.add(tip)?;
        Ok(tray)
    }

    pub(crate) unsafe fn restore(&mut self, percent: Option<f64>, tip: &str) -> io::Result<()> {
        self.added = false;
        self.replace_icon(percent)?;
        self.add(tip)
    }

    pub(crate) unsafe fn update(&mut self, percent: Option<f64>, tip: &str) -> io::Result<()> {
        self.replace_icon(percent)?;
        let data = notify_data(self.owner, self.icon, tip);
        if Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(crate) unsafe fn show_menu(owner: HWND, settings: &UiSettings) -> Option<u16> {
        let menu = CreatePopupMenu().ok()?;
        let ko = settings.resolved_language == Language::Korean;
        let result = (|| {
            add(
                menu,
                MENU_REFRESH,
                if ko { "지금 갱신" } else { "Refresh now" },
                false,
            )?;
            separator(menu)?;
            add(
                menu,
                MENU_DISPLAY_TASKBAR,
                if ko {
                    "표시: 작업 표시줄"
                } else {
                    "Display: taskbar"
                },
                settings.display_mode == DisplayMode::Taskbar,
            )?;
            add(
                menu,
                MENU_DISPLAY_FLOATING,
                if ko {
                    "표시: 부동 창"
                } else {
                    "Display: floating"
                },
                settings.display_mode == DisplayMode::Floating,
            )?;
            for (id, minutes) in [
                (MENU_INTERVAL_1, 1),
                (MENU_INTERVAL_5, 5),
                (MENU_INTERVAL_10, 10),
                (MENU_INTERVAL_15, 15),
                (MENU_INTERVAL_30, 30),
            ] {
                add(
                    menu,
                    id,
                    &if ko {
                        format!("갱신 간격: {minutes}분")
                    } else {
                        format!("Refresh interval: {minutes} min")
                    },
                    settings.refresh_interval_minutes == minutes,
                )?;
            }
            separator(menu)?;
            add(
                menu,
                MENU_AUTOSTART,
                if ko {
                    "Windows 시작 시 실행"
                } else {
                    "Start with Windows"
                },
                settings.start_with_windows,
            )?;
            add(
                menu,
                MENU_STARTUP_WIDGET,
                if ko {
                    "시작: 위젯 표시"
                } else {
                    "Startup: show widget"
                },
                settings.startup_view == StartupView::Widget,
            )?;
            add(
                menu,
                MENU_STARTUP_TRAY,
                if ko {
                    "시작: 트레이만"
                } else {
                    "Startup: tray only"
                },
                settings.startup_view == StartupView::TrayOnly,
            )?;
            add(
                menu,
                MENU_AUTH_REFRESH,
                if ko {
                    "인증 갱신"
                } else {
                    "Refresh authentication"
                },
                false,
            )?;
            add(
                menu,
                MENU_AUTO_AUTH_REFRESH,
                if ko {
                    "자동 인증 갱신"
                } else {
                    "Automatic authentication refresh"
                },
                settings.auto_auth_refresh,
            )?;
            add(
                menu,
                MENU_ALWAYS_ON_TOP,
                if ko { "항상 위" } else { "Always on top" },
                settings.always_on_top,
            )?;
            add(
                menu,
                MENU_LANGUAGE_AUTO,
                if ko {
                    "언어: 자동"
                } else {
                    "Language: automatic"
                },
                settings.language == LanguagePreference::Auto,
            )?;
            add(
                menu,
                MENU_LANGUAGE_KOREAN,
                if ko {
                    "언어: 한국어"
                } else {
                    "Language: Korean"
                },
                settings.language == LanguagePreference::Korean,
            )?;
            add(
                menu,
                MENU_LANGUAGE_ENGLISH,
                if ko {
                    "언어: 영어"
                } else {
                    "Language: English"
                },
                settings.language == LanguagePreference::English,
            )?;
            separator(menu)?;
            add(
                menu,
                MENU_POSITION_RESET,
                if ko {
                    "위치 초기화"
                } else {
                    "Reset position"
                },
                false,
            )?;
            add(
                menu,
                MENU_DIAGNOSTICS,
                if ko { "진단" } else { "Diagnostics" },
                false,
            )?;
            add(
                menu,
                MENU_UPDATE_CHECK,
                if settings.update_available {
                    if ko {
                        "새 업데이트를 사용할 수 있습니다"
                    } else {
                        "An update is available"
                    }
                } else if ko {
                    "업데이트 확인"
                } else {
                    "Check for updates"
                },
                false,
            )?;
            add(
                menu,
                MENU_WIDGET_VISIBLE,
                if settings.widget_visible {
                    if ko {
                        "위젯 숨기기"
                    } else {
                        "Hide widget"
                    }
                } else {
                    if ko {
                        "위젯 표시"
                    } else {
                        "Show widget"
                    }
                },
                settings.widget_visible,
            )?;
            separator(menu)?;
            add(menu, MENU_EXIT, if ko { "종료" } else { "Exit" }, false)?;
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            let _ = SetForegroundWindow(owner);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                owner,
                None,
            );
            (command.0 > 0).then_some(command.0 as u16)
        })();
        let _ = PostMessageW(Some(owner), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
        result
    }

    unsafe fn add(&mut self, tip: &str) -> io::Result<()> {
        let mut data = notify_data(self.owner, self.icon, tip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            return Err(io::Error::last_os_error());
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        self.added = true;
        Ok(())
    }

    unsafe fn replace_icon(&mut self, percent: Option<f64>) -> io::Result<()> {
        let next = meter_icon(percent)?;
        let previous = std::mem::replace(&mut self.icon, next);
        let _ = DestroyIcon(previous);
        Ok(())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            if self.added {
                let data = notify_data(self.owner, self.icon, "");
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            let _ = DestroyIcon(self.icon);
        }
    }
}

fn notify_data(owner: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: owner,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        hIcon: icon,
        ..Default::default()
    };
    for (target, source) in data.szTip.iter_mut().take(127).zip(tip.encode_utf16()) {
        *target = source;
    }
    data
}

unsafe fn meter_icon(percent: Option<f64>) -> io::Result<HICON> {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 16;
    const BYTES_PER_ROW: usize = 2;
    let mut xor = [0_u8; HEIGHT * BYTES_PER_ROW];
    let and = [0_u8; HEIGHT * BYTES_PER_ROW];
    let percent = percent.filter(|value| value.is_finite()).unwrap_or(0.0);
    let fill = ((percent.clamp(0.0, 100.0) / 100.0) * 12.0).round() as usize;
    for y in 1..15 {
        for x in 2..14 {
            let border = x == 2 || x == 13 || y == 1 || y == 14;
            let filled = y >= 14_usize.saturating_sub(fill);
            if border || filled {
                xor[y * BYTES_PER_ROW + x / 8] |= 1 << (7 - x % 8);
            }
        }
    }
    CreateIcon(
        None,
        WIDTH as i32,
        HEIGHT as i32,
        1,
        1,
        and.as_ptr(),
        xor.as_ptr(),
    )
    .map_err(|_| io::Error::last_os_error())
}

unsafe fn add(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    id: u16,
    text: &str,
    checked: bool,
) -> Option<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let flags = MF_STRING
        | if checked {
            MF_CHECKED
        } else {
            Default::default()
        };
    AppendMenuW(menu, flags, usize::from(id), PCWSTR(wide.as_ptr()))
        .ok()
        .map(|_| ())
}

unsafe fn separator(menu: windows::Win32::UI::WindowsAndMessaging::HMENU) -> Option<()> {
    AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
        .ok()
        .map(|_| ())
}
