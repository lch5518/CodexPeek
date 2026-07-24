use std::io;

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, RECT},
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                FindWindowExW, FindWindowW, GetParent, GetWindowLongPtrW, GetWindowRect, SetParent,
                SetWindowLongPtrW, SetWindowPos, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            },
        },
    },
};

use super::{
    place_taskbar_widget, run_taskbar_attachment, taskbar_widget_size, Rect,
    TaskbarAttachmentBackend, TaskbarGeometry,
};

pub fn taskbar_available() -> bool {
    unsafe {
        taskbars()
            .into_iter()
            .any(|taskbar| taskbar_has_widget_space(taskbar))
    }
}

pub(crate) unsafe fn taskbar_targets() -> Vec<HWND> {
    taskbars()
        .into_iter()
        .filter(|taskbar| taskbar_has_widget_space(*taskbar))
        .collect()
}

pub(crate) unsafe fn attach_to_taskbar(hwnd: HWND, offset: i32, taskbar: HWND) -> io::Result<()> {
    let (geometry, dpi) = taskbar_geometry(taskbar)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "taskbar geometry unavailable"))?;
    let widget_size = taskbar_widget_size(geometry.taskbar.height(), dpi)
        .map_err(|_| io::Error::other("taskbar is not compatible"))?;
    let offset = crate::windows::widget::logical_to_physical(offset, dpi);
    let placement = place_taskbar_widget(geometry, widget_size, offset)
        .map_err(|_| io::Error::other("taskbar has insufficient widget space"))?;
    let mut backend = WindowsAttachmentBackend {
        hwnd,
        placement,
        taskbar_origin: (geometry.taskbar.left, geometry.taskbar.top),
    };
    run_taskbar_attachment(&mut backend, taskbar)
        .map_err(|error| io::Error::other(error.to_string()))
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

unsafe fn taskbar_has_widget_space(taskbar: HWND) -> bool {
    let Some((geometry, dpi)) = taskbar_geometry(taskbar) else {
        return false;
    };
    let Ok(size) = taskbar_widget_size(geometry.taskbar.height(), dpi) else {
        return false;
    };
    place_taskbar_widget(geometry, size, 0).is_ok()
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
    Some((
        TaskbarGeometry {
            taskbar: taskbar_bounds,
            notification,
        },
        dpi,
    ))
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
