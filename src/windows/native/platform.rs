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
            DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
            DT_VCENTER, FF_SWISS, FW_MEDIUM, FW_NORMAL, HDC, HGDIOBJ, NULL_PEN, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, PROOF_QUALITY, TRANSPARENT,
        },
        System::{
            Console::{AttachConsole, ATTACH_PARENT_PROCESS},
            LibraryLoader::GetModuleHandleW,
            Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
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
                WINDOW_STYLE, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_DPICHANGED, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETTINGCHANGE,
                WM_THEMECHANGED, WM_TIMER, WNDCLASSW, WS_CLIPSIBLINGS, WS_EX_LAYERED,
                WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::diagnostics::{DiagnosticLogger, SafeDiagnostic};

use super::super::{
    is_exact_github_tag_page,
    lifecycle::{CleanupAction, NativeLifecycle, RecoveryEvent},
    taskbar::{
        attach_to_taskbar, reposition_taskbar_widget, TaskbarObserver, TaskbarTarget,
        TASKBAR_LAYOUT_CHANGED,
    },
    taskbar_widget::{
        progress_fill_width, select_weekly_row, HoverTransition, TaskbarLayout, TaskbarLayoutMode,
        TaskbarRisk, TASKBAR_WIDTH_LOGICAL,
    },
    tray::{AsyncTrayIcon, TrayIcon, TRAY_CALLBACK},
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
    widgets: Vec<WidgetSlot>,
    taskbar_observer: Option<TaskbarObserver>,
    tray: Option<AsyncTrayIcon>,
    settings: UiSettings,
    lifecycle: NativeLifecycle,
}

struct WidgetSlot {
    hwnd: HWND,
    taskbar_parent: HWND,
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
            widgets: Vec::new(),
            taskbar_observer: None,
            tray: None,
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
            state.taskbar_observer = Some(TaskbarObserver::start(owner)?);
            let snapshot = state.backend.snapshot();
            state.tray = Some(AsyncTrayIcon::new(
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
    drop((*state_pointer).taskbar_observer.take());
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
                destroy_all_widgets(state_pointer);
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
            WM_TIMER
                | TRAY_CALLBACK
                | WM_CLOSE
                | WM_DESTROY
                | TASKBAR_LAYOUT_CHANGED
                | WM_DISPLAYCHANGE
                | WM_SETTINGCHANGE
                | WM_THEMECHANGED
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
        if let Some(observer) = &(*pointer).taskbar_observer {
            observer.invalidate();
        }
        return LRESULT(0);
    }

    match message {
        WM_TIMER if wparam.0 == TIMER_ID => {
            let settings = (*pointer).backend.settings();
            (*pointer).settings = settings;
            let _ = refresh_tray(pointer, false);
            update_tooltips(pointer);
            let _ = recover_widget(pointer, RecoveryEvent::Timer);
            let state = &*pointer;
            if state.settings.widget_visible {
                let snapshot = state.backend.snapshot();
                for widget in &state.widgets {
                    let _ = paint_taskbar_widget(widget.hwnd, &snapshot, widget.hover.value());
                }
            }
            LRESULT(0)
        }
        TASKBAR_LAYOUT_CHANGED => {
            let _ = apply_window_policy(pointer);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_THEMECHANGED => {
            if let Some(observer) = &(*pointer).taskbar_observer {
                observer.refresh();
            }
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
            if let Some(index) = state.widgets.iter().position(|widget| widget.hwnd == hwnd) {
                let widget = state.widgets.remove(index);
                if widget.tooltip != HWND::default() && IsWindow(Some(widget.tooltip)).as_bool() {
                    let _ = DestroyWindow(widget.tooltip);
                }
            }
            if state.widgets.is_empty() {
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
        WM_MOUSEMOVE => {
            let Some(widget) = widget_slot(pointer, hwnd) else {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            };
            let widget = &mut *widget;
            widget.hover.set_hovered(true);
            if !widget.mouse_tracking {
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                if TrackMouseEvent(&mut tracking).is_ok() {
                    widget.mouse_tracking = true;
                }
            }
            let _ = SetTimer(Some(hwnd), HOVER_TIMER_ID, 15, None);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let Some(widget) = widget_slot(pointer, hwnd) else {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            };
            let widget = &mut *widget;
            widget.mouse_tracking = false;
            widget.hover.set_hovered(false);
            let _ = SetTimer(Some(hwnd), HOVER_TIMER_ID, 15, None);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == HOVER_TIMER_ID => {
            let Some(widget) = widget_slot(pointer, hwnd) else {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            };
            let widget = &mut *widget;
            let needs_more = widget.hover.tick();
            let _ = InvalidateRect(Some(hwnd), None, false);
            if !needs_more {
                let _ = KillTimer(Some(hwnd), HOVER_TIMER_ID);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let snapshot = (*pointer).backend.snapshot();
            let hover = widget_slot(pointer, hwnd)
                .map(|widget| (*widget).hover.value())
                .unwrap_or_default();
            validate_paint(hwnd);
            let _ = paint_taskbar_widget(hwnd, &snapshot, hover);
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

unsafe fn widget_slot(state_pointer: *mut NativeState<'_>, hwnd: HWND) -> Option<*mut WidgetSlot> {
    (&mut *state_pointer)
        .widgets
        .iter_mut()
        .find(|widget| widget.hwnd == hwnd)
        .map(|widget| widget as *mut WidgetSlot)
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

unsafe fn create_widget(
    state_pointer: *mut NativeState<'_>,
    target: TaskbarTarget,
) -> io::Result<HWND> {
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
    let was_empty = (*state_pointer).widgets.is_empty();
    (*state_pointer).widgets.push(WidgetSlot {
        hwnd: widget,
        taskbar_parent: target.parent,
        hover: HoverTransition::default(),
        mouse_tracking: false,
        tooltip: HWND::default(),
        tooltip_text: Vec::new(),
    });
    if was_empty {
        let state = &mut *state_pointer;
        state.lifecycle.widget_created();
    }
    if let Err(error) =
        set_layered_mode(widget, true).and_then(|()| attach_to_taskbar(widget, target))
    {
        let _ = DestroyWindow(widget);
        return Err(error);
    }
    (*state_pointer).lifecycle.widget_attached_to_taskbar();
    let _ = create_tooltip(state_pointer, widget);
    Ok(widget)
}

unsafe fn create_tooltip(state_pointer: *mut NativeState<'_>, widget: HWND) -> io::Result<()> {
    let existing = widget_slot(state_pointer, widget)
        .map(|slot| (*slot).tooltip)
        .unwrap_or_default();
    if existing != HWND::default() && IsWindow(Some(existing)).as_bool() {
        let _ = DestroyWindow(existing);
    }
    let (owner, instance) = {
        let state = &*state_pointer;
        (state.owner, state.instance)
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
    let tooltip_text: Vec<u16> = (*state_pointer)
        .backend
        .snapshot()
        .taskbar_tooltip
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let slot = widget_slot(state_pointer, widget)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "widget slot unavailable"))?;
    (*slot).tooltip = tooltip;
    (*slot).tooltip_text = tooltip_text;
    let tool = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: owner,
        uId: widget.0 as usize,
        hinst: instance,
        lpszText: PWSTR((*slot).tooltip_text.as_mut_ptr()),
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
        (*slot).tooltip = HWND::default();
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

unsafe fn update_tooltips(state_pointer: *mut NativeState<'_>) {
    let tooltip_text: Vec<u16> = (*state_pointer)
        .backend
        .snapshot()
        .taskbar_tooltip
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let state = &mut *state_pointer;
    for widget in &mut state.widgets {
        widget.tooltip_text = tooltip_text.clone();
        if widget.tooltip == HWND::default() {
            continue;
        }
        let tool = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: state.owner,
            uId: widget.hwnd.0 as usize,
            hinst: state.instance,
            lpszText: PWSTR(widget.tooltip_text.as_mut_ptr()),
            ..Default::default()
        };
        let _ = SendMessageW(
            widget.tooltip,
            TTM_UPDATETIPTEXTW,
            Some(WPARAM(0)),
            Some(LPARAM((&tool as *const TTTOOLINFOW) as isize)),
        );
    }
}

unsafe fn recover_widget(
    state_pointer: *mut NativeState<'_>,
    event: RecoveryEvent,
) -> io::Result<()> {
    if !(*state_pointer).settings.widget_visible {
        return Ok(());
    }
    let targets = desired_taskbars(&*state_pointer);
    if matches!(event, RecoveryEvent::TaskbarCreated)
        || !widgets_match_targets(&(*state_pointer).widgets, &targets)
    {
        apply_window_policy(state_pointer)?;
    } else {
        reposition_widgets(&(*state_pointer).widgets, &targets);
    }
    Ok(())
}

unsafe fn refresh_tray(state_pointer: *mut NativeState<'_>, restore: bool) -> io::Result<()> {
    let snapshot = (*state_pointer).backend.snapshot();
    let tray = (*state_pointer)
        .tray
        .as_ref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tray icon unavailable"))?;
    if restore {
        tray.restore(highest_percent(&snapshot), &snapshot.status);
    } else {
        tray.update(highest_percent(&snapshot), &snapshot.status);
    }
    Ok(())
}

unsafe fn apply_window_policy(state_pointer: *mut NativeState<'_>) -> io::Result<()> {
    let settings = (*state_pointer).settings.clone();
    if !settings.widget_visible {
        for widget in &(*state_pointer).widgets {
            let _ = ShowWindow(widget.hwnd, SW_HIDE);
        }
        return Ok(());
    }
    let targets = desired_taskbars(&*state_pointer);
    if widgets_match_targets(&(*state_pointer).widgets, &targets) {
        reposition_widgets(&(*state_pointer).widgets, &targets);
        let state = &*state_pointer;
        let snapshot = state.backend.snapshot();
        for widget in &state.widgets {
            let _ = ShowWindow(widget.hwnd, SW_SHOWNA);
            if let Err(error) = paint_taskbar_widget(widget.hwnd, &snapshot, widget.hover.value()) {
                log_taskbar_render_error("compose", &error);
            }
        }
        return Ok(());
    }
    destroy_all_widgets(state_pointer);
    let snapshot = (*state_pointer).backend.snapshot();
    for target in targets {
        match create_widget(state_pointer, target) {
            Ok(widget) => {
                let _ = ShowWindow(widget, SW_SHOWNA);
                if let Err(error) = paint_taskbar_widget(widget, &snapshot, 0) {
                    log_taskbar_render_error("compose", &error);
                }
            }
            Err(error) => log_taskbar_render_error("attach", &error),
        }
    }
    Ok(())
}

unsafe fn desired_taskbars(state: &NativeState<'_>) -> Vec<TaskbarTarget> {
    let mut targets = state
        .taskbar_observer
        .as_ref()
        .map(|observer| observer.targets(state.settings.taskbar_offset))
        .unwrap_or_default();
    if state.settings.taskbar_display_mode == crate::TaskbarDisplayMode::Primary {
        targets.truncate(1);
    }
    targets
}

unsafe fn widgets_match_targets(widgets: &[WidgetSlot], targets: &[TaskbarTarget]) -> bool {
    widgets.len() == targets.len()
        && widgets.iter().zip(targets).all(|(widget, target)| {
            widget.taskbar_parent == target.parent
                && IsWindow(Some(widget.hwnd)).as_bool()
                && IsWindow(Some(target.parent)).as_bool()
                && GetParent(widget.hwnd).ok() == Some(target.parent)
        })
}

unsafe fn reposition_widgets(widgets: &[WidgetSlot], targets: &[TaskbarTarget]) {
    for (widget, target) in widgets.iter().zip(targets) {
        if let Err(error) = reposition_taskbar_widget(widget.hwnd, *target) {
            log_taskbar_render_error("position", &error);
        }
    }
}

unsafe fn destroy_all_widgets(state_pointer: *mut NativeState<'_>) {
    let windows: Vec<HWND> = (*state_pointer)
        .widgets
        .iter()
        .map(|widget| widget.hwnd)
        .collect();
    for widget in windows {
        if IsWindow(Some(widget)).as_bool() {
            let _ = DestroyWindow(widget);
        }
    }
    if !(*state_pointer).widgets.is_empty() {
        (*state_pointer).widgets.clear();
        (*state_pointer).lifecycle.widget_destroyed();
    }
}

unsafe fn set_layered_mode(widget: HWND, enabled: bool) -> io::Result<()> {
    let extended_style = GetWindowLongPtrW(widget, GWL_EXSTYLE) as u32;
    let desired_style = if enabled {
        extended_style | WS_EX_LAYERED.0
    } else {
        extended_style & !WS_EX_LAYERED.0
    };
    if desired_style == extended_style {
        return Ok(());
    }
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
    let palette = taskbar_palette(system_uses_light_theme());
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
        palette,
    );
    apply_glass_alpha(
        std::slice::from_raw_parts_mut(bits.cast::<u32>(), pixel_count),
        width,
        height,
        dpi,
        hover,
        palette.material,
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

fn apply_glass_alpha(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    dpi: u32,
    hover: u8,
    material_rgb: u32,
) {
    // 투명한 기본 재질은 실제 작업표시줄 색과 배경 효과를 그대로 통과시킵니다.
    // 마우스를 올렸을 때만 매우 옅은 표면을 추가해 클릭 영역을 드러냅니다.
    let base_alpha = ((u16::from(hover) * 28) / 255) as u8;
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
            let alpha = if rgb == material_rgb {
                ((u16::from(base_alpha) * u16::from(coverage)) / 255) as u8
            } else {
                ((235_u16 * u16::from(coverage)) / 255) as u8
            };
            let noise = if rgb == material_rgb {
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

#[derive(Clone, Copy)]
struct TaskbarPalette {
    material: u32,
    label: u32,
    percent: u32,
    track: u32,
}

const fn taskbar_palette(light: bool) -> TaskbarPalette {
    if light {
        TaskbarPalette {
            material: 0x00f3_f3f3,
            label: 0x0020_2020,
            percent: 0x0010_1010,
            track: 0x00c7_c7c7,
        }
    } else {
        TaskbarPalette {
            material: 0x0028_2828,
            label: 0x00ed_eded,
            percent: 0x00f5_f5f5,
            track: 0x0042_4242,
        }
    }
}

fn system_uses_light_theme() -> bool {
    unsafe {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
            && value != 0
    }
}

unsafe fn paint_compact_taskbar_content(
    dc: HDC,
    client: RECT,
    dpi: u32,
    view: &WidgetViewModel,
    palette: TaskbarPalette,
) {
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

    let background = CreateSolidBrush(COLORREF(palette.material));
    FillRect(dc, &client, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let _ = SetBkMode(dc, TRANSPARENT);

    if risk == TaskbarRisk::Error && layout.dot.is_some() {
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
        let mut dot = native_rect(layout.dot.expect("checked above"));
        draw_text(
            dc,
            "!",
            &mut dot,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            accent,
        );
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    } else if let Some(dot) = layout.dot {
        let brush = CreateSolidBrush(accent);
        let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let dot = native_rect(dot);
        let _ = Ellipse(dc, dot.left, dot.top, dot.right, dot.bottom);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }

    if let Some(label) = layout.label {
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
        let mut label = native_rect(label);
        draw_text(
            dc,
            &view.taskbar_label,
            &mut label,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            COLORREF(palette.label),
        );
        SelectObject(dc, old_font);
        let _ = DeleteObject(HGDIOBJ(label_font.0));
    }

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
    let percent_alignment = if layout.mode == TaskbarLayoutMode::Minimal {
        DT_CENTER
    } else {
        DT_RIGHT
    };
    draw_text(
        dc,
        row.map_or("--", |row| row.percent_text.as_str()),
        &mut percent,
        percent_alignment | DT_SINGLELINE | DT_VCENTER,
        COLORREF(palette.percent),
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(percent_font.0));

    let track = CreateSolidBrush(COLORREF(palette.track));
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
        glass_noise, rounded_material_alpha, should_open_tray_menu, taskbar_palette, NIN_SELECT,
        WM_CONTEXTMENU,
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

    #[test]
    fn taskbar_palette_keeps_text_legible_on_both_system_themes() {
        let light = taskbar_palette(true);
        let dark = taskbar_palette(false);
        assert_ne!(light.material, dark.material);
        assert!(light.label < light.material);
        assert!(dark.label > dark.material);
    }
}
