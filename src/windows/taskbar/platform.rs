use std::{
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, RECT, RPC_E_CHANGED_MODE, WPARAM},
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            Threading::GetCurrentThreadId,
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, SetWinEventHook, TreeScope_Descendants,
                UIA_ClassNamePropertyId, UnhookWinEvent,
            },
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                EnumChildWindows, FindWindowExW, FindWindowW, GetClassNameW, GetMessageW,
                GetParent, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
                PeekMessageW, PostMessageW, PostThreadMessageW, SetParent, SetWindowLongPtrW,
                SetWindowPos, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
                EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_REORDER, GWL_STYLE, HWND_TOP, MSG,
                PM_NOREMOVE, PM_REMOVE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SWP_NOZORDER, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP,
            },
        },
    },
};

use super::{
    place_taskbar_widget, run_taskbar_attachment, taskbar_widget_minimum_width,
    taskbar_widget_size, Rect, TaskbarAttachmentBackend, TaskbarGeometry,
};

const TASK_BUTTON_GAP_LOGICAL: i32 = 4;
pub(crate) const TASKBAR_LAYOUT_CHANGED: u32 = WM_APP + 41;
const OBSERVER_REFRESH: u32 = WM_APP + 42;
const OBSERVER_STOP: u32 = WM_APP + 43;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskbarTarget {
    pub parent: HWND,
    pub placement: Rect,
    pub origin: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ObservedTaskbar {
    parent: isize,
    geometry: TaskbarGeometry,
    dpi: u32,
}

struct GenerationSnapshot<T> {
    generation: u64,
    value: T,
}

/// Explorer 세대별로 최신 작업 표시줄 관찰 결과만 공개하는 짧은 잠금 캐시입니다.
struct GenerationCache<T> {
    generation: AtomicU64,
    snapshot: Mutex<GenerationSnapshot<T>>,
}

impl<T: Clone + Default + PartialEq> GenerationCache<T> {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            snapshot: Mutex::new(GenerationSnapshot {
                generation: 0,
                value: T::default(),
            }),
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn current(&self) -> T {
        let generation = self.generation();
        let snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if generation == self.generation() && snapshot.generation == generation {
            snapshot.value.clone()
        } else {
            T::default()
        }
    }

    fn publish(&self, generation: u64, value: T) -> bool {
        if generation != self.generation() {
            return false;
        }
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if generation != self.generation() {
            return false;
        }
        let changed = snapshot.generation != generation || snapshot.value != value;
        if changed {
            snapshot.generation = generation;
            snapshot.value = value;
        }
        changed
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

pub(crate) struct TaskbarObserver {
    snapshot: Arc<GenerationCache<Vec<ObservedTaskbar>>>,
    thread_id: u32,
}

impl TaskbarObserver {
    pub(crate) fn start(owner: HWND) -> io::Result<Self> {
        let snapshot = Arc::new(GenerationCache::new());
        let worker_snapshot = Arc::clone(&snapshot);
        let owner = owner.0 as isize;
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("taskbar-observer".to_owned())
            .spawn(move || unsafe {
                run_observer_thread(owner, worker_snapshot, ready_sender);
            })?;
        let thread_id = ready_receiver.recv().unwrap_or_default();
        let observer = Self {
            snapshot,
            thread_id,
        };
        // Explorer가 시작 직후 작업 표시줄을 순차적으로 만들 수 있으므로 첫 관찰 뒤 한 번 더 확인합니다.
        observer.refresh();
        Ok(observer)
    }

    pub(crate) fn targets(&self, offset: i32) -> Vec<TaskbarTarget> {
        self.snapshot
            .current()
            .into_iter()
            .filter_map(|taskbar| target_from_observation(taskbar, offset))
            .collect()
    }

    pub(crate) fn refresh(&self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, OBSERVER_REFRESH, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Explorer 재시작 전에 시작된 조회와 캐시된 HWND를 모두 무효화합니다.
    pub(crate) fn invalidate(&self) {
        self.snapshot.invalidate();
        self.refresh();
    }
}

impl Drop for TaskbarObserver {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, OBSERVER_STOP, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub fn taskbar_available() -> bool {
    unsafe {
        taskbars()
            .into_iter()
            .filter_map(|taskbar| {
                taskbar_geometry(taskbar, None).map(|(geometry, dpi)| ObservedTaskbar {
                    parent: taskbar.0 as isize,
                    geometry,
                    dpi,
                })
            })
            .any(|taskbar| target_from_observation(taskbar, 0).is_some())
    }
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

fn target_from_observation(taskbar: ObservedTaskbar, offset: i32) -> Option<TaskbarTarget> {
    let Ok(size) = taskbar_widget_size(taskbar.geometry.taskbar.height(), taskbar.dpi) else {
        return None;
    };
    let minimum_width = taskbar_widget_minimum_width(taskbar.dpi);
    let offset = crate::windows::widget::logical_to_physical(offset, taskbar.dpi);
    let placement = place_taskbar_widget(taskbar.geometry, size, minimum_width, offset).ok()?;
    Some(TaskbarTarget {
        parent: HWND(taskbar.parent as *mut core::ffi::c_void),
        placement,
        origin: (taskbar.geometry.taskbar.left, taskbar.geometry.taskbar.top),
    })
}

unsafe fn taskbar_geometry(
    taskbar: HWND,
    automation: Option<&IUIAutomation>,
) -> Option<(TaskbarGeometry, u32)> {
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
    let occupied = automation
        .and_then(|automation| task_button_area(taskbar, taskbar_bounds, automation))
        .map(|mut occupied| {
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

unsafe fn task_button_area(
    taskbar: HWND,
    taskbar_bounds: Rect,
    automation: &IUIAutomation,
) -> Option<Rect> {
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

unsafe fn run_observer_thread(
    owner: isize,
    snapshot: Arc<GenerationCache<Vec<ObservedTaskbar>>>,
    ready: mpsc::SyncSender<u32>,
) {
    let thread_id = GetCurrentThreadId();
    let mut queue_message = MSG::default();
    let _ = PeekMessageW(&mut queue_message, None, 0, 0, PM_NOREMOVE);
    let _ = ready.send(thread_id);

    let automation = TaskbarAutomation::new();
    let mut explorer_process = 0;
    let mut event_hook = None;
    refresh_observer_snapshot(
        owner,
        &snapshot,
        automation.automation.as_ref(),
        &mut explorer_process,
        &mut event_hook,
    );

    let mut message = MSG::default();
    loop {
        let result = GetMessageW(&mut message, None, 0, 0);
        if result.0 <= 0 || message.message == OBSERVER_STOP {
            break;
        }
        if message.message != OBSERVER_REFRESH {
            continue;
        }
        while PeekMessageW(
            &mut queue_message,
            None,
            OBSERVER_REFRESH,
            OBSERVER_REFRESH,
            PM_REMOVE,
        )
        .as_bool()
        {}
        refresh_observer_snapshot(
            owner,
            &snapshot,
            automation.automation.as_ref(),
            &mut explorer_process,
            &mut event_hook,
        );
    }
    if let Some(hook) = event_hook {
        let _ = UnhookWinEvent(hook);
    }
}

unsafe fn refresh_observer_snapshot(
    owner: isize,
    snapshot: &GenerationCache<Vec<ObservedTaskbar>>,
    automation: Option<&IUIAutomation>,
    explorer_process: &mut u32,
    event_hook: &mut Option<windows::Win32::UI::Accessibility::HWINEVENTHOOK>,
) {
    let generation = snapshot.generation();
    let windows = taskbars();
    let next_process = windows.first().map_or(0, |taskbar| {
        let mut process = 0;
        GetWindowThreadProcessId(*taskbar, Some(&mut process));
        process
    });
    if next_process != *explorer_process {
        if let Some(hook) = event_hook.take() {
            let _ = UnhookWinEvent(hook);
        }
        *explorer_process = next_process;
        if next_process != 0 {
            let hook = SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(taskbar_event),
                next_process,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            if !hook.is_invalid() {
                *event_hook = Some(hook);
            }
        }
    }

    let taskbars = windows
        .into_iter()
        .filter_map(|taskbar| {
            taskbar_geometry(taskbar, automation).map(|(geometry, dpi)| ObservedTaskbar {
                parent: taskbar.0 as isize,
                geometry,
                dpi,
            })
        })
        .collect::<Vec<_>>();
    let changed = snapshot.publish(generation, taskbars);
    if changed {
        let owner = HWND(owner as *mut core::ffi::c_void);
        let _ = PostMessageW(Some(owner), TASKBAR_LAYOUT_CHANGED, WPARAM(0), LPARAM(0));
    }
}

unsafe extern "system" fn taskbar_event(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if matches!(
        event,
        EVENT_OBJECT_CREATE
            | EVENT_OBJECT_DESTROY
            | EVENT_OBJECT_REORDER
            | EVENT_OBJECT_LOCATIONCHANGE
    ) {
        let _ = PostThreadMessageW(GetCurrentThreadId(), OBSERVER_REFRESH, WPARAM(0), LPARAM(0));
    }
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
        sync::{mpsc, Arc},
        thread,
        time::{Duration, Instant},
    };

    use super::GenerationCache;

    #[test]
    fn observer_reads_never_wait_for_a_blocked_scanner() {
        let cache = Arc::new(GenerationCache::new());
        assert!(cache.publish(cache.generation(), vec![1_u32]));

        let worker_cache = Arc::clone(&cache);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let generation = worker_cache.generation();
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            worker_cache.publish(generation, vec![2_u32])
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let started = Instant::now();
        assert_eq!(cache.current(), vec![1]);
        assert!(started.elapsed() < Duration::from_millis(100));

        release_tx.send(()).unwrap();
        assert!(worker.join().unwrap());
        assert_eq!(cache.current(), vec![2]);
    }

    #[test]
    fn invalidation_ignores_a_result_from_before_explorer_restart() {
        let cache = GenerationCache::new();
        let stale_generation = cache.generation();
        assert!(cache.publish(stale_generation, vec![1_u32]));

        cache.invalidate();

        assert!(cache.current().is_empty());
        assert!(!cache.publish(stale_generation, vec![2]));
        assert!(cache.current().is_empty());

        let current_generation = cache.generation();
        assert!(cache.publish(current_generation, vec![3]));
        assert_eq!(cache.current(), vec![3]);
    }
}
