use std::{
    io,
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
};

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, RECT, RPC_E_CHANGED_MODE},
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ClassNamePropertyId,
            },
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowExW, FindWindowW, GetClassNameW, GetParent,
                GetWindowLongPtrW, GetWindowRect, SetParent, SetWindowLongPtrW, SetWindowPos,
                GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SWP_NOZORDER,
            },
        },
    },
};

use super::{
    place_taskbar_widget, run_taskbar_attachment, taskbar_widget_minimum_width,
    taskbar_widget_size, Rect, TaskbarAttachmentBackend, TaskbarGeometry,
};

const TASK_BUTTON_GAP_LOGICAL: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskbarTarget {
    pub parent: HWND,
    pub placement: Rect,
    pub origin: (i32, i32),
}

// HWND는 이 워커에서 생성·소유하지 않고 값으로만 전달하며, 실제 창 조작은 UI 스레드에서 수행합니다.
unsafe impl Send for TaskbarTarget {}

#[derive(Clone, Copy)]
struct RefreshRequest {
    generation: u64,
    offset: i32,
}

struct RefreshResult<T> {
    request: RefreshRequest,
    value: T,
}

/// 느릴 수 있는 작업 표시줄 조회와 UI 스레드 사이의 최신 결과 캐시입니다.
struct BackgroundRefresh<T> {
    requested: Arc<Mutex<RefreshRequest>>,
    trigger: SyncSender<()>,
    results: Receiver<RefreshResult<T>>,
    generation: u64,
    latest: Option<(i32, T)>,
}

impl<T: Send + 'static> BackgroundRefresh<T> {
    /// 느릴 수 있는 조회를 전용 워커에서 실행하는 비차단 캐시를 만듭니다.
    ///
    /// `scanner`는 워커 스레드에서 호출됩니다. 호출이 정지하더라도 `current`는 마지막 정상 결과를
    /// 즉시 반환하며, 프로세스 종료 시 정지한 외부 호출을 기다리지 않습니다.
    fn spawn(scanner: impl Fn(i32) -> T + Send + 'static) -> io::Result<Self> {
        let requested = Arc::new(Mutex::new(RefreshRequest {
            generation: 0,
            offset: 0,
        }));
        let worker_request = Arc::clone(&requested);
        let (trigger, requests) = mpsc::sync_channel(1);
        let (result_tx, results) = mpsc::channel();
        thread::Builder::new()
            .name("taskbar-geometry".to_string())
            .spawn(move || {
                while requests.recv().is_ok() {
                    let request = *worker_request
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let value = scanner(request.offset);
                    if result_tx.send(RefreshResult { request, value }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requested,
            trigger,
            results,
            generation: 0,
            latest: None,
        })
    }

    fn current(&mut self, offset: i32) -> Option<&T> {
        while let Ok(result) = self.results.try_recv() {
            if result.request.generation == self.generation && result.request.offset == offset {
                self.latest = Some((offset, result.value));
            }
        }
        *self
            .requested
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = RefreshRequest {
            generation: self.generation,
            offset,
        };
        let _ = self.trigger.try_send(());
        self.latest
            .as_ref()
            .and_then(|(cached_offset, value)| (*cached_offset == offset).then_some(value))
    }

    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
    }
}

/// UI Automation을 기다리지 않고 마지막 작업 표시줄 배치를 제공하는 비동기 조회기입니다.
pub(crate) struct AsyncTaskbarTargets {
    refresh: BackgroundRefresh<Vec<TaskbarTarget>>,
}

impl AsyncTaskbarTargets {
    /// 작업 표시줄 기하 조회를 전용 워커에서 수행하는 캐시를 만듭니다.
    ///
    /// UI 스레드는 `current`에서 마지막 완료 결과만 읽으며 UI Automation 완료를 기다리지 않습니다.
    /// Explorer가 다시 시작되면 `invalidate`로 이전 HWND 결과를 폐기해야 합니다.
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            refresh: BackgroundRefresh::spawn(|offset| unsafe { taskbar_targets(offset) })?,
        })
    }

    /// 현재 오프셋에 대한 마지막 완료 결과를 반환하고 다음 갱신을 예약합니다.
    ///
    /// 결과가 아직 없거나 오프셋이 바뀌었으면 빈 목록을 반환합니다. 반환된 HWND는 UI 스레드에서만
    /// 사용해야 하며, 이 호출은 진행 중인 UI Automation을 기다리지 않습니다.
    pub(crate) fn current(&mut self, offset: i32) -> Vec<TaskbarTarget> {
        self.refresh.current(offset).cloned().unwrap_or_default()
    }

    /// Explorer 재시작 전에 시작된 조회와 캐시된 HWND를 모두 무효화합니다.
    pub(crate) fn invalidate(&mut self) {
        self.refresh.invalidate();
    }
}

pub fn taskbar_available() -> bool {
    unsafe {
        taskbars()
            .into_iter()
            .any(|taskbar| taskbar_target(taskbar, 0).is_some())
    }
}

