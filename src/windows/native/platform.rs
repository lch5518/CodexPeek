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
        Globalization::{GetUserDefaultLocaleName, GetUserDefaultUILanguage},
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
                GetDpiForMonitor, GetDpiForWindow, SetProcessDpiAwarenessContext,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
            },
            Shell::{ShellExecuteW, NIN_SELECT},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, GetParent, GetWindowLongPtrW, GetWindowRect, IsWindow, KillTimer,
                LoadCursorW, MessageBoxW, MoveWindow, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SetParent, SetTimer, SetWindowLongPtrW, SetWindowPos,
                ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
                GWLP_USERDATA, GWL_STYLE, HTCAPTION, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW,
                MB_ICONINFORMATION, MB_OK, MSG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
                SW_HIDE, SW_SHOWNA, SW_SHOWNORMAL, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU,
                WM_DESTROY, WM_DPICHANGED, WM_EXITSIZEMOVE, WM_LBUTTONUP, WM_NCCREATE,
                WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_CHILD,
                WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::{DisplayMode, UsageLevel};

use super::super::{
    is_exact_github_tag_page,
    lifecycle::{
        CleanupAction, DetachOutcome, FloatingTransition, NativeLifecycle, RecoveryDecision,
        RecoveryEvent,
    },
    taskbar::attach_to_taskbar,
    tray::{TrayIcon, TRAY_CALLBACK},
    widget::{
        clamp_floating_position, logical_to_physical, restore_monitor_relative_position,
        save_monitor_relative_position, Rect, WidgetLayout,
    },
    UiAction, UiBackend, UiSettings, UsageRowView, WidgetViewModel,
};

const TIMER_ID: usize = 1;
const OWNER_CLASS: PCWSTR = w!("CodexUsageMonitor.Hidden.v1");
const WIDGET_CLASS: PCWSTR = w!("CodexUsageMonitor.Widget.v1");
static TASKBAR_CREATED_MESSAGE: AtomicU32 = AtomicU32::new(0);

pub(super) struct SingleInstanceGuard(pub(super) HANDLE);

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct NativeState<'a> {
    backend: &'a mut dyn UiBackend,
    instance: HINSTANCE,
    owner: HWND,
    widget: HWND,
    tray: Option<TrayIcon>,
    taskbar_parent: Option<HWND>,
    settings: UiSettings,
    lifecycle: NativeLifecycle,
}

