use std::{
    io,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread,
};

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
                MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_NULL,
            },
        },
    },
};

use crate::{Language, LanguagePreference, StartupView, TaskbarDisplayMode};

use super::super::{
    UiSettings, MENU_AUTH_REFRESH, MENU_AUTOSTART, MENU_AUTO_AUTH_REFRESH, MENU_DIAGNOSTICS,
    MENU_EXIT, MENU_INTERVAL_1, MENU_INTERVAL_10, MENU_INTERVAL_15, MENU_INTERVAL_30,
    MENU_INTERVAL_5, MENU_LANGUAGE_AUTO, MENU_LANGUAGE_ENGLISH, MENU_LANGUAGE_KOREAN, MENU_REFRESH,
    MENU_SHOW_REMAINING, MENU_STARTUP_TRAY, MENU_STARTUP_WIDGET, MENU_TASKBAR_ALL,
    MENU_TASKBAR_PRIMARY, MENU_UPDATE_CHECK, MENU_WIDGET_VISIBLE,
};

pub(crate) const TRAY_CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

/// 셸 명령을 하나의 워커에서 직렬화하고 대기 명령을 최신 값으로 합치는 실행기입니다.
struct CoalescingWorker<C> {
    pending: Arc<Mutex<C>>,
    trigger: SyncSender<()>,
}

impl<C: Clone + Send + 'static> CoalescingWorker<C> {
    /// 느릴 수 있는 최신 명령 하나를 전용 워커에서 실행합니다.
    ///
    /// `submit`은 진행 중인 명령을 기다리지 않습니다. 대기 중인 여러 명령은 마지막 값으로 합쳐지며,
    /// 외부 호출이 정지한 경우에도 워커 스레드를 추가로 만들지 않습니다.
    fn spawn<H>(
        initial: C,
        handler_factory: impl FnOnce() -> H + Send + 'static,
    ) -> io::Result<Self>
    where
        H: FnMut(C) + 'static,
    {
        let pending = Arc::new(Mutex::new(initial));
        let worker_pending = Arc::clone(&pending);
        let (trigger, commands) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("tray-shell".to_string())
            .spawn(move || {
                let mut handler = handler_factory();
                while commands.recv().is_ok() {
                    let command = worker_pending
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .clone();
                    handler(command);
                }
            })?;
        Ok(Self { pending, trigger })
    }

    fn submit(&self, command: C) {
        *self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = command;
        let _ = self.trigger.try_send(());
    }
}

/// 트레이 워커에 전달하는 최신 아이콘 표시 명령입니다.
#[derive(Clone)]
struct TrayUpdate {
    percent: Option<f64>,
    tip: String,
    restore: bool,
}

/// Explorer 셸 호출과 UI 메시지 처리를 분리하는 비동기 트레이 아이콘입니다.
pub(crate) struct AsyncTrayIcon {
    worker: CoalescingWorker<TrayUpdate>,
}

impl AsyncTrayIcon {
    /// Explorer 셸 호출을 전용 워커에서 실행하는 트레이 아이콘을 만듭니다.
    ///
    /// `owner`는 트레이 콜백을 받을 UI 창입니다. 생성·갱신·복구·삭제 셸 호출은 UI 스레드를
    /// 차단하지 않으며, Explorer가 응답하지 않으면 마지막 명령 하나만 대기합니다.
    pub(crate) fn new(owner: HWND, percent: Option<f64>, tip: &str) -> io::Result<Self> {
        let initial = TrayUpdate {
            percent,
            tip: tip.to_string(),
            restore: true,
        };
        let owner_value = owner.0 as usize;
        let worker = CoalescingWorker::spawn(initial.clone(), move || {
            let owner = HWND(owner_value as *mut _);
            let mut tray: Option<TrayIcon> = None;
            move |update: TrayUpdate| unsafe {
                let result = match tray.as_mut() {
                    Some(tray) if update.restore => tray.restore(update.percent, &update.tip),
                    Some(tray) => tray
                        .update(update.percent, &update.tip)
                        .or_else(|_| tray.restore(update.percent, &update.tip)),
                    None => TrayIcon::new(owner, update.percent, &update.tip).map(|created| {
                        tray = Some(created);
                    }),
                };
                let _ = result;
            }
        })?;
        worker.submit(initial);
        Ok(Self { worker })
    }