pub(crate) unsafe fn taskbar_targets(offset: i32) -> Vec<TaskbarTarget> {
    taskbars()
        .into_iter()
        .filter_map(|taskbar| taskbar_target(taskbar, offset))
        .collect()
}

pub(crate) unsafe fn attach_to_taskbar(hwnd: HWND, target: TaskbarTarget) -> io::Result<()> {
    let mut backend = WindowsAttachmentBackend {
        hwnd,
        placement: target.placement,
        taskbar_origin: target.origin,
    };
    run_taskbar_attachment(&mut backend, target.parent)
        .map_err(|error| io::Error::other(error.to_string()))
}

pub(crate) unsafe fn reposition_taskbar_widget(
    hwnd: HWND,
    target: TaskbarTarget,
) -> io::Result<bool> {
    let mut current = RECT::default();
    GetWindowRect(hwnd, &mut current).map_err(win_error)?;
    if from_native(current) == target.placement {
        return Ok(false);
    }
    SetWindowPos(
        hwnd,
        None,
        target.placement.left - target.origin.0,
        target.placement.top - target.origin.1,
        target.placement.width(),
        target.placement.height(),
        SWP_NOACTIVATE | SWP_NOZORDER,
    )
    .map_err(win_error)?;
    Ok(true)
}

struct WindowsAttachmentBackend {
    hwnd: HWND,
    placement: Rect,
    taskbar_origin: (i32, i32),
}

impl TaskbarAttachmentBackend for WindowsAttachmentBackend {
    type Parent = HWND;
    type Error = io::Error;

    fn read_style(&mut self) -> io::Result<u32> {
        unsafe { Ok(GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32) }
    }

    fn read_parent(&mut self) -> io::Result<Option<HWND>> {
        unsafe { Ok(GetParent(self.hwnd).ok()) }
    }

    fn set_style(&mut self, style: u32) -> io::Result<()> {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, style as isize);
        }
        Ok(())
    }

    fn set_parent(&mut self, parent: Option<HWND>) -> io::Result<()> {
        unsafe { SetParent(self.hwnd, parent).map(|_| ()).map_err(win_error) }
    }

    fn set_position(&mut self) -> io::Result<()> {
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                self.placement.left - self.taskbar_origin.0,
                self.placement.top - self.taskbar_origin.1,
                self.placement.width(),
                self.placement.height(),
                SWP_FRAMECHANGED | SWP_NOACTIVATE,
            )
            .map_err(win_error)
        }
    }

    fn refresh_frame(&mut self) -> io::Result<()> {
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            )
            .map_err(win_error)
        }
    }
}

fn win_error(_: windows::core::Error) -> io::Error {
    io::Error::last_os_error()
}

unsafe fn taskbar_target(taskbar: HWND, offset: i32) -> Option<TaskbarTarget> {
    let (geometry, dpi) = taskbar_geometry(taskbar)?;
    let Ok(size) = taskbar_widget_size(geometry.taskbar.height(), dpi) else {
        return None;
    };
    let minimum_width = taskbar_widget_minimum_width(dpi);
    let offset = crate::windows::widget::logical_to_physical(offset, dpi);
    let placement = place_taskbar_widget(geometry, size, minimum_width, offset).ok()?;
    Some(TaskbarTarget {
        parent: taskbar,
        placement,
        origin: (geometry.taskbar.left, geometry.taskbar.top),
    })
}

unsafe fn taskbar_geometry(taskbar: HWND) -> Option<(TaskbarGeometry, u32)> {
    let mut taskbar_rect = RECT::default();
    if GetWindowRect(taskbar, &mut taskbar_rect).is_err() {
        return None;
    }
    let dpi = GetDpiForWindow(taskbar).max(96);
    let taskbar_bounds = from_native(taskbar_rect);
    let notification = notification_area(taskbar)
        .and_then(|notification| {
            let mut rect = RECT::default();
            GetWindowRect(notification, &mut rect)
                .is_ok()
                .then(|| from_native(rect))
        })
        .unwrap_or_else(|| fallback_notification_area(taskbar_bounds, dpi));
    let occupied = task_button_area(taskbar, taskbar_bounds).map(|mut occupied| {
        occupied.right = occupied
            .right
            .saturating_add(crate::windows::widget::logical_to_physical(
                TASK_BUTTON_GAP_LOGICAL,
                dpi,
            ))
            .min(notification.left);
        occupied
    });
    Some((
        TaskbarGeometry {
            taskbar: taskbar_bounds,
            notification,
            occupied,
        },
        dpi,
    ))
}

