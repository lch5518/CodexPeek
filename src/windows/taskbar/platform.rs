use std::io;

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
