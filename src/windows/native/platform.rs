use std::{
    io,
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    core::{w, BOOL, PCWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, COLORREF, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND,
            LPARAM, LRESULT, RECT, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
            EnumDisplayMonitors, FillRect, GetMonitorInfoW, InvalidateRect, MonitorFromWindow,
            SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
            DEFAULT_PITCH, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_SWISS,
            FW_NORMAL, HDC, HGDIOBJ, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
            OUT_DEFAULT_PRECIS, PAINTSTRUCT, PROOF_QUALITY, TRANSPARENT,
        },
        System::{
            Console::{AttachConsole, ATTACH_PARENT_PROCESS},
            LibraryLoader::GetModuleHandleW,
            Threading::CreateMutexW,
        },
        UI::{
            HiDpi::{
                GetDpiForWindow, SetProcessDpiAwarenessContext,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            Shell::{ShellExecuteW, NIN_SELECT},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, GetParent, GetWindowLongPtrW, GetWindowRect, IsWindow, KillTimer,
                LoadCursorW, MoveWindow, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
                SetParent, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
                CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, GWL_STYLE,
                HTCAPTION, HWND_MESSAGE, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, MSG, SWP_NOMOVE,
                SW_HIDE, SW_SHOWNA, SW_SHOWNORMAL, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU,
                WM_DESTROY, WM_DPICHANGED, WM_EXITSIZEMOVE, WM_LBUTTONUP, WM_NCCREATE,
                WM_NCHITTEST, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_CHILD,
                WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::{DisplayMode, LogicalPosition, UsageLevel};

use super::super::{
    taskbar::attach_to_taskbar,
    tray::{TrayIcon, TRAY_CALLBACK},
    widget::{
        clamp_floating_position, logical_to_physical, physical_to_logical, Rect, WidgetLayout,
    },
    UiAction, UiBackend, UiSettings, UsageRowView, WidgetViewModel,
};

const TIMER_ID: usize = 1;
const OWNER_CLASS: PCWSTR = w!("CodexUsageMonitor.Hidden.v1");
const WIDGET_CLASS: PCWSTR = w!("CodexUsageMonitor.Widget.v1");
static TASKBAR_CREATED_MESSAGE: AtomicU32 = AtomicU32::new(0);

struct OwnedMutex(HANDLE);

impl Drop for OwnedMutex {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct NativeState<'a> {
    backend: &'a mut dyn UiBackend,
    owner: HWND,
    widget: HWND,
    tray: Option<TrayIcon>,
    taskbar_parent: Option<HWND>,
    settings: UiSettings,
}

pub(super) fn run(backend: &mut dyn UiBackend) -> io::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex = CreateMutexW(None, true, w!("Local\\CodexUsageMonitor.SingleInstance.v1"))
            .map_err(win_error)?;
        let mutex = OwnedMutex(mutex);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Codex Usage Monitor is already running",
            ));
        }

        let module = GetModuleHandleW(None).map_err(win_error)?;
        let instance = HINSTANCE(module.0);
        register_classes(instance)?;
        let settings = backend.settings();
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED_MESSAGE.store(taskbar_created, Ordering::Relaxed);
        let mut state = Box::new(NativeState {
            backend,
            owner: HWND::default(),
            widget: HWND::default(),
            tray: None,
            taskbar_parent: None,
            settings,
        });
        let state_pointer = (&mut *state as *mut NativeState<'_>).cast();
        let owner = CreateWindowExW(
            Default::default(),
            OWNER_CLASS,
            w!("Codex Usage Monitor"),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            Some(state_pointer),
        )
        .map_err(win_error)?;
        state.owner = owner;
        let layout = WidgetLayout::for_dpi(96);
        let widget = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            WIDGET_CLASS,
            w!("Codex Usage Monitor"),
            WS_POPUP | WS_CLIPSIBLINGS,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            layout.window.width(),
            layout.window.height(),
            Some(owner),
            None,
            Some(instance),
            Some(state_pointer),
        )
        .map_err(win_error)?;
        state.widget = widget;
        place_initial_floating(&state.settings, widget);
        let snapshot = state.backend.snapshot();
        state.tray = Some(TrayIcon::new(
            owner,
            highest_percent(&snapshot),
            &snapshot.status,
        )?);
        if SetTimer(Some(owner), TIMER_ID, 1_000, None) == 0 {
            state.tray.take();
            let _ = DestroyWindow(widget);
            let _ = DestroyWindow(owner);
            return Err(io::Error::last_os_error());
        }
        apply_window_policy(&mut state);

        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 == -1 {
                return Err(io::Error::last_os_error());
            }
            if result.0 == 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        let _ = KillTimer(Some(owner), TIMER_ID);
        state.tray.take();
        if IsWindow(Some(widget)).as_bool() {
            let _ = DestroyWindow(widget);
        }
        if IsWindow(Some(owner)).as_bool() {
            let _ = DestroyWindow(owner);
        }
        drop(mutex);
        Ok(())
    }
}

