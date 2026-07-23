use std::{
    io,
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, COLORREF, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND,
            LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
        },
        Globalization::{GetUserDefaultLocaleName, GetUserDefaultUILanguage},
        Graphics::Gdi::{
            BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush,
            DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, GetDC, GetStockObject,
            InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
            BITMAPINFOHEADER, BLENDFUNCTION, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
            DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
            FF_SWISS, FW_MEDIUM, FW_NORMAL, HDC, HGDIOBJ, NULL_PEN, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, PROOF_QUALITY, TRANSPARENT,
        },
        System::{
            Console::{AttachConsole, ATTACH_PARENT_PROCESS},
            LibraryLoader::GetModuleHandleW,
            Threading::CreateMutexW,
        },
        UI::{
            Controls::{
                TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH,
                TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW, WM_MOUSELEAVE,
            },
            HiDpi::{
                GetDpiForWindow, SetProcessDpiAwarenessContext,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
            Shell::{ShellExecuteW, NIN_SELECT},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, GetParent, GetWindowLongPtrW, IsWindow, KillTimer, LoadCursorW,
                MessageBoxW, MoveWindow, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
                SendMessageW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
                CW_USEDEFAULT, GWLP_USERDATA, GWL_EXSTYLE, HWND_TOPMOST, IDC_ARROW,
                MB_ICONINFORMATION, MB_OK, MSG, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
                SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNA, SW_SHOWNORMAL, ULW_ALPHA,
                WINDOW_STYLE, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_DPICHANGED, WM_MOUSEMOVE,
                WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_CLIPSIBLINGS,
                WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::diagnostics::{DiagnosticLogger, SafeDiagnostic};

use super::super::{
    is_exact_github_tag_page,
    lifecycle::{CleanupAction, NativeLifecycle, RecoveryDecision, RecoveryEvent},
    taskbar::attach_to_taskbar,
    taskbar_widget::{
        progress_fill_width, select_weekly_row, HoverTransition, TaskbarLayout, TaskbarRisk,
        TASKBAR_WIDTH_LOGICAL,
    },
    tray::{TrayIcon, TRAY_CALLBACK},
    widget::{logical_to_physical, Rect},
    UiAction, UiBackend, UiSettings, WidgetDataState, WidgetViewModel,
};

const TIMER_ID: usize = 1;
const HOVER_TIMER_ID: usize = 2;
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
    hover: HoverTransition,
    mouse_tracking: bool,
    tooltip: HWND,
    tooltip_text: Vec<u16>,
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
            hover: HoverTransition::default(),
            mouse_tracking: false,
            tooltip: HWND::default(),
            tooltip_text: Vec::new(),
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
            apply_window_policy((&mut *state) as *mut NativeState<'_>)?;
            let _ = create_tooltip((&mut *state) as *mut NativeState<'_>);

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
    let tooltip = (*state_pointer).tooltip;
    if tooltip != HWND::default() && IsWindow(Some(tooltip)).as_bool() {
        let _ = DestroyWindow(tooltip);
    }
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
        && !matches!(message, WM_TIMER | TRAY_CALLBACK | WM_CLOSE | WM_DESTROY)
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
            let settings = (*pointer).backend.settings();
            (*pointer).settings = settings;
            let _ = refresh_tray(pointer, false);
            update_tooltip(pointer);
            let widget = (*pointer).widget;
            if widget != HWND::default() {
                let _ = InvalidateRect(Some(widget), None, false);
            }
            let _ = recover_widget(pointer, RecoveryEvent::Timer);
            LRESULT(0)
        }
        TRAY_CALLBACK => {
            let event = lparam.0 as u32 & 0xffff;
            if should_open_tray_menu(event) {
                let current_settings = (*pointer).backend.settings();
                (*pointer).settings = current_settings;
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
        WM_CLOSE | WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

const fn should_open_tray_menu(event: u32) -> bool {
    matches!(event, WM_CONTEXTMENU | NIN_SELECT)
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
        WM_PAINT | WM_DPICHANGED | WM_CLOSE | WM_MOUSEMOVE | WM_MOUSELEAVE | WM_TIMER
    ) {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_MOUSEMOVE if (*pointer).taskbar_parent.is_some() => {
            let state = &mut *pointer;
            state.hover.set_hovered(true);
            if !state.mouse_tracking {
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if TrackMouseEvent(&mut tracking).is_ok() {
                    state.mouse_tracking = true;
                }
            }
            let _ = SetTimer(Some(hwnd), HOVER_TIMER_ID, 15, None);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let state = &mut *pointer;
            state.mouse_tracking = false;
            state.hover.set_hovered(false);
            let _ = SetTimer(Some(hwnd), HOVER_TIMER_ID, 15, None);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == HOVER_TIMER_ID => {
            let state = &mut *pointer;
            let needs_more = state.hover.tick();
            let _ = InvalidateRect(Some(hwnd), None, false);
            if !needs_more {
                let _ = KillTimer(Some(hwnd), HOVER_TIMER_ID);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let snapshot = (*pointer).backend.snapshot();
            validate_paint(hwnd);
            let _ = paint_taskbar_widget(hwnd, &snapshot, (*pointer).hover.value());
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
        WM_CLOSE => {
            let settings = (*pointer).backend.dispatch(UiAction::ToggleWidget);
            (*pointer).settings = settings;
            let _ = apply_window_policy(pointer);
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
    let settings = (*state_pointer).backend.dispatch(action);
    (*state_pointer).settings = settings;
    let _ = apply_window_policy(state_pointer);
}

unsafe fn create_widget(state_pointer: *mut NativeState<'_>) -> io::Result<HWND> {
    let (owner, instance) = {
        let state = &*state_pointer;
        (state.owner, state.instance)
    };
    // 작업표시줄 위젯의 기본 논리 크기. 실제 물리 크기는 작업표시줄 부착 시 보정됩니다.
    let width = logical_to_physical(TASKBAR_WIDTH_LOGICAL, 96);
    let height = logical_to_physical(48, 96);
    let widget = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        WIDGET_CLASS,
        w!("Codex Usage Monitor"),
        WS_POPUP | WS_CLIPSIBLINGS,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        width,
        height,
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
    Ok(widget)
}

unsafe fn create_tooltip(state_pointer: *mut NativeState<'_>) -> io::Result<()> {
    let existing = (*state_pointer).tooltip;
    if existing != HWND::default() && IsWindow(Some(existing)).as_bool() {
        let _ = DestroyWindow(existing);
    }
    (*state_pointer).tooltip = HWND::default();
    let (owner, widget, instance) = {
        let state = &*state_pointer;
        (state.owner, state.widget, state.instance)
    };
    let tooltip = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        TOOLTIPS_CLASSW,
        PCWSTR::null(),
        WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        Some(owner),
        None,
        Some(instance),
        None,
    )
    .map_err(win_error)?;
    (*state_pointer).tooltip = tooltip;
    update_tooltip(state_pointer);
    let state = &mut *state_pointer;
    let tool = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: owner,
        uId: widget.0 as usize,
        hinst: instance,
        lpszText: PWSTR(state.tooltip_text.as_mut_ptr()),
        ..Default::default()
    };
    let result = SendMessageW(
        tooltip,
        TTM_ADDTOOLW,
        Some(WPARAM(0)),
        Some(LPARAM((&tool as *const TTTOOLINFOW) as isize)),
    );
    if result.0 == 0 {
        let _ = DestroyWindow(tooltip);
        state.tooltip = HWND::default();
        return Err(io::Error::last_os_error());
    }
    let _ = SendMessageW(
        tooltip,
        TTM_SETMAXTIPWIDTH,
        Some(WPARAM(0)),
        Some(LPARAM(
            logical_to_physical(320, GetDpiForWindow(widget).max(96)) as isize,
        )),
    );
    let _ = SetWindowPos(
        tooltip,
        Some(HWND_TOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    Ok(())
}

unsafe fn update_tooltip(state_pointer: *mut NativeState<'_>) {
    let view = (*state_pointer).backend.snapshot();
    let state = &mut *state_pointer;
    state.tooltip_text = view.taskbar_tooltip.encode_utf16().chain(Some(0)).collect();
    if state.tooltip == HWND::default() {
        return;
    }
    let tool = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: state.owner,
        uId: state.widget.0 as usize,
        hinst: state.instance,
        lpszText: PWSTR(state.tooltip_text.as_mut_ptr()),
        ..Default::default()
    };
    let _ = SendMessageW(
        state.tooltip,
        TTM_UPDATETIPTEXTW,
        Some(WPARAM(0)),
        Some(LPARAM((&tool as *const TTTOOLINFOW) as isize)),
    );
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
    let decision = {
        let state = &*state_pointer;
        state
            .lifecycle
            .recovery_decision(event, state.settings.widget_visible)
    };
    if matches!(decision, RecoveryDecision::RecreateAndApply) {
        create_widget(state_pointer)?;
        let _ = create_tooltip(state_pointer);
    }
    if !matches!(decision, RecoveryDecision::NoWidgetNeeded) {
        apply_window_policy(state_pointer)?;
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

unsafe fn apply_window_policy(state_pointer: *mut NativeState<'_>) -> io::Result<()> {
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
    let parent_is_valid = previous_taskbar_parent.is_some_and(|parent| {
        IsWindow(Some(parent)).as_bool() && GetParent(widget).ok() == Some(parent)
    });
    if !parent_is_valid {
        (*state_pointer).taskbar_parent = None;
    }
    match set_layered_mode(widget, true) {
        Ok(()) => {
            if let Ok(parent) = attach_to_taskbar(widget, settings.taskbar_offset) {
                let state = &mut *state_pointer;
                state.taskbar_parent = Some(parent);
                state.lifecycle.widget_attached_to_taskbar();
                let _ = ShowWindow(widget, SW_SHOWNA);
                let snapshot = state.backend.snapshot();
                match paint_taskbar_widget(widget, &snapshot, state.hover.value()) {
                    Ok(()) => return Ok(()),
                    Err(error) => log_taskbar_render_error("compose", &error),
                }
            }
        }
        Err(error) => log_taskbar_render_error("style", &error),
    }
    // 작업표시줄 부착에 실패하면 위젯을 숨기고 트레이 아이콘만 유지합니다.
    // 1초 타이머와 TaskbarCreated 메시지에서 작업표시줄 재부착을 계속 재시도합니다.
    let widget = (*state_pointer).widget;
    let _ = ShowWindow(widget, SW_HIDE);
    Ok(())
}

unsafe fn set_layered_mode(widget: HWND, enabled: bool) -> io::Result<()> {
    let extended_style = GetWindowLongPtrW(widget, GWL_EXSTYLE) as u32;
    let desired_style = if enabled {
        extended_style | WS_EX_LAYERED.0
    } else {
        extended_style & !WS_EX_LAYERED.0
    };
    SetWindowLongPtrW(widget, GWL_EXSTYLE, desired_style as isize);
    if GetWindowLongPtrW(widget, GWL_EXSTYLE) as u32 != desired_style {
        return Err(io::Error::other("layered window style verification failed"));
    }
    SetWindowPos(
        widget,
        None,
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
    )
    .map_err(win_error)
}

unsafe fn validate_paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let _ = BeginPaint(hwnd, &mut paint);
    let _ = EndPaint(hwnd, &paint);
}

unsafe fn paint_taskbar_widget(hwnd: HWND, view: &WidgetViewModel, hover: u8) -> io::Result<()> {
    let mut client = RECT::default();
    GetClientRect(hwnd, &mut client).map_err(win_error)?;
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width <= 0 || height <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "taskbar widget has an empty client area",
        ));
    }
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid layered bitmap size"))?;

    let screen_dc = GetDC(None);
    let memory_dc = CreateCompatibleDC(Some(screen_dc));
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap = match CreateDIBSection(
        Some(memory_dc),
        &bitmap_info,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
    ) {
        Ok(bitmap) => bitmap,
        Err(error) => {
            let error = win_error(error);
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err(error);
        }
    };
    if bitmap.is_invalid() || bits.is_null() {
        if !bitmap.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        let _ = DeleteDC(memory_dc);
        let _ = ReleaseDC(None, screen_dc);
        return Err(io::Error::last_os_error());
    }

    let old_bitmap = SelectObject(memory_dc, HGDIOBJ(bitmap.0));
    let dpi = GetDpiForWindow(hwnd).max(96);
    paint_compact_taskbar_content(
        memory_dc,
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        dpi,
        view,
    );
    apply_glass_alpha(
        std::slice::from_raw_parts_mut(bits.cast::<u32>(), pixel_count),
        width,
        height,
        dpi,
        hover,
    );

    let source = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1,
    };
    let result = UpdateLayeredWindow(
        hwnd,
        Some(screen_dc),
        None,
        Some(&size),
        Some(memory_dc),
        Some(&source),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    )
    .map_err(win_error);
    SelectObject(memory_dc, old_bitmap);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(memory_dc);
    let _ = ReleaseDC(None, screen_dc);
    result
}

fn rounded_material_alpha(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: i32,
    base_alpha: u8,
) -> u8 {
    let px = f64::from(x) + 0.5;
    let py = f64::from(y) + 0.5;
    let radius = f64::from(radius.max(1));
    let center_x = if px < radius {
        radius
    } else if px > f64::from(width) - radius {
        f64::from(width) - radius
    } else {
        px
    };
    let center_y = if py < radius {
        radius
    } else if py > f64::from(height) - radius {
        f64::from(height) - radius
    } else {
        py
    };
    let distance = ((px - center_x).powi(2) + (py - center_y).powi(2)).sqrt();
    let coverage = (radius - distance).clamp(0.0, 1.0);
    (f64::from(base_alpha) * coverage).round() as u8
}

fn glass_noise(x: i32, y: i32) -> i32 {
    let hash = x
        .wrapping_mul(73_856_093)
        .wrapping_add(y.wrapping_mul(19_349_663))
        .wrapping_add(83_492_791);
    hash.rem_euclid(5) - 2
}

fn apply_glass_alpha(pixels: &mut [u32], width: i32, height: i32, dpi: u32, hover: u8) {
    const MATERIAL_RGB: u32 = 0x0028_2828;
    let base_alpha = 174_u8.saturating_add(((u16::from(hover) * 14) / 255) as u8);
    let radius = logical_to_physical(10, dpi).min(width.min(height) / 2);
    for y in 0..height {
        for x in 0..width {
            let index = (y as usize) * (width as usize) + x as usize;
            let pixel = pixels[index];
            let coverage = rounded_material_alpha(x, y, width, height, radius, 255);
            if coverage == 0 {
                pixels[index] = 0;
                continue;
            }
            let rgb = pixel & 0x00ff_ffff;
            let alpha = if rgb == MATERIAL_RGB {
                ((u16::from(base_alpha) * u16::from(coverage)) / 255) as u8
            } else {
                ((235_u16 * u16::from(coverage)) / 255) as u8
            };
            let noise = if rgb == MATERIAL_RGB {
                glass_noise(x, y) + i32::from(y <= 1) * 2
            } else {
                0
            };
            let blue =
                (((pixel & 0xff) as i32 + noise).clamp(0, 255) as u32 * u32::from(alpha)) / 255;
            let green = ((((pixel >> 8) & 0xff) as i32 + noise).clamp(0, 255) as u32
                * u32::from(alpha))
                / 255;
            let red = ((((pixel >> 16) & 0xff) as i32 + noise).clamp(0, 255) as u32
                * u32::from(alpha))
                / 255;
            pixels[index] = (u32::from(alpha) << 24) | (red << 16) | (green << 8) | blue;
        }
    }
}

unsafe fn paint_compact_taskbar_content(dc: HDC, client: RECT, dpi: u32, view: &WidgetViewModel) {
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let layout = TaskbarLayout::for_size(width, height, dpi);
    let row = select_weekly_row(view.primary.as_ref(), view.secondary.as_ref());
    let risk = match view.data_state {
        WidgetDataState::Loading => TaskbarRisk::Loading,
        WidgetDataState::Error => TaskbarRisk::Error,
        WidgetDataState::Ready => row
            .map(|row| TaskbarRisk::from_percent(row.used_percent))
            .unwrap_or(TaskbarRisk::Loading),
    };
    let accent = taskbar_risk_color(risk);

    let background = CreateSolidBrush(COLORREF(0x0028_2828));
    FillRect(dc, &client, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let _ = SetBkMode(dc, TRANSPARENT);

    if risk == TaskbarRisk::Error {
        let font = CreateFontW(
            -logical_to_physical(11, dpi),
            0,
            0,
            0,
            FW_MEDIUM.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            PROOF_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
            w!("Segoe UI Variable"),
        );
        let old = SelectObject(dc, HGDIOBJ(font.0));
        let mut dot = native_rect(layout.dot);
        draw_text(
            dc,
            "!",
            &mut dot,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            accent,
        );
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    } else {
        let brush = CreateSolidBrush(accent);
        let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let dot = native_rect(layout.dot);
        let _ = Ellipse(dc, dot.left, dot.top, dot.right, dot.bottom);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }

    let label_font = CreateFontW(
        -logical_to_physical(12, dpi),
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
        w!("Segoe UI Variable"),
    );
    let old_font = SelectObject(dc, HGDIOBJ(label_font.0));
    let mut label = native_rect(layout.label);
    draw_text(
        dc,
        &view.taskbar_label,
        &mut label,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        COLORREF(0x00ed_eded),
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(label_font.0));

    let percent_font = CreateFontW(
        -logical_to_physical(12, dpi),
        0,
        0,
        0,
        FW_MEDIUM.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        PROOF_QUALITY,
        u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
        w!("Segoe UI Variable"),
    );
    let old_font = SelectObject(dc, HGDIOBJ(percent_font.0));
    let mut percent = native_rect(layout.percent);
    draw_text(
        dc,
        row.map_or("--", |row| row.percent_text.as_str()),
        &mut percent,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
        COLORREF(0x00f5_f5f5),
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(percent_font.0));

    let track = CreateSolidBrush(COLORREF(0x0042_4242));
    FillRect(dc, &native_rect(layout.progress), track);
    let _ = DeleteObject(HGDIOBJ(track.0));
    if let Some(row) = row {
        let fill_width = progress_fill_width(layout.progress.width(), row.display_percent);
        if fill_width > 0 {
            let fill = CreateSolidBrush(accent);
            FillRect(
                dc,
                &RECT {
                    right: layout.progress.left + fill_width,
                    ..native_rect(layout.progress)
                },
                fill,
            );
            let _ = DeleteObject(HGDIOBJ(fill.0));
        }
    }
}

const fn taskbar_risk_color(risk: TaskbarRisk) -> COLORREF {
    match risk {
        TaskbarRisk::Healthy => COLORREF(0x0074_c748),
        TaskbarRisk::Warning => COLORREF(0x0023_a6f5),
        TaskbarRisk::Critical | TaskbarRisk::Error => COLORREF(0x005c_5cff),
        TaskbarRisk::Loading => COLORREF(0x0097_9797),
    }
}

fn log_taskbar_render_error(stage: &'static str, error: &io::Error) {
    let _ = DiagnosticLogger::new().record_safe(SafeDiagnostic::TaskbarRender {
        stage,
        error_code: error.raw_os_error(),
    });
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

#[cfg(test)]
mod tests {
    use super::{
        glass_noise, rounded_material_alpha, should_open_tray_menu, NIN_SELECT, WM_CONTEXTMENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONUP, WM_RBUTTONUP};

    #[test]
    fn tray_menu_uses_only_version_4_activation_events() {
        assert!(should_open_tray_menu(WM_CONTEXTMENU));
        assert!(should_open_tray_menu(NIN_SELECT));
        assert!(!should_open_tray_menu(WM_RBUTTONUP));
        assert!(!should_open_tray_menu(WM_LBUTTONUP));
    }

    #[test]
    fn rounded_material_alpha_softens_corners_and_keeps_center_translucent() {
        assert_eq!(rounded_material_alpha(0, 0, 208, 48, 10, 174), 0);
        assert_eq!(rounded_material_alpha(104, 24, 208, 48, 10, 174), 174);
        let edge = rounded_material_alpha(3, 3, 208, 48, 10, 174);
        assert!(edge > 0 && edge < 174);
    }

    #[test]
    fn glass_noise_is_deterministic_and_subtle() {
        for y in 0..48 {
            for x in 0..208 {
                let first = glass_noise(x, y);
                assert_eq!(first, glass_noise(x, y));
                assert!((-2..=2).contains(&first));
            }
        }
    }
}