    /// 최신 상태로 트레이 아이콘 갱신을 예약하고 즉시 반환합니다.
    pub(crate) fn update(&self, percent: Option<f64>, tip: &str) {
        self.submit(percent, tip, false);
    }

    /// Explorer 재시작 후 트레이 아이콘 복구를 예약하고 즉시 반환합니다.
    pub(crate) fn restore(&self, percent: Option<f64>, tip: &str) {
        self.submit(percent, tip, true);
    }

    fn submit(&self, percent: Option<f64>, tip: &str, restore: bool) {
        self.worker.submit(TrayUpdate {
            percent,
            tip: tip.to_string(),
            restore,
        });
    }
}

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
                MENU_LANGUAGE_AUTO,
                super::language_menu_label(LanguagePreference::Auto, settings.resolved_language),
                settings.language == LanguagePreference::Auto,
            )?;
            add(
                menu,
                MENU_LANGUAGE_KOREAN,
                super::language_menu_label(LanguagePreference::Korean, settings.resolved_language),
                settings.language == LanguagePreference::Korean,
            )?;
            add(
                menu,
                MENU_LANGUAGE_ENGLISH,
                super::language_menu_label(LanguagePreference::English, settings.resolved_language),
                settings.language == LanguagePreference::English,
            )?;
            add(
                menu,
                MENU_SHOW_REMAINING,
                super::usage_mode_menu_text(
                    settings.show_remaining_percent,
                    settings.resolved_language,
                ),
                false,
            )?;
            separator(menu)?;
            add(
                menu,
                MENU_DIAGNOSTICS,
                if ko { "진단" } else { "Diagnostics" },
                false,
            )?;
            add(
                menu,
                MENU_UPDATE_CHECK,
                super::update_menu_text(settings.update_status, settings.resolved_language),
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
                } else if ko {
                    "위젯 표시"
                } else {
                    "Show widget"
                },
                settings.widget_visible,
            )?;
            add(
                menu,
                MENU_TASKBAR_ALL,
                if ko {
                    "위젯: 모든 모니터"
                } else {
                    "Widget: all monitors"
                },
                settings.taskbar_display_mode == TaskbarDisplayMode::All,
            )?;
            add(
                menu,
                MENU_TASKBAR_PRIMARY,
                if ko {
                    "위젯: 주 모니터만"
                } else {
                    "Widget: primary monitor only"
                },
                settings.taskbar_display_mode == TaskbarDisplayMode::Primary,
            )?;
            separator(menu)?;
            add(menu, MENU_EXIT, if ko { "종료" } else { "Exit" }, false)?;
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            let _ = SetForegroundWindow(owner);
            let command = TrackPopupMenu(
                menu,
                TPM_NONOTIFY | TPM_RETURNCMD | TPM_RIGHTBUTTON,
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

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    use super::CoalescingWorker;

    #[test]
    fn tray_worker_submission_never_waits_for_a_blocked_shell_call() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (handled_tx, handled_rx) = mpsc::channel();
        let worker = CoalescingWorker::spawn(0_u32, move || {
            move |value| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                handled_tx.send(value).unwrap();
            }
        })
        .unwrap();

        let delayed_release = release_tx.clone();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            delayed_release.send(()).unwrap();
        });
        let started = Instant::now();
        worker.submit(1);
        assert!(started.elapsed() < Duration::from_millis(100));
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release.join().unwrap();
        assert_eq!(handled_rx.recv_timeout(Duration::from_secs(1)), Ok(1));

        worker.submit(2);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(handled_rx.recv_timeout(Duration::from_secs(1)), Ok(2));
    }
}