pub(super) fn run(backend: &mut dyn UiBackend) -> io::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let module = GetModuleHandleW(None).map_err(win_error)?;
        let instance = HINSTANCE(module.0);
        register_classes(instance)?;
        let settings = backend.settings();
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED_MESSAGE.store(taskbar_created, Ordering::Relaxed);
        let mut state = Box::new(NativeState {
            backend,
            instance,
            owner: HWND::default(),
            widget: HWND::default(),
            tray: None,
            taskbar_parent: None,
            settings,
            lifecycle: NativeLifecycle::default(),
        });
        let state_pointer = (&mut *state as *mut NativeState<'_>).cast();
        let result = (|| {
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                OWNER_CLASS,
                w!("Codex Usage Monitor"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                Some(state_pointer),
            )
            .map_err(win_error)?;
            state.owner = owner;
            state.lifecycle.owner_created();
            create_widget((&mut *state) as *mut NativeState<'_>)?;
            let snapshot = state.backend.snapshot();
            state.tray = Some(TrayIcon::new(
                owner,
                highest_percent(&snapshot),
                &snapshot.status,
            )?);
            state.lifecycle.tray_created();
            if SetTimer(Some(owner), TIMER_ID, 1_000, None) == 0 {
                return Err(io::Error::last_os_error());
            }
            state.lifecycle.timer_started();
            apply_window_policy((&mut *state) as *mut NativeState<'_>, true)?;

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
            Ok(())
        })();
        cleanup_native_state((&mut *state) as *mut NativeState<'_>);
        result
    }
}

pub(super) fn acquire_single_instance() -> io::Result<SingleInstanceGuard> {
    unsafe {
        let mutex = CreateMutexW(None, true, w!("Local\\CodexUsageMonitor.SingleInstance.v1"))
            .map_err(win_error)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Codex Usage Monitor is already running",
            ));
        }
        Ok(SingleInstanceGuard(mutex))
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

unsafe fn cleanup_native_state(state_pointer: *mut NativeState<'_>) {
    let actions = (*state_pointer).lifecycle.cleanup_actions();
    for action in actions {
        match action {
            CleanupAction::StopTimer => {
                let owner = (*state_pointer).owner;
                if owner != HWND::default() {
                    let _ = KillTimer(Some(owner), TIMER_ID);
                }
            }
            CleanupAction::RemoveTray => {
                let tray = (*state_pointer).tray.take();
                drop(tray);
            }
            CleanupAction::DestroyWidget => {
                let widget = (*state_pointer).widget;
                if widget != HWND::default() && IsWindow(Some(widget)).as_bool() {
                    let _ = DestroyWindow(widget);
                }
            }
            CleanupAction::DestroyOwner => {
                let owner = (*state_pointer).owner;
                if owner != HWND::default() && IsWindow(Some(owner)).as_bool() {
                    let _ = DestroyWindow(owner);
                }
            }
        }
    }
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
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }

    if message == taskbar_created {
        let _ = refresh_tray(pointer, true);
        let _ = recover_widget(pointer, RecoveryEvent::TaskbarCreated);
        return LRESULT(0);
    }

    match message {
        WM_TIMER if wparam.0 == TIMER_ID => {
            let _ = refresh_tray(pointer, false);
            let widget = (*pointer).widget;
            if widget != HWND::default() {
                let _ = InvalidateRect(Some(widget), None, false);
            }
            let _ = recover_widget(pointer, RecoveryEvent::Timer);
            LRESULT(0)
        }
        TRAY_CALLBACK => {
            let event = lparam.0 as u32 & 0xffff;
            if matches!(
                event,
                WM_CONTEXTMENU | WM_RBUTTONUP | WM_LBUTTONUP | NIN_SELECT
            ) {
                let (owner, settings) = {
                    let state = &*pointer;
                    (state.owner, state.settings.clone())
                };
                let command = TrayIcon::show_menu(owner, &settings);
                if let Some(command) = command {
                    dispatch_menu(pointer, command);
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            dispatch_menu(pointer, (wparam.0 & 0xffff) as u16);
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
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeState<'static>;
    if message == WM_NCDESTROY {
        if let Some(state) = pointer.as_mut() {
            if state.widget == hwnd {
                state.widget = HWND::default();
                state.taskbar_parent = None;
                state.lifecycle.widget_destroyed();
            }
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if !matches!(
        message,
        WM_NCHITTEST | WM_PAINT | WM_DPICHANGED | WM_EXITSIZEMOVE | WM_CLOSE
    ) {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_NCHITTEST if (*pointer).taskbar_parent.is_none() => LRESULT(HTCAPTION as isize),
        WM_PAINT => {
            let snapshot = (*pointer).backend.snapshot();
            paint_widget(hwnd, &snapshot);
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
        WM_EXITSIZEMOVE if (*pointer).taskbar_parent.is_none() => {
            persist_position(pointer);
            LRESULT(0)
        }
        WM_CLOSE => {
            let settings = (*pointer).backend.dispatch(UiAction::ToggleWidget);
            (*pointer).settings = settings;
            let _ = apply_window_policy(pointer, false);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn store_state(hwnd: HWND, lparam: LPARAM) {
    let create = &*(lparam.0 as *const CREATESTRUCTW);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
}

unsafe fn dispatch_menu(state_pointer: *mut NativeState<'_>, menu_id: u16) {
    let Some(action) = super::super::menu_action(menu_id) else {
        return;
    };
    if action == UiAction::Exit {
        PostQuitMessage(0);
        return;
    }
    let force_floating_position = matches!(
        action,
        UiAction::ResetPosition | UiAction::SetDisplayMode(DisplayMode::Floating)
    );
    let settings = (*state_pointer).backend.dispatch(action);
    (*state_pointer).settings = settings;
    let _ = apply_window_policy(state_pointer, force_floating_position);
}

unsafe fn create_widget(state_pointer: *mut NativeState<'_>) -> io::Result<HWND> {
    let (owner, instance, settings) = {
        let state = &*state_pointer;
        (state.owner, state.instance, state.settings.clone())
    };
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
        Some(state_pointer.cast()),
    )
    .map_err(win_error)?;
    {
        let state = &mut *state_pointer;
        state.widget = widget;
        state.taskbar_parent = None;
        state.lifecycle.widget_created();
    }
    place_floating(&settings, widget)?;
    Ok(widget)
}

unsafe fn recover_widget(
    state_pointer: *mut NativeState<'_>,
    event: RecoveryEvent,
) -> io::Result<()> {
    let widget = (*state_pointer).widget;
    if widget != HWND::default() && !IsWindow(Some(widget)).as_bool() {
        let state = &mut *state_pointer;
        state.widget = HWND::default();
        state.taskbar_parent = None;
        state.lifecycle.widget_destroyed();
    }
    let (decision, mode) = {
        let state = &*state_pointer;
        (
            state
                .lifecycle
                .recovery_decision(event, state.settings.widget_visible),
            state.settings.display_mode,
        )
    };
    if matches!(decision, RecoveryDecision::RecreateAndApply) {
        create_widget(state_pointer)?;
    }
    if !matches!(decision, RecoveryDecision::NoWidgetNeeded)
        && (matches!(event, RecoveryEvent::TaskbarCreated) || mode == DisplayMode::Taskbar)
    {
        apply_window_policy(state_pointer, false)?;
    }
    Ok(())
}

unsafe fn refresh_tray(state_pointer: *mut NativeState<'_>, restore: bool) -> io::Result<()> {
    let snapshot = (*state_pointer).backend.snapshot();
    let mut tray = (*state_pointer)
        .tray
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tray icon unavailable"))?;
    let result = if restore {
        tray.restore(highest_percent(&snapshot), &snapshot.status)
    } else {
        tray.update(highest_percent(&snapshot), &snapshot.status)
    };
    (*state_pointer).tray = Some(tray);
    result
}

unsafe fn apply_window_policy(
    state_pointer: *mut NativeState<'_>,
    force_floating_position: bool,
) -> io::Result<()> {
    if (*state_pointer).widget == HWND::default() {
        create_widget(state_pointer)?;
    }
    let (widget, settings, previous_taskbar_parent) = {
        let state = &*state_pointer;
        (state.widget, state.settings.clone(), state.taskbar_parent)
    };
    if !settings.widget_visible {
        let _ = ShowWindow(widget, SW_HIDE);
        return Ok(());
    }
    if settings.display_mode == DisplayMode::Taskbar {
        let parent_is_valid = previous_taskbar_parent.is_some_and(|parent| {
            IsWindow(Some(parent)).as_bool() && GetParent(widget).ok() == Some(parent)
        });
        if !parent_is_valid {
            (*state_pointer).taskbar_parent = None;
        }
        if let Ok(parent) = attach_to_taskbar(
            widget,
            settings.taskbar_offset,
            settings.monitor_device.as_deref(),
        ) {
            let state = &mut *state_pointer;
            state.taskbar_parent = Some(parent);
            state.lifecycle.widget_attached_to_taskbar();
            let _ = ShowWindow(widget, SW_SHOWNA);
            return Ok(());
        }
    }
    transition_to_floating(
        state_pointer,
        force_floating_position || previous_taskbar_parent.is_some(),
    )?;
    let widget = (*state_pointer).widget;
    let _ = ShowWindow(widget, SW_SHOWNA);
    Ok(())
}

unsafe fn transition_to_floating(
    state_pointer: *mut NativeState<'_>,
    force_position: bool,
) -> io::Result<()> {
    let (widget, owner, settings, recorded_attached) = {
        let state = &*state_pointer;
        (
            state.widget,
            state.owner,
            state.settings.clone(),
            state.taskbar_parent.is_some(),
        )
    };
    let attached =
        recorded_attached || GetParent(widget).ok().is_some_and(|parent| parent != owner);
    let outcome = if attached {
        detach_widget(widget)
    } else {
        DetachOutcome::DetachedAndVerified
    };
    if matches!(
        NativeLifecycle::floating_transition(outcome),
        FloatingTransition::RecreateAndPlace
    ) {
        if IsWindow(Some(widget)).as_bool() {
            let _ = DestroyWindow(widget);
        }
        create_widget(state_pointer)?;
        return transition_to_floating(state_pointer, true);
    }
    let style = GetWindowLongPtrW(widget, GWL_STYLE) as u32;
    SetWindowLongPtrW(
        widget,
        GWL_STYLE,
        ((style & !WS_CHILD.0) | WS_POPUP.0 | WS_CLIPSIBLINGS.0) as isize,
    );
    (*state_pointer).taskbar_parent = None;
    let layout = WidgetLayout::for_dpi(GetDpiForWindow(widget).max(96));
    let _ = SetWindowPos(
        widget,
        Some(if settings.always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        }),
        0,
        0,
        layout.window.width(),
        layout.window.height(),
        SWP_NOMOVE | SWP_FRAMECHANGED | SWP_NOACTIVATE,
    );
    if force_position {
        place_floating(&settings, widget)?;
    }
    Ok(())
}

unsafe fn detach_widget(widget: HWND) -> DetachOutcome {
    if SetParent(widget, None).is_err() {
        return DetachOutcome::ApiFailed;
    }
    if GetParent(widget).is_ok() {
        DetachOutcome::ParentRemains
    } else {
        DetachOutcome::DetachedAndVerified
    }
}

unsafe fn place_floating(settings: &UiSettings, hwnd: HWND) -> io::Result<()> {
    let monitor = settings
        .monitor_device
        .as_deref()
        .and_then(|device| find_monitor_by_device(device))
        .unwrap_or_else(|| MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
    let mut monitor_info = MONITORINFOEXW::default();
    monitor_info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !GetMonitorInfoW(monitor, &mut monitor_info.monitorInfo).as_bool() {
        return Err(io::Error::last_os_error());
    }
    let work = monitor_info.monitorInfo.rcWork;
    let dpi = monitor_dpi(monitor);
    let layout = WidgetLayout::for_dpi(dpi);
    let desired = settings
        .floating_position
        .map(|position| restore_monitor_relative_position(position, (work.left, work.top), dpi))
        .unwrap_or((
            work.right - layout.window.width() - logical_to_physical(24, dpi),
            work.top + logical_to_physical(24, dpi),
        ));
    let position = clamp_floating_position(
        desired,
        (layout.window.width(), layout.window.height()),
        Rect::new(work.left, work.top, work.right, work.bottom),
    );
    MoveWindow(
        hwnd,
        position.0,
        position.1,
        layout.window.width(),
        layout.window.height(),
        false,
    )?;
    let actual_dpi = GetDpiForWindow(hwnd).max(96);
    if actual_dpi != dpi {
        let actual_layout = WidgetLayout::for_dpi(actual_dpi);
        let desired = settings
            .floating_position
            .map(|saved| {
                restore_monitor_relative_position(saved, (work.left, work.top), actual_dpi)
            })
            .unwrap_or(position);
        let actual_position = clamp_floating_position(
            desired,
            (actual_layout.window.width(), actual_layout.window.height()),
            Rect::new(work.left, work.top, work.right, work.bottom),
        );
        MoveWindow(
            hwnd,
            actual_position.0,
            actual_position.1,
            actual_layout.window.width(),
            actual_layout.window.height(),
            false,
        )?;
    }
    Ok(())
}

unsafe fn monitor_dpi(monitor: HMONITOR) -> u32 {
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() {
        dpi_x.max(96)
    } else {
        96
    }
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

unsafe fn persist_position(state_pointer: *mut NativeState<'_>) {
    let widget = (*state_pointer).widget;
    let mut rect = RECT::default();
    if GetWindowRect(widget, &mut rect).is_err() {
        return;
    }
    let dpi = GetDpiForWindow(widget).max(1);
    let monitor = MonitorFromWindow(widget, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let (work_origin, device) = if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        let end = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        (
            (info.monitorInfo.rcWork.left, info.monitorInfo.rcWork.top),
            Some(String::from_utf16_lossy(&info.szDevice[..end])),
        )
    } else {
        ((0, 0), None)
    };
    let position = save_monitor_relative_position((rect.left, rect.top), work_origin, dpi);
    let settings = (*state_pointer)
        .backend
        .dispatch(UiAction::SaveFloatingPosition {
            position,
            monitor_device: device,
        });
    (*state_pointer).settings = settings;
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
        // Windows 10의 낮은 작업 표시줄에서는 상태 문구를 생략한 2행 축약 뷰를 사용합니다.
        let margin = logical_to_physical(8, dpi);
        let gap = logical_to_physical(1, dpi);
        let midpoint = (client.bottom - client.top) / 2;
        if let Some(row) = &view.primary {
            draw_compact_row(
                dc,
                row,
                Rect::new(margin, gap, client.right - margin, midpoint - gap),
                dpi,
            );
        }
        if let Some(row) = &view.secondary {
            draw_compact_row(
                dc,
                row,
                Rect::new(
                    margin,
                    midpoint + gap,
                    client.right - margin,
                    client.bottom - gap,
                ),
                dpi,
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
    dpi: u32,
) {
    let marker = match row.level {
        UsageLevel::Stable => "○",
        UsageLevel::Normal => "■",
        UsageLevel::Caution => "▲",
        UsageLevel::Danger => "!",
        UsageLevel::Limited => "×",
    };
    let color = level_color(row.level);
    let bar_height = logical_to_physical(3, dpi);
    let label_reserve = logical_to_physical(64, dpi);
    let percent_width = logical_to_physical(60, dpi);
    let bar = Rect::new(
        bounds.left,
        bounds.bottom - bar_height,
        bounds.right,
        bounds.bottom,
    );
    let mut label = RECT {
        left: bounds.left,
        top: bounds.top,
        right: bounds.right - label_reserve,
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
        left: bounds.right - percent_width,
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

pub(super) unsafe fn user_ui_language() -> (Option<u16>, Option<String>) {
    let language = Some(GetUserDefaultUILanguage());
    let mut buffer = [0_u16; 85];
    let length = GetUserDefaultLocaleName(&mut buffer);
    let locale = if length > 1 {
        Some(String::from_utf16_lossy(&buffer[..length as usize - 1]))
    } else {
        None
    };
    (language, locale)
}

pub(super) unsafe fn open_validated_tag_page(url: &str) -> io::Result<()> {
    if !is_exact_github_tag_page(url) {
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

pub(super) unsafe fn show_diagnostic_summary(title: &str, message: &str) -> io::Result<()> {
    let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let result = MessageBoxW(
        None,
        PCWSTR(message.as_ptr()),
        PCWSTR(title.as_ptr()),
        MB_OK | MB_ICONINFORMATION,
    );
    if result.0 == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