unsafe fn register_classes(instance: HINSTANCE) -> io::Result<()> {
    let cursor = LoadCursorW(None, IDC_ARROW).map_err(win_error)?;
    for (name, procedure) in [
        (
            OWNER_CLASS,
            owner_proc as unsafe extern "system" fn(_, _, _, _) -> _,
        ),
        (
            WIDGET_CLASS,
            widget_proc as unsafe extern "system" fn(_, _, _, _) -> _,
        ),
    ] {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: name,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

unsafe extern "system" fn owner_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        store_state(hwnd, lparam);
    }
    let taskbar_created = TASKBAR_CREATED_MESSAGE.load(Ordering::Relaxed);
    if message != taskbar_created
        && !matches!(
            message,
            WM_TIMER | TRAY_CALLBACK | WM_COMMAND | WM_CLOSE | WM_DESTROY
        )
    {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeState<'static>;
    let Some(state) = pointer.as_mut() else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };

    if message == taskbar_created {
        let snapshot = state.backend.snapshot();
        if let Some(tray) = &mut state.tray {
            let _ = tray.restore(highest_percent(&snapshot), &snapshot.status);
        }
        state.taskbar_parent = None;
        apply_window_policy(state);
        return LRESULT(0);
    }

    match message {
        WM_TIMER if wparam.0 == TIMER_ID => {
            let snapshot = state.backend.snapshot();
            if let Some(tray) = &mut state.tray {
                let _ = tray.update(highest_percent(&snapshot), &snapshot.status);
            }
            let _ = InvalidateRect(Some(state.widget), None, false);
            apply_window_policy(state);
            LRESULT(0)
        }
        TRAY_CALLBACK => {
            let event = lparam.0 as u32 & 0xffff;
            if matches!(
                event,
                WM_CONTEXTMENU | WM_RBUTTONUP | WM_LBUTTONUP | NIN_SELECT
            ) {
                let command = state
                    .tray
                    .as_ref()
                    .and_then(|tray| tray.show_menu(&state.settings));
                if let Some(command) = command {
                    dispatch_menu(state, command);
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            dispatch_menu(state, (wparam.0 & 0xffff) as u16);
            LRESULT(0)
        }
        WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn widget_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        store_state(hwnd, lparam);
    }
    if !matches!(
        message,
        WM_NCHITTEST | WM_PAINT | WM_DPICHANGED | WM_EXITSIZEMOVE | WM_CLOSE
    ) {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeState<'static>;
    let Some(state) = pointer.as_mut() else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    match message {
        WM_NCHITTEST if state.taskbar_parent.is_none() => LRESULT(HTCAPTION as isize),
        WM_PAINT => {
            paint_widget(hwnd, &state.backend.snapshot());
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let suggested = &*(lparam.0 as *const RECT);
            let _ = MoveWindow(
                hwnd,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                true,
            );
            LRESULT(0)
        }
        WM_EXITSIZEMOVE if state.taskbar_parent.is_none() => {
            persist_position(state);
            LRESULT(0)
        }
        WM_CLOSE => {
            state.settings = state.backend.dispatch(UiAction::ToggleWidget);
            apply_window_policy(state);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn store_state(hwnd: HWND, lparam: LPARAM) {
    let create = &*(lparam.0 as *const CREATESTRUCTW);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
}

unsafe fn dispatch_menu(state: &mut NativeState<'_>, menu_id: u16) {
    let Some(action) = super::super::menu_action(menu_id) else {
        return;
    };
    if action == UiAction::Exit {
        PostQuitMessage(0);
        return;
    }
    state.settings = state.backend.dispatch(action);
    apply_window_policy(state);
}

unsafe fn apply_window_policy(state: &mut NativeState<'_>) {
    if !state.settings.widget_visible {
        let _ = ShowWindow(state.widget, SW_HIDE);
        return;
    }
    if state.settings.display_mode == DisplayMode::Taskbar {
        let parent_is_valid = state.taskbar_parent.is_some_and(|parent| {
            IsWindow(Some(parent)).as_bool() && GetParent(state.widget).ok() == Some(parent)
        });
        if !parent_is_valid {
            state.taskbar_parent = None;
        }
        if let Ok(parent) = attach_to_taskbar(
            state.widget,
            state.settings.taskbar_offset,
            state.settings.monitor_device.as_deref(),
        ) {
            state.taskbar_parent = Some(parent);
            let _ = ShowWindow(state.widget, SW_SHOWNA);
            return;
        }
    }
    detach_to_floating(state);
    let _ = ShowWindow(state.widget, SW_SHOWNA);
}

unsafe fn detach_to_floating(state: &mut NativeState<'_>) {
    if GetParent(state.widget).ok().is_some() {
        let _ = SetParent(state.widget, None);
    }
    let style = GetWindowLongPtrW(state.widget, GWL_STYLE) as u32;
    SetWindowLongPtrW(
        state.widget,
        GWL_STYLE,
        ((style & !WS_CHILD.0) | WS_POPUP.0 | WS_CLIPSIBLINGS.0) as isize,
    );
    state.taskbar_parent = None;
    let layout = WidgetLayout::for_dpi(GetDpiForWindow(state.widget));
    let _ = SetWindowPos(
        state.widget,
        Some(if state.settings.always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        }),
        0,
        0,
        layout.window.width(),
        layout.window.height(),
        SWP_NOMOVE,
    );
}

unsafe fn place_initial_floating(settings: &UiSettings, hwnd: HWND) {
    let monitor = settings
        .monitor_device
        .as_deref()
        .and_then(|device| find_monitor_by_device(device))
        .unwrap_or_else(|| MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let _ = GetMonitorInfoW(monitor, &mut monitor_info.monitorInfo);
    let work = monitor_info.monitorInfo.rcWork;
    let dpi = GetDpiForWindow(hwnd).max(96);
    let layout = WidgetLayout::for_dpi(dpi);
    let desired = settings
        .floating_position
        .map(|position| {
            (
                logical_to_physical(position.x, dpi),
                logical_to_physical(position.y, dpi),
            )
        })
        .unwrap_or((work.right - layout.window.width() - 24, work.top + 24));
    let position = clamp_floating_position(
        desired,
        (layout.window.width(), layout.window.height()),
        Rect::new(work.left, work.top, work.right, work.bottom),
    );
    let _ = MoveWindow(
        hwnd,
        position.0,
        position.1,
        layout.window.width(),
        layout.window.height(),
        false,
    );
}

struct MonitorSearch {
    device: Vec<u16>,
    found: Option<HMONITOR>,
}

unsafe fn find_monitor_by_device(device: &str) -> Option<HMONITOR> {
    let mut search = MonitorSearch {
        device: device.encode_utf16().collect(),
        found: None,
    };
    let _ = EnumDisplayMonitors(
        None,
        None,
        Some(monitor_search_callback),
        LPARAM((&mut search as *mut MonitorSearch) as isize),
    );
    search.found
}

unsafe extern "system" fn monitor_search_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let search = &mut *(data.0 as *mut MonitorSearch);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        let end = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        if info.szDevice[..end] == search.device {
            search.found = Some(monitor);
            return BOOL(0);
        }
    }
    BOOL(1)
}

unsafe fn persist_position(state: &mut NativeState<'_>) {
    let mut rect = RECT::default();
    if GetWindowRect(state.widget, &mut rect).is_err() {
        return;
    }
    let dpi = GetDpiForWindow(state.widget).max(1);
    let monitor = MonitorFromWindow(state.widget, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let device = if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        let end = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        Some(String::from_utf16_lossy(&info.szDevice[..end]))
    } else {
        None
    };
    state.settings = state.backend.dispatch(UiAction::SaveFloatingPosition {
        position: LogicalPosition {
            x: physical_to_logical(rect.left, dpi),
            y: physical_to_logical(rect.top, dpi),
        },
        monitor_device: device,
    });
}

unsafe fn paint_widget(hwnd: HWND, view: &WidgetViewModel) {
    let mut paint = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut paint);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let layout = WidgetLayout::for_dpi(dpi);
    let mut client = RECT::default();
    if GetClientRect(hwnd, &mut client).is_err() {
        let _ = EndPaint(hwnd, &paint);
        return;
    }
    let background = CreateSolidBrush(COLORREF(0x0023_211f));
    FillRect(dc, &client, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let font_height = -((13_i64 * i64::from(dpi) + 48) / 96) as i32;
    let font = CreateFontW(
        font_height,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        PROOF_QUALITY,
        u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
        w!("Segoe UI"),
    );
    let previous = SelectObject(dc, HGDIOBJ(font.0));
    let _ = SetBkMode(dc, TRANSPARENT);
    if client.bottom - client.top <= logical_to_physical(64, dpi) {
        let midpoint = (client.bottom - client.top) / 2;
        if let Some(row) = &view.primary {
            draw_compact_row(dc, row, Rect::new(8, 1, client.right - 8, midpoint - 1));
        }
        if let Some(row) = &view.secondary {
            draw_compact_row(
                dc,
                row,
                Rect::new(8, midpoint + 1, client.right - 8, client.bottom - 1),
            );
        }
    } else {
        if let Some(row) = &view.primary {
            draw_row(dc, row, layout.primary_bar, dpi);
        }
        if let Some(row) = &view.secondary {
            draw_row(dc, row, layout.secondary_bar, dpi);
        }
        let status = if view.last_success.is_empty() {
            view.status.clone()
        } else {
            format!("{} · {}", view.status, view.last_success)
        };
        let mut status_rect = native_rect(layout.status);
        draw_text(
            dc,
            &status,
            &mut status_rect,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            COLORREF(0x00b8_b8b8),
        );
    }
    SelectObject(dc, previous);
    let _ = DeleteObject(HGDIOBJ(font.0));
    let _ = EndPaint(hwnd, &paint);
}

unsafe fn draw_compact_row(
    dc: windows::Win32::Graphics::Gdi::HDC,
    row: &UsageRowView,
    bounds: Rect,
) {
    let marker = match row.level {
        UsageLevel::Stable => "○",
        UsageLevel::Normal => "■",
        UsageLevel::Caution => "▲",
        UsageLevel::Danger => "!",
        UsageLevel::Limited => "×",
    };
    let color = level_color(row.level);
    let bar = Rect::new(bounds.left, bounds.bottom - 3, bounds.right, bounds.bottom);
    let mut label = RECT {
        left: bounds.left,
        top: bounds.top,
        right: bounds.right - 64,
        bottom: bar.top,
    };
    draw_text(
        dc,
        &format!("{marker} {}", row.label),
        &mut label,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        COLORREF(0x00f0_f0f0),
    );
    let mut percent = RECT {
        left: bounds.right - 60,
        top: bounds.top,
        right: bounds.right,
        bottom: bar.top,
    };
    draw_text(
        dc,
        &row.percent_text,
        &mut percent,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
        color,
    );
    let background = CreateSolidBrush(COLORREF(0x0045_4545));
    FillRect(dc, &native_rect(bar), background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let width =
        (f64::from(bar.width()) * row.used_percent.clamp(0.0, 100.0) / 100.0).round() as i32;
    if width > 0 {
        let fill = CreateSolidBrush(color);
        FillRect(
            dc,
            &RECT {
                right: bar.left + width,
                ..native_rect(bar)
            },
            fill,
        );
        let _ = DeleteObject(HGDIOBJ(fill.0));
    }
}

unsafe fn draw_row(
    dc: windows::Win32::Graphics::Gdi::HDC,
    row: &UsageRowView,
    bar: Rect,
    dpi: u32,
) {
    let scale = |value: i32| ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32;
    let top = bar.top - scale(20);
    let marker = match row.level {
        UsageLevel::Stable => "○",
        UsageLevel::Normal => "■",
        UsageLevel::Caution => "▲",
        UsageLevel::Danger => "!",
        UsageLevel::Limited => "×",
    };
    let color = level_color(row.level);
    let mut label_rect = RECT {
        left: bar.left,
        top,
        right: bar.right - scale(70),
        bottom: bar.top,
    };
    draw_text(
        dc,
        &format!("{marker} {} · {}", row.label, row.reset_text),
        &mut label_rect,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        COLORREF(0x00f0_f0f0),
    );
    let mut percent_rect = RECT {
        left: bar.right - scale(68),
        top,
        right: bar.right,
        bottom: bar.top,
    };
    draw_text(
        dc,
        &row.percent_text,
        &mut percent_rect,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
        color,
    );
    let background = CreateSolidBrush(COLORREF(0x0045_4545));
    FillRect(dc, &native_rect(bar), background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let used = row.used_percent.clamp(0.0, 100.0);
    let width = (f64::from(bar.width()) * used / 100.0).round() as i32;
    if width > 0 {
        let fill = CreateSolidBrush(color);
        FillRect(
            dc,
            &RECT {
                right: bar.left + width,
                ..native_rect(bar)
            },
            fill,
        );
        let _ = DeleteObject(HGDIOBJ(fill.0));
    }
}

unsafe fn draw_text(
    dc: windows::Win32::Graphics::Gdi::HDC,
    value: &str,
    rect: &mut RECT,
    format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
    color: COLORREF,
) {
    let _ = SetTextColor(dc, color);
    let mut text: Vec<u16> = value.encode_utf16().collect();
    let _ = DrawTextW(dc, &mut text, rect, format);
}

const fn native_rect(rect: Rect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

const fn level_color(level: UsageLevel) -> COLORREF {
    match level {
        UsageLevel::Stable => COLORREF(0x0085_d96b),
        UsageLevel::Normal => COLORREF(0x00dc_b45c),
        UsageLevel::Caution => COLORREF(0x0048_b8f0),
        UsageLevel::Danger => COLORREF(0x0045_6df2),
        UsageLevel::Limited => COLORREF(0x00a4_55d8),
    }
}

fn highest_percent(view: &WidgetViewModel) -> Option<f64> {
    [view.primary.as_ref(), view.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|row| row.used_percent)
        .filter(|value| value.is_finite())
        .max_by(f64::total_cmp)
}

fn win_error(_: windows::core::Error) -> io::Error {
    io::Error::last_os_error()
}

pub(super) unsafe fn attach_parent_console() {
    let _ = AttachConsole(ATTACH_PARENT_PROCESS);
}

pub(super) unsafe fn open_validated_tag_page(url: &str) -> io::Result<()> {
    if !url.starts_with("https://github.com/")
        || !url.contains("/releases/tag/")
        || url.contains(['?', '#', '@', '\r', '\n', '\0'])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe release URL",
        ));
    }
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let result = ShellExecuteW(
        None,
        w!("open"),
        PCWSTR(url.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
    if result.0 as isize <= 32 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
