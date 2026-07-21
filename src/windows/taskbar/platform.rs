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
                FindWindowExW, FindWindowW, GetParent, GetWindowLongPtrW, GetWindowRect,
                MoveWindow, SetParent, SetWindowLongPtrW, GWL_STYLE, WS_CHILD, WS_CLIPSIBLINGS,
                WS_POPUP,
            },
        },
    },
};

use super::{place_taskbar_widget, Rect, TaskbarGeometry};
use crate::windows::widget::logical_to_physical;

pub fn taskbar_available() -> bool {
    unsafe {
        taskbars()
            .into_iter()
            .any(|bar| notification_area(bar).is_some())
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
        let widget_height = geometry.taskbar.height().min(logical_to_physical(48, dpi));
        let widget_width = logical_to_physical(380, dpi);
        let offset = logical_to_physical(offset, dpi);
        let Ok(placement) = place_taskbar_widget(geometry, (widget_width, widget_height), offset)
        else {
            continue;
        };

        let previous_style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let child_style = (previous_style & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, child_style as isize);
        if SetParent(hwnd, Some(taskbar)).is_err()
            || GetParent(hwnd).ok() != Some(taskbar)
            || MoveWindow(
                hwnd,
                placement.left - geometry.taskbar.left,
                placement.top - geometry.taskbar.top,
                placement.width(),
                placement.height(),
                true,
            )
            .is_err()
        {
            let _ = SetParent(hwnd, None);
            SetWindowLongPtrW(hwnd, GWL_STYLE, previous_style as isize);
            continue;
        }
        return Ok(taskbar);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no compatible horizontal taskbar",
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
