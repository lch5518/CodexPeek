use std::io;

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
        },
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

pub(crate) unsafe fn attach_to_taskbar(
    hwnd: HWND,
    offset: i32,
    preferred_device: Option<&str>,
) -> io::Result<HWND> {
    let mut taskbars = taskbars();
    taskbars.sort_by_key(|taskbar| {
        u8::from(
            preferred_device.is_none() || monitor_device(*taskbar).as_deref() != preferred_device,
        )
    });
    for taskbar in taskbars {
        let Some(notification) = notification_area(taskbar) else {
            continue;
        };
        let mut taskbar_rect = RECT::default();
        let mut notification_rect = RECT::default();
        if GetWindowRect(taskbar, &mut taskbar_rect).is_err()
            || GetWindowRect(notification, &mut notification_rect).is_err()
        {
            continue;
        }
        let geometry = TaskbarGeometry {
            taskbar: from_native(taskbar_rect),
            notification: from_native(notification_rect),
        };
        let dpi = GetDpiForWindow(taskbar).max(96);
        let Ok(widget_size) = taskbar_widget_size(geometry.taskbar.height(), dpi) else {
            continue;
        };
        let offset = crate::windows::widget::logical_to_physical(offset, dpi);
        let Ok(placement) = place_taskbar_widget(geometry, widget_size, offset) else {
            continue;
        };

        let mut backend = WindowsAttachmentBackend {
            hwnd,
            placement,
            taskbar_origin: (geometry.taskbar.left, geometry.taskbar.top),
        };
        match run_taskbar_attachment(&mut backend, taskbar) {
            Ok(()) => return Ok(taskbar),
            Err(error) if error.rollback_failed() => {
                return Err(io::Error::other(error.to_string()));
            }
            Err(_) => continue,
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no compatible horizontal taskbar",
    ))
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
    let Some(notification) = notification_area(taskbar) else {
        return false;
    };
    let mut taskbar_rect = RECT::default();
    let mut notification_rect = RECT::default();
    if GetWindowRect(taskbar, &mut taskbar_rect).is_err()
        || GetWindowRect(notification, &mut notification_rect).is_err()
    {
        return false;
    }
    let geometry = TaskbarGeometry {
        taskbar: from_native(taskbar_rect),
        notification: from_native(notification_rect),
    };
    let dpi = GetDpiForWindow(taskbar).max(96);
    let Ok(size) = taskbar_widget_size(geometry.taskbar.height(), dpi) else {
        return false;
    };
    place_taskbar_widget(geometry, size, 0).is_ok()
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

unsafe fn monitor_device(window: HWND) -> Option<String> {
    let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        return None;
    }
    let end = info
        .szDevice
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(info.szDevice.len());
    Some(String::from_utf16_lossy(&info.szDevice[..end]))
}

const fn from_native(rect: RECT) -> Rect {
    Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}
