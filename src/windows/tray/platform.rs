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
                PostMessageW, SetForegroundWindow, TrackPopupMenu, HICON, MF_CHECKED, MF_DISABLED,
                MF_GRAYED, MF_SEPARATOR, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
                WM_APP, WM_NULL,
            },
        },
    },
};

use super::{super::UiSettings, TrayMenuEntry};

pub(crate) const TRAY_CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

/// 셸 명령을 하나의 워커에서 직렬화하고 대기 명령을 최신 값으로 합치는 실행기입니다.
struct CoalescingWorker<C> {
    pending: Arc<Mutex<C>>,
    trigger: SyncSender<()>,
    shutdown: Arc<Mutex<Option<SyncSender<()>>>>,
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
        let shutdown = Arc::new(Mutex::new(None::<SyncSender<()>>));
        let worker_shutdown = Arc::clone(&shutdown);
        let (trigger, commands) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("tray-shell".to_string())
            .spawn(move || {
                let mut handler = Some(handler_factory());
                while commands.recv().is_ok() {
                    if let Some(completion) = worker_shutdown
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take()
                    {
                        drop(handler.take());
                        let _ = completion.send(());
                        break;
                    }
                    let command = worker_pending
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .clone();
                    handler.as_mut().expect("handler exists before shutdown")(command);
                }
            })?;
        Ok(Self {
            pending,
            trigger,
            shutdown,
        })
    }

    fn submit(&self, command: C) {
        *self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = command;
        let _ = self.trigger.try_send(());
    }

    /// 현재 셸 호출이 끝나고 워커가 보유 리소스를 해제했을 때 완료 신호를 반환합니다.
    fn begin_shutdown(&self) -> mpsc::Receiver<()> {
        let (completion, receiver) = mpsc::sync_channel(1);
        *self
            .shutdown
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(completion);
        let _ = self.trigger.try_send(());
        receiver
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

    /// 아이콘 삭제를 워커에서 완료한 뒤 수신기로 알립니다.
    ///
    /// 반환된 수신기는 `TrayIcon`의 drop이 끝난 후에만 값을 받습니다. 호출자는 owner 창을
    /// 파괴하기 전에 이 신호를 확인해야 합니다.
    pub(crate) fn begin_shutdown(&self) -> mpsc::Receiver<()> {
        self.worker.begin_shutdown()
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

    pub(crate) unsafe fn show_menu(
        owner: HWND,
        settings: &UiSettings,
        reset_credits_text: Option<&str>,
    ) -> Option<u16> {
        let menu = CreatePopupMenu().ok()?;
        let result = (|| {
            if let Some(text) = reset_credits_text {
                add_info_banner(menu, text)?;
                separator(menu)?;
            }
            for entry in super::tray_menu_entries(settings) {
                match entry {
                    TrayMenuEntry::Command(command) => {
                        add(menu, command.id, &command.label, command.checked)?;
                    }
                    TrayMenuEntry::Separator => separator(menu)?,
                }
            }
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

/// 클릭할 수 없는 정보 배너 항목을 메뉴에 추가합니다.
///
/// 명령 식별자로 0을 사용하므로 선택되어도 어떤 동작도 발생하지 않습니다.
unsafe fn add_info_banner(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    text: &str,
) -> Option<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    AppendMenuW(
        menu,
        MF_STRING | MF_DISABLED | MF_GRAYED,
        0,
        PCWSTR(wide.as_ptr()),
    )
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

    #[test]
    fn tray_worker_shutdown_acknowledges_only_after_the_active_shell_call_returns() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = CoalescingWorker::spawn(0_u32, move || {
            move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
        })
        .unwrap();

        worker.submit(1);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let shutdown = worker.begin_shutdown();
        assert!(shutdown.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).unwrap();
        shutdown.recv_timeout(Duration::from_secs(1)).unwrap();
    }
}