unsafe fn task_button_area(taskbar: HWND, taskbar_bounds: Rect) -> Option<Rect> {
    TASKBAR_AUTOMATION.with(|client| {
        let automation = client.automation.as_ref()?;
        let automation_root = taskbar_content_bridge(taskbar).unwrap_or(taskbar);
        let root = automation.ElementFromHandle(automation_root).ok()?;
        let class_name = VARIANT::from("Taskbar.TaskListButtonAutomationPeer");
        let condition = automation
            .CreatePropertyCondition(UIA_ClassNamePropertyId, &class_name)
            .ok()?;
        let elements = root.FindAll(TreeScope_Descendants, &condition).ok()?;
        let length = elements.Length().ok()?;
        let mut occupied: Option<Rect> = None;
        for index in 0..length {
            let Ok(element) = elements.GetElement(index) else {
                continue;
            };
            let Ok(bounds) = element.CurrentBoundingRectangle() else {
                continue;
            };
            let bounds = from_native(bounds);
            if bounds.width() <= 0 || bounds.height() <= 0 || !bounds.intersects(taskbar_bounds) {
                continue;
            }
            occupied = Some(match occupied {
                Some(current) => Rect::new(
                    current.left.min(bounds.left),
                    current.top.min(bounds.top),
                    current.right.max(bounds.right),
                    current.bottom.max(bounds.bottom),
                ),
                None => bounds,
            });
        }
        occupied
    })
}

unsafe fn taskbar_content_bridge(taskbar: HWND) -> Option<HWND> {
    let mut result = HWND::default();
    let _ = EnumChildWindows(
        Some(taskbar),
        Some(find_content_bridge),
        windows::Win32::Foundation::LPARAM((&mut result as *mut HWND) as isize),
    );
    (result != HWND::default()).then_some(result)
}

unsafe extern "system" fn find_content_bridge(
    hwnd: HWND,
    result: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    let mut class_name = [0_u16; 96];
    let length = GetClassNameW(hwnd, &mut class_name);
    let is_content_bridge = length > 0
        && "Windows.UI.Composition.DesktopWindowContentBridge"
            .encode_utf16()
            .eq(class_name[..length as usize].iter().copied());
    if is_content_bridge {
        *(result.0 as *mut HWND) = hwnd;
        false.into()
    } else {
        true.into()
    }
}

struct TaskbarAutomation {
    automation: Option<IUIAutomation>,
    uninitialize: bool,
}

impl TaskbarAutomation {
    unsafe fn new() -> Self {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        let can_use_com = initialized.is_ok() || initialized == RPC_E_CHANGED_MODE;
        let automation = can_use_com
            .then(|| CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok())
            .flatten();
        Self {
            automation,
            uninitialize: initialized.is_ok(),
        }
    }
}

impl Drop for TaskbarAutomation {
    fn drop(&mut self) {
        self.automation.take();
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static TASKBAR_AUTOMATION: TaskbarAutomation = unsafe { TaskbarAutomation::new() };
}

unsafe fn taskbars() -> Vec<HWND> {
    let mut result = Vec::new();
    if let Ok(primary) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
        result.push(primary);
    }
    let mut after = None;
    while let Ok(secondary) =
        FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), PCWSTR::null())
    {
        result.push(secondary);
        after = Some(secondary);
    }
    result
}

unsafe fn notification_area(taskbar: HWND) -> Option<HWND> {
    for class in [w!("TrayNotifyWnd"), w!("ClockButton")] {
        if let Ok(window) = FindWindowExW(Some(taskbar), None, class, PCWSTR::null()) {
            return Some(window);
        }
    }
    None
}

fn fallback_notification_area(taskbar: Rect, dpi: u32) -> Rect {
    let reserved = crate::windows::widget::logical_to_physical(180, dpi)
        .min(taskbar.width().saturating_div(3))
        .max(0);
    Rect::new(
        taskbar.right.saturating_sub(reserved),
        taskbar.top,
        taskbar.right,
        taskbar.bottom,
    )
}

const fn from_native(rect: RECT) -> Rect {
    Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc,
        },
        thread,
        time::{Duration, Instant},
    };

    use super::BackgroundRefresh;

    #[test]
    fn background_refresh_never_waits_for_a_blocked_scanner() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut refresh = BackgroundRefresh::spawn(move |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            7_u32
        })
        .unwrap();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            release_tx.send(()).unwrap();
        });

        let started = Instant::now();
        let current = refresh.current(0);
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(current, None);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        release.join().unwrap();
    }

    #[test]
    fn invalidation_ignores_a_result_from_before_explorer_restart() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = Arc::clone(&calls);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut refresh = BackgroundRefresh::spawn(move |_| {
            if worker_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                1_u32
            } else {
                2_u32
            }
        })
        .unwrap();

        assert_eq!(refresh.current(0), None);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        refresh.invalidate();
        assert_eq!(refresh.current(0), None);
        release_tx.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match refresh.current(0).copied() {
                Some(1) => panic!("stale target survived Explorer restart"),
                Some(2) => break,
                None if Instant::now() < deadline => thread::yield_now(),
                None => panic!("refreshed target was not published"),
                Some(other) => panic!("unexpected target {other}"),
            }
        }
    }
}
