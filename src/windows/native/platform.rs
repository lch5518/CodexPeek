use std::{
    cell::Cell,
    io,
    sync::atomic::{AtomicU32, Ordering},
    sync::mpsc::{Receiver, TryRecvError},
};

use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, COLORREF, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND,
            LPARAM, LRESULT, POINT, RECT, RPC_E_CHANGED_MODE, SIZE, WPARAM,
        },
        Globalization::{GetUserDefaultLocaleName, GetUserDefaultUILanguage},
        Graphics::Gdi::{
            BeginPaint, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush,
            DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, GetDC, GetMonitorInfoW,
            GetStockObject, InvalidateRect, MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode,
            SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CLIP_DEFAULT_PRECIS,
            DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
            DT_RIGHT, DT_RTLREADING, DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_MEDIUM, FW_NORMAL,
            HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, NULL_PEN, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, PROOF_QUALITY, TRANSPARENT,
        },
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
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
                SendMessageW, SetParent, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
                ShowWindow, TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW, CS_HREDRAW,
                CS_VREDRAW, CW_USEDEFAULT, GWLP_HWNDPARENT, GWLP_USERDATA, GWL_EXSTYLE, GWL_STYLE,
                HWND_TOPMOST, IDC_ARROW, IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK,
                MB_SETFOREGROUND, MB_TASKMODAL, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MSG,
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE,
                SW_SHOWNA, SW_SHOWNORMAL, ULW_ALPHA, WINDOW_STYLE, WM_CLOSE, WM_CONTEXTMENU,
                WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_MOUSEMOVE, WM_NCCREATE,
                WM_NCDESTROY, WM_PAINT, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER, WNDCLASSW,
                WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

use crate::diagnostics::{DiagnosticLogger, SafeDiagnostic};
use crate::{localized_text, Language, LocalizationKey, UpdateCheckNotice};

use super::super::{
    is_exact_github_tag_page,
    lifecycle::{CleanupAction, NativeLifecycle, RecoveryEvent},
    profile_dialog::{
        confirm_profile_login_owned, show_profile_manager_owned_live, show_profile_message,
        ProfileMessageRoute,
    },
    taskbar::{
        attach_to_taskbar, reconcile_widget_surfaces, reposition_taskbar_widget, TaskbarObserver,
        TaskbarTarget, WidgetSurface, WidgetSurfaceBackend, TASKBAR_LAYOUT_CHANGED,
    },
    taskbar_widget::{
        profile_header_text, progress_fill_width, select_weekly_row, taskbar_visual_state,
        tooltip_text_needs_update, widget_surface_layout, HoverTransition, TaskbarIndicator,
        TaskbarLayout, TaskbarLayoutMode, TaskbarRisk, TASKBAR_WIDTH_LOGICAL,
    },
    theme,
    tray::{AsyncTrayIcon, TrayIcon, TRAY_CALLBACK},
    widget::{logical_to_physical, Rect},
    UiAction, UiBackend, UiSettings, WidgetViewModel,
};
use super::{
    profile_dialog_ui_action, profile_login_confirmation_request,
    usage_forecast_clear_confirmation_request, ProfileLoginDispatch,
};

const TIMER_ID: usize = 1;
const HOVER_TIMER_ID: usize = 2;
const TASKBAR_STARTUP_RETRIES: u8 = 5;
const TASKBAR_RECONCILE_TICKS: u8 = 30;
const OWNER_CLASS: PCWSTR = w!("CodexUsageMonitor.Hidden.v1");
const WIDGET_CLASS: PCWSTR = w!("CodexUsageMonitor.Widget.v1");
static TASKBAR_CREATED_MESSAGE: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static UPDATE_DIALOG_IN_PROGRESS: Cell<bool> = const { Cell::new(false) };
}

struct UpdateDialogGuard;

impl UpdateDialogGuard {
    fn acquire() -> Option<Self> {
        UPDATE_DIALOG_IN_PROGRESS.with(|in_progress| {
            if in_progress.replace(true) {
                None
            } else {
                Some(Self)
            }
        })
    }
}

impl Drop for UpdateDialogGuard {
    fn drop(&mut self) {
        UPDATE_DIALOG_IN_PROGRESS.with(|in_progress| in_progress.set(false));
    }
}

fn update_dialog_in_progress() -> bool {
    UPDATE_DIALOG_IN_PROGRESS.with(Cell::get)
}

struct TaskbarRefreshSchedule {
    startup_retries_remaining: u8,
    reconcile_ticks: u8,
}

impl TaskbarRefreshSchedule {
    const fn new() -> Self {
        Self {
            startup_retries_remaining: TASKBAR_STARTUP_RETRIES,
            reconcile_ticks: 0,
        }
    }

    fn tick(&mut self) -> bool {
        if self.startup_retries_remaining > 0 {
            self.startup_retries_remaining -= 1;
            return true;
        }
        self.reconcile_ticks = self.reconcile_ticks.saturating_add(1);
        if self.reconcile_ticks >= TASKBAR_RECONCILE_TICKS {
            self.reconcile_ticks = 0;
            true
        } else {
            false
        }
    }
}

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
    tray_shutdown: Option<Receiver<()>>,
    shutting_down: bool,
    settings: UiSettings,
    lifecycle: NativeLifecycle,
    taskbar_refresh_schedule: TaskbarRefreshSchedule,
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
            tray_shutdown: None,
            shutting_down: false,
            settings,
            lifecycle: NativeLifecycle::default(),
            taskbar_refresh_schedule: TaskbarRefreshSchedule::new(),
        });
        let state_pointer = (&mut *state as *mut NativeState<'_>).cast();
        let result = (|| {
            let owner_title = localized_window_title(state.settings.resolved_language);
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                OWNER_CLASS,
                PCWSTR(owner_title.as_ptr()),
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

    if message == TRAY_CALLBACK && update_dialog_in_progress() {
        return LRESULT(0);
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
            if (*pointer).shutting_down {
                if tray_shutdown_complete(pointer) {
                    PostQuitMessage(0);
                }
                return LRESULT(0);
            }
            let settings = (*pointer).backend.settings();
            let language_changed =
                settings.resolved_language != (*pointer).settings.resolved_language;
            (*pointer).settings = settings;
            show_pending_update_notice_if_ready(pointer);
            if language_changed {
                update_window_titles(pointer);
            }
            if (*pointer).taskbar_refresh_schedule.tick() {
                if let Some(observer) = &(*pointer).taskbar_observer {
                    observer.refresh();
                }
            }
            let _ = refresh_tray(pointer, false);
            update_tooltips(pointer);
            let _ = recover_widget(pointer, RecoveryEvent::Timer);
            let state = &*pointer;
            if state.settings.widget_visible {
                let snapshot = state.backend.snapshot();
                let rtl = matches!(state.settings.resolved_language, crate::Language::Arabic);
                for widget in &state.widgets {
                    let _ = paint_taskbar_widget(
                        widget.hwnd,
                        &snapshot,
                        widget.hover.value(),
                        rtl,
                        widget_is_attached_to_taskbar(widget),
                    );
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
                show_settings_menu(pointer);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            begin_exit(pointer);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

const fn should_open_tray_menu(event: u32) -> bool {
    matches!(event, WM_CONTEXTMENU | NIN_SELECT)
}

unsafe fn show_settings_menu(pointer: *mut NativeState<'_>) {
    if update_dialog_in_progress() {
        return;
    }

    let snapshot = (*pointer).backend.snapshot();
    (*pointer).settings = (*pointer).backend.settings();
    let (owner, settings) = {
        let state = &*pointer;
        (state.owner, state.settings.clone())
    };
    let action = TrayIcon::show_menu(owner, &settings, snapshot.reset_credits_text.as_deref());
    if let Some(action) = action {
        dispatch_action(pointer, action);
    }
}

const fn should_open_widget_menu(message: u32) -> bool {
    message == WM_CONTEXTMENU
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
    ) && !should_open_widget_menu(message)
    {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_CONTEXTMENU => {
            show_settings_menu(pointer);
            LRESULT(0)
        }
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
            let (hover, attached_to_taskbar) = widget_slot(pointer, hwnd)
                .map(|widget| {
                    let widget = &*widget;
                    (widget.hover.value(), widget_is_attached_to_taskbar(widget))
                })
                .unwrap_or((0, false));
            let rtl = matches!(
                (*pointer).settings.resolved_language,
                crate::Language::Arabic
            );
            validate_paint(hwnd);
            let _ = paint_taskbar_widget(hwnd, &snapshot, hover, rtl, attached_to_taskbar);
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
    (*state_pointer)
        .widgets
        .iter_mut()
        .find(|widget| widget.hwnd == hwnd)
        .map(|widget| widget as *mut WidgetSlot)
}

unsafe fn widget_is_attached_to_taskbar(widget: &WidgetSlot) -> bool {
    widget.taskbar_parent != HWND::default()
        && IsWindow(Some(widget.taskbar_parent)).as_bool()
        && GetParent(widget.hwnd).ok() == Some(widget.taskbar_parent)
}

unsafe fn store_state(hwnd: HWND, lparam: LPARAM) {
    let create = &*(lparam.0 as *const CREATESTRUCTW);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
}

unsafe fn dispatch_action(state_pointer: *mut NativeState<'_>, action: UiAction) {
    if action == UiAction::Exit {
        begin_exit(state_pointer);
        return;
    }
    if matches!(
        action,
        UiAction::OpenAddUsageProfile | UiAction::OpenManageUsageProfiles
    ) {
        open_profile_dialog(state_pointer);
        return;
    }
    let settings = if let Some(request) = usage_forecast_clear_confirmation_request(&action) {
        let latest = (*state_pointer).backend.settings();
        (*state_pointer).settings = latest;
        let confirmed = match confirm_usage_forecast_clear_owned(
            (*state_pointer).owner,
            (*state_pointer).settings.resolved_language,
        ) {
            Ok(confirmed) => confirmed,
            Err(_) => return,
        };
        let Some(action) = request.resolve(confirmed) else {
            return;
        };
        (*state_pointer).backend.dispatch(action)
    } else if matches!(
        action,
        UiAction::AddUsageProfile(_) | UiAction::Login | UiAction::LoginUsageProfile(_)
    ) {
        let latest = (*state_pointer).backend.settings();
        (*state_pointer).settings = latest;
        let Some(request) = profile_login_confirmation_request(&action, &(*state_pointer).settings)
        else {
            show_profile_dialog_error(
                (*state_pointer).owner,
                (*state_pointer).settings.resolved_language,
            );
            return;
        };
        let confirmed = match confirm_profile_login_owned(
            (*state_pointer).owner,
            request.label(),
            (*state_pointer).settings.resolved_language,
        ) {
            Ok(confirmed) => confirmed,
            Err(_) => {
                show_profile_dialog_error(
                    (*state_pointer).owner,
                    (*state_pointer).settings.resolved_language,
                );
                return;
            }
        };
        let Some(dispatch) = request.resolve(confirmed) else {
            return;
        };
        match dispatch {
            ProfileLoginDispatch::Normal(action) => (*state_pointer).backend.dispatch(action),
            ProfileLoginDispatch::Confirmed(action) => (*state_pointer)
                .backend
                .dispatch_confirmed_profile_login(action),
        }
    } else {
        (*state_pointer).backend.dispatch(action)
    };
    (*state_pointer).settings = settings;
    show_pending_update_notice_if_ready(state_pointer);
    update_window_titles(state_pointer);
    let _ = apply_window_policy(state_pointer);
    let _ = refresh_tray(state_pointer, false);
    update_tooltips(state_pointer);
}

/// 현재 UI 복사본으로 프로필 관리 창을 열고 검증된 결과만 백엔드에 전달합니다.
///
/// 대화상자 자체는 파일·설정·Codex 작업을 수행하지 않습니다. `UiBackend`는 반환된 타입 지정
/// 의도를 받아 장시간 작업을 워커에 예약해야 하며, 오류 문구에는 OS 코드나 프로필 식별 정보를
/// 포함하지 않습니다.
unsafe fn open_profile_dialog(state_pointer: *mut NativeState<'_>) {
    let settings = (*state_pointer).backend.settings();
    (*state_pointer).settings = settings;
    let owner = (*state_pointer).owner;
    let profiles = (*state_pointer).settings.usage_profiles.clone();
    let mutation_pending = (*state_pointer).settings.usage_profile_mutation_pending;
    let language = (*state_pointer).settings.resolved_language;
    let mut refresh = || {
        let settings = (*state_pointer).backend.settings();
        let snapshot = (
            settings.usage_profiles.clone(),
            settings.usage_profile_mutation_pending,
        );
        (*state_pointer).settings = settings;
        snapshot
    };
    let result =
        show_profile_manager_owned_live(owner, &profiles, mutation_pending, language, &mut refresh);
    match result {
        Ok(Some(action)) => dispatch_action(state_pointer, profile_dialog_ui_action(action)),
        Ok(None) => {}
        Err(_) => show_profile_dialog_error(owner, language),
    }
}

/// 네이티브 UI가 프로필 메시지와 일반 애플리케이션 메시지를 서로 다른 경계로 표시합니다.
trait NativeMessagePresenter {
    fn present_profile(
        &mut self,
        route: ProfileMessageRoute,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT>;

    fn present_application(
        &mut self,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT>;
}

struct WindowsNativeMessagePresenter;

impl NativeMessagePresenter for WindowsNativeMessagePresenter {
    fn present_profile(
        &mut self,
        route: ProfileMessageRoute,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT> {
        // SAFETY: 네이티브 UI 호출자는 기존과 동일한 owner 수명 계약을 지키며, 공통 경계가
        // 문자열을 소유 UTF-16 버퍼로 복사한 뒤 동기적으로 표시합니다.
        unsafe { show_profile_message(route, owner, message, title, style) }
    }

    fn present_application(
        &mut self,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT> {
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let owner = (owner != HWND::default()).then_some(owner);
        // SAFETY: 두 버퍼는 NUL 종료되어 있고 동기 `MessageBoxW` 호출이 반환될 때까지 살아
        // 있습니다. 유효한 숨은 소유 창을 전달해 대화상자가 트레이 앱과 함께 표시됩니다.
        let result = unsafe {
            MessageBoxW(
                owner,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                style,
            )
        };
        if result.0 == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result)
        }
    }
}

unsafe fn show_profile_dialog_error(owner: HWND, language: crate::Language) {
    let mut presenter = WindowsNativeMessagePresenter;
    show_profile_dialog_error_with_presenter(owner, language, &mut presenter);
}

/// 사용량 소진 예측 기록 삭제를 owner 창에 연결한 확인 대화상자로 표시합니다.
unsafe fn confirm_usage_forecast_clear_owned(
    owner: HWND,
    language: crate::Language,
) -> io::Result<bool> {
    let mut presenter = WindowsNativeMessagePresenter;
    confirm_usage_forecast_clear_with_presenter(owner, language, &mut presenter)
}

/// 사용량 소진 예측 기록 삭제 확인 문구를 일반 애플리케이션 메시지 경계로 전달합니다.
fn confirm_usage_forecast_clear_with_presenter<P: NativeMessagePresenter>(
    owner: HWND,
    language: crate::Language,
    presenter: &mut P,
) -> io::Result<bool> {
    let result = presenter.present_application(
        owner,
        localized_text(LocalizationKey::UsageForecastClearConfirm, language),
        localized_text(LocalizationKey::WindowTitle, language),
        MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND | MB_TASKMODAL,
    )?;
    Ok(result == IDYES)
}

/// 네이티브 프로필 작업 오류를 프로필 전용 가운데 배치 경계로 전달합니다.
fn show_profile_dialog_error_with_presenter<P: NativeMessagePresenter>(
    owner: HWND,
    language: crate::Language,
    presenter: &mut P,
) {
    let _ = presenter.present_profile(
        ProfileMessageRoute::NativeOperationError,
        owner,
        crate::localized_text(
            crate::LocalizationKey::UsageProfileOperationFailed,
            language,
        ),
        crate::localized_text(crate::LocalizationKey::WindowTitle, language),
        MB_OK | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
    );
}

/// 트레이 아이콘 삭제가 끝난 뒤에만 owner 창을 파괴하도록 비동기 종료를 시작합니다.
///
/// 셸 호출은 워커에서 계속 수행되므로 UI 스레드는 대기하지 않습니다. 트레이가 없으면 즉시
/// 메시지 루프를 종료합니다.
unsafe fn begin_exit(state_pointer: *mut NativeState<'_>) {
    if (*state_pointer).shutting_down {
        return;
    }
    (*state_pointer).shutting_down = true;
    (*state_pointer).tray_shutdown = (*state_pointer)
        .tray
        .as_ref()
        .map(AsyncTrayIcon::begin_shutdown);
    if (*state_pointer).tray_shutdown.is_none() {
        PostQuitMessage(0);
    }
}

/// 트레이 워커가 아이콘을 삭제했거나 비정상 종료했으면 종료 정리를 진행할 수 있는지 반환합니다.
unsafe fn tray_shutdown_complete(state_pointer: *mut NativeState<'_>) -> bool {
    let Some(receiver) = (*state_pointer).tray_shutdown.as_ref() else {
        return true;
    };
    matches!(
        receiver.try_recv(),
        Ok(()) | Err(TryRecvError::Disconnected)
    )
}

unsafe fn create_detached_widget(state_pointer: *mut NativeState<'_>) -> io::Result<HWND> {
    let (owner, instance) = {
        let state = &*state_pointer;
        (state.owner, state.instance)
    };
    // 분리 위젯은 프로필 헤더와 기존 48px 본문을 함께 표시합니다.
    let width = logical_to_physical(TASKBAR_WIDTH_LOGICAL, 96);
    let height = logical_to_physical(72, 96);
    let widget_title = localized_window_title((*state_pointer).settings.resolved_language);
    let widget = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        WIDGET_CLASS,
        PCWSTR(widget_title.as_ptr()),
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
        taskbar_parent: HWND::default(),
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
        set_layered_mode(widget, true).and_then(|()| position_detached_widget(widget))
    {
        let _ = DestroyWindow(widget);
        return Err(error);
    }
    let _ = create_tooltip(state_pointer, widget);
    Ok(widget)
}

unsafe fn attach_widget(
    state_pointer: *mut NativeState<'_>,
    widget: HWND,
    target: TaskbarTarget,
) -> io::Result<()> {
    if let Err(attach_error) = attach_to_taskbar(widget, target) {
        let _ = detach_widget(state_pointer, widget);
        return Err(attach_error);
    }
    let slot = widget_slot(state_pointer, widget)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "widget slot unavailable"))?;
    (*slot).taskbar_parent = target.parent;
    (*state_pointer).lifecycle.widget_attached_to_taskbar();
    Ok(())
}

unsafe fn detach_widget(state_pointer: *mut NativeState<'_>, widget: HWND) -> io::Result<()> {
    let owner = (*state_pointer).owner;
    SetParent(widget, None).map_err(win_error)?;
    let style = GetWindowLongPtrW(widget, GWL_STYLE) as u32;
    let detached_style = (style & !WS_CHILD.0) | WS_POPUP.0 | WS_CLIPSIBLINGS.0;
    SetWindowLongPtrW(widget, GWL_STYLE, detached_style as isize);
    SetWindowLongPtrW(widget, GWLP_HWNDPARENT, owner.0 as isize);
    let verified_style = GetWindowLongPtrW(widget, GWL_STYLE) as u32;
    if verified_style & (WS_CHILD.0 | WS_POPUP.0) != WS_POPUP.0
        || GetParent(widget).ok() != Some(owner)
    {
        return Err(io::Error::other(
            "detached widget style or owner verification failed",
        ));
    }
    position_detached_widget(widget)?;
    let slot = widget_slot(state_pointer, widget)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "widget slot unavailable"))?;
    (*slot).taskbar_parent = HWND::default();
    Ok(())
}

unsafe fn position_detached_widget(widget: HWND) -> io::Result<()> {
    let monitor = MonitorFromWindow(widget, MONITOR_DEFAULTTOPRIMARY);
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
        return Err(io::Error::last_os_error());
    }
    let dpi = GetDpiForWindow(widget).max(96);
    let width = logical_to_physical(TASKBAR_WIDTH_LOGICAL, dpi);
    let height = logical_to_physical(72, dpi);
    let margin = logical_to_physical(16, dpi);
    let work_area = monitor_info.rcWork;
    let x = (work_area.right - width - margin).max(work_area.left);
    let y = (work_area.bottom - height - margin).max(work_area.top);
    SetWindowPos(
        widget,
        Some(HWND_TOPMOST),
        x,
        y,
        width.min(work_area.right - work_area.left),
        height.min(work_area.bottom - work_area.top),
        SWP_FRAMECHANGED | SWP_NOACTIVATE,
    )
    .map_err(win_error)
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
        if !tooltip_text_needs_update(&widget.tooltip_text, &tooltip_text) {
            continue;
        }
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

struct NativeWidgetSurfaceBackend<'a> {
    state_pointer: *mut NativeState<'a>,
    targets: Vec<TaskbarTarget>,
}

impl WidgetSurfaceBackend for NativeWidgetSurfaceBackend<'_> {
    type Window = HWND;
    type Target = HWND;
    type Error = io::Error;

    fn surfaces(&self) -> Vec<(Self::Window, WidgetSurface<Self::Target>)> {
        unsafe {
            (*self.state_pointer)
                .widgets
                .iter()
                .filter(|widget| IsWindow(Some(widget.hwnd)).as_bool())
                .map(|widget| {
                    let surface = if widget_is_attached_to_taskbar(widget) {
                        WidgetSurface::Attached(widget.taskbar_parent)
                    } else {
                        WidgetSurface::Detached
                    };
                    (widget.hwnd, surface)
                })
                .collect()
        }
    }

    fn create_detached(&mut self) -> Result<Self::Window, Self::Error> {
        unsafe { create_detached_widget(self.state_pointer) }
    }

    fn attach(&mut self, window: Self::Window, target: Self::Target) -> Result<(), Self::Error> {
        let target = self
            .targets
            .iter()
            .find(|candidate| candidate.parent == target)
            .copied()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "taskbar target unavailable"))?;
        unsafe { attach_widget(self.state_pointer, window, target) }
    }

    fn detach(&mut self, window: Self::Window) -> Result<(), Self::Error> {
        unsafe { detach_widget(self.state_pointer, window) }
    }

    fn destroy(&mut self, window: Self::Window) -> Result<(), Self::Error> {
        unsafe { DestroyWindow(window).map_err(win_error) }
    }
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
    let target_parents = targets
        .iter()
        .map(|target| target.parent)
        .collect::<Vec<_>>();
    let attach_errors = {
        let mut backend = NativeWidgetSurfaceBackend {
            state_pointer,
            targets: targets.clone(),
        };
        reconcile_widget_surfaces(&mut backend, &target_parents)?
    };
    for error in attach_errors {
        log_taskbar_render_error("attach", &error);
    }

    reposition_widgets(&(*state_pointer).widgets, &targets);
    let snapshot = (*state_pointer).backend.snapshot();
    let rtl = matches!(
        (*state_pointer).settings.resolved_language,
        crate::Language::Arabic
    );
    for widget in &(*state_pointer).widgets {
        let _ = ShowWindow(widget.hwnd, SW_SHOWNA);
        if let Err(error) = paint_taskbar_widget(
            widget.hwnd,
            &snapshot,
            widget.hover.value(),
            rtl,
            widget_is_attached_to_taskbar(widget),
        ) {
            log_taskbar_render_error("compose", &error);
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
    for widget in widgets {
        if let Some(target) = targets
            .iter()
            .find(|target| target.parent == widget.taskbar_parent)
        {
            if let Err(error) = reposition_taskbar_widget(widget.hwnd, *target) {
                log_taskbar_render_error("position", &error);
            }
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

unsafe fn paint_taskbar_widget(
    hwnd: HWND,
    view: &WidgetViewModel,
    hover: u8,
    rtl: bool,
    attached_to_taskbar: bool,
) -> io::Result<()> {
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
    let palette = taskbar_palette(theme::system_uses_light_theme());
    let background = CreateSolidBrush(COLORREF(palette.material));
    FillRect(
        memory_dc,
        &RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        background,
    );
    let _ = DeleteObject(HGDIOBJ(background.0));
    let surface = widget_surface_layout(width, height, dpi, attached_to_taskbar);
    paint_compact_taskbar_content(
        memory_dc,
        native_rect(surface.content),
        dpi,
        view,
        palette,
        rtl,
    );
    if let (Some(header), Some(label)) =
        (surface.profile_header, profile_header_text(view, surface))
    {
        paint_profile_header(memory_dc, header, label, dpi, palette, rtl);
    }
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

const MATERIAL_HIT_TEST_ALPHA: u8 = 1;

fn material_surface_alpha(hover: u8, coverage: u8) -> u8 {
    if coverage == 0 {
        return 0;
    }

    // Layered-window pixels with zero alpha are excluded from hit testing. Keep the visually
    // transparent material at one alpha step so its complete rounded surface receives hover.
    let hover_alpha = ((u16::from(hover) * 28) / 255) as u8;
    ((u16::from(hover_alpha) * u16::from(coverage)) / 255).max(u16::from(MATERIAL_HIT_TEST_ALPHA))
        as u8
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
                material_surface_alpha(hover, coverage)
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

unsafe fn paint_compact_taskbar_content(
    dc: HDC,
    client: RECT,
    dpi: u32,
    view: &WidgetViewModel,
    palette: TaskbarPalette,
    rtl: bool,
) {
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let layout = TaskbarLayout::for_size(width, height, dpi);
    let positioned = |rect: Rect| {
        Rect::new(
            rect.left + client.left,
            rect.top + client.top,
            rect.right + client.left,
            rect.bottom + client.top,
        )
    };
    let row = select_weekly_row(view.primary.as_ref(), view.secondary.as_ref());
    let visual = taskbar_visual_state(view);
    let indicator_accent = taskbar_indicator_color(visual.indicator);
    let progress_accent = taskbar_risk_color(visual.progress_risk);

    let background = CreateSolidBrush(COLORREF(palette.material));
    FillRect(dc, &client, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let _ = SetBkMode(dc, TRANSPARENT);

    if visual.indicator == TaskbarIndicator::Error {
        if let Some(dot) = layout.dot {
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
            let mut dot = native_rect(positioned(dot));
            draw_text(
                dc,
                "!",
                &mut dot,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                indicator_accent,
            );
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    } else if let Some(dot) = layout.dot {
        let brush = CreateSolidBrush(indicator_accent);
        let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let dot = native_rect(positioned(dot));
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
        let mut label = native_rect(positioned(label));
        let alignment = if rtl {
            DT_RIGHT | DT_RTLREADING
        } else {
            DT_LEFT
        };
        draw_text(
            dc,
            &view.taskbar_label,
            &mut label,
            alignment | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
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
    let mut percent = native_rect(positioned(layout.percent));
    let percent_alignment = if layout.mode == TaskbarLayoutMode::Minimal {
        DT_CENTER
    } else {
        DT_RIGHT
    };
    let minimal_error =
        layout.mode == TaskbarLayoutMode::Minimal && visual.indicator == TaskbarIndicator::Error;
    let percent_text = compact_percent_text(
        layout.mode,
        visual.indicator,
        row.map(|row| row.percent_text.as_str()),
    );
    let percent_color = if minimal_error {
        indicator_accent
    } else {
        COLORREF(palette.percent)
    };
    draw_text(
        dc,
        percent_text,
        &mut percent,
        percent_alignment | DT_SINGLELINE | DT_VCENTER,
        percent_color,
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(percent_font.0));

    let track = CreateSolidBrush(COLORREF(palette.track));
    let progress = positioned(layout.progress);
    FillRect(dc, &native_rect(progress), track);
    let _ = DeleteObject(HGDIOBJ(track.0));
    if let Some(row) = row {
        let fill_width = progress_fill_width(layout.progress.width(), row.display_percent);
        if fill_width > 0 {
            let fill = CreateSolidBrush(progress_accent);
            FillRect(
                dc,
                &RECT {
                    right: progress.left + fill_width,
                    ..native_rect(progress)
                },
                fill,
            );
            let _ = DeleteObject(HGDIOBJ(fill.0));
        }
    }
}

unsafe fn paint_profile_header(
    dc: HDC,
    header: Rect,
    label: &str,
    dpi: u32,
    palette: TaskbarPalette,
    rtl: bool,
) {
    let header = native_rect(header);
    let background = CreateSolidBrush(COLORREF(palette.material));
    FillRect(dc, &header, background);
    let _ = DeleteObject(HGDIOBJ(background.0));
    let _ = SetBkMode(dc, TRANSPARENT);

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
    let old_font = SelectObject(dc, HGDIOBJ(font.0));
    let mut text_rect = header;
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    draw_text(
        dc,
        label,
        &mut text_rect,
        alignment | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        COLORREF(palette.label),
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(font.0));
}

fn compact_percent_text(
    mode: TaskbarLayoutMode,
    indicator: TaskbarIndicator,
    percent: Option<&str>,
) -> &str {
    if mode == TaskbarLayoutMode::Minimal && indicator == TaskbarIndicator::Error {
        "!"
    } else {
        percent.unwrap_or("--")
    }
}

const fn taskbar_indicator_color(indicator: TaskbarIndicator) -> COLORREF {
    match indicator {
        TaskbarIndicator::Comfortable => COLORREF(0x0074_c748),
        TaskbarIndicator::Normal => COLORREF(0x0023_a6f5),
        TaskbarIndicator::Fast | TaskbarIndicator::Error => COLORREF(0x005c_5cff),
        TaskbarIndicator::Neutral => COLORREF(0x0097_9797),
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

fn localized_window_title(language: crate::Language) -> Vec<u16> {
    crate::localized_text(crate::LocalizationKey::WindowTitle, language)
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

unsafe fn update_window_titles(state_pointer: *mut NativeState<'_>) {
    let title = localized_window_title((*state_pointer).settings.resolved_language);
    let title = PCWSTR(title.as_ptr());
    if (*state_pointer).owner != HWND::default() {
        let _ = SetWindowTextW((*state_pointer).owner, title);
    }
    for widget in &(*state_pointer).widgets {
        let _ = SetWindowTextW(widget.hwnd, title);
    }
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
    open_browser_url(url)
}

pub(super) unsafe fn open_validated_login_page(url: &str) -> io::Result<()> {
    if !super::super::is_valid_chatgpt_login_url(url) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe login URL",
        ));
    }
    open_browser_url(url)
}

unsafe fn open_browser_url(url: &str) -> io::Result<()> {
    let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    run_with_shell_com(|| {
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
    })
}

/// 셸 URL 실행 전에 호출 스레드의 COM 아파트를 준비합니다.
///
/// 로그인 URL은 프로필 폴링 워커에서 실행되므로 UI 스레드의 COM 초기화 상태를 기대할 수
/// 없습니다. 이미 다른 모델로 초기화된 스레드는 `RPC_E_CHANGED_MODE`를 허용하고, 이 함수가
/// 직접 초기화한 경우에만 반환 시 `CoUninitialize`를 호출합니다.
fn run_with_shell_com<F, T>(operation: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T>,
{
    let _apartment = ShellComApartment::initialize()?;
    operation()
}

struct ShellComApartment {
    uninitialize: bool,
}

impl ShellComApartment {
    fn initialize() -> io::Result<Self> {
        // SAFETY: COM initialization is scoped to the current caller thread and balanced by Drop
        // when this instance owns the initialization reference.
        let initialized =
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        if initialized.is_ok() {
            Ok(Self { uninitialize: true })
        } else if initialized == RPC_E_CHANGED_MODE {
            Ok(Self {
                uninitialize: false,
            })
        } else {
            Err(io::Error::other("Windows shell initialization failed"))
        }
    }
}

impl Drop for ShellComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: this instance only sets `uninitialize` after a successful matching init.
            unsafe { CoUninitialize() };
        }
    }
}

pub(super) unsafe fn show_diagnostic_summary(title: &str, message: &str) -> io::Result<()> {
    let mut presenter = WindowsNativeMessagePresenter;
    show_diagnostic_summary_with_presenter(title, message, &mut presenter)
}

/// 진단 요약을 프로필 라우팅과 분리된 일반 애플리케이션 메시지 경계로 전달합니다.
fn show_diagnostic_summary_with_presenter<P: NativeMessagePresenter>(
    title: &str,
    message: &str,
    presenter: &mut P,
) -> io::Result<()> {
    presenter
        .present_application(HWND::default(), message, title, MB_OK | MB_ICONINFORMATION)
        .map(|_| ())
}

unsafe fn show_pending_update_notice_if_ready(state_pointer: *mut NativeState<'_>) {
    let Some(_guard) = UpdateDialogGuard::acquire() else {
        return;
    };
    let Some(notice) = (*state_pointer).backend.take_update_notice() else {
        return;
    };
    let mut presenter = WindowsNativeMessagePresenter;
    let _ = show_update_notice_with_presenter(
        (*state_pointer).owner,
        &notice,
        (*state_pointer).settings.resolved_language,
        &mut presenter,
        |url| open_validated_tag_page(url),
    );
}

/// 사용자 업데이트 결과를 소유 창에 표시하고 명시적 확인 뒤에만 릴리스 페이지를 엽니다.
///
/// `notice`는 이미 제한된 GitHub 릴리스 응답에서 검증된 값이며, `open_release`는 검증된 URL만
/// 처리해야 합니다. 네트워크 작업은 수행하지 않고 대화상자 결과와 브라우저 열기 오류만 처리합니다.
fn show_update_notice_with_presenter<P, F>(
    owner: HWND,
    notice: &UpdateCheckNotice,
    language: Language,
    presenter: &mut P,
    mut open_release: F,
) -> io::Result<()>
where
    P: NativeMessagePresenter,
    F: FnMut(&str) -> io::Result<()>,
{
    let (message, confirm_open, warning, release_url) = match notice {
        UpdateCheckNotice::Current => (
            localized_text(LocalizationKey::UpdateCurrent, language).to_owned(),
            false,
            false,
            None,
        ),
        UpdateCheckNotice::Available(update) => (
            localized_text(LocalizationKey::UpdateAvailablePrompt, language)
                .replace("{version}", &update.version.to_string()),
            true,
            false,
            Some(update.release_url.as_str()),
        ),
        UpdateCheckNotice::Failed => (
            localized_text(LocalizationKey::UpdateFailedHelp, language).to_owned(),
            false,
            true,
            None,
        ),
    };
    let buttons = if confirm_open { MB_YESNO } else { MB_OK };
    let icon = if warning {
        MB_ICONWARNING
    } else {
        MB_ICONINFORMATION
    };
    let style = update_dialog_style(buttons, icon);
    let result = presenter.present_application(
        owner,
        &message,
        localized_text(LocalizationKey::WindowTitle, language),
        style,
    )?;
    let Some(release_url) = release_url else {
        return Ok(());
    };
    if !confirm_open || !update_dialog_opens_release(result) {
        return Ok(());
    }
    if open_release(release_url).is_err() {
        let _ = presenter.present_application(
            owner,
            localized_text(LocalizationKey::UpdateOpenFailed, language),
            localized_text(LocalizationKey::WindowTitle, language),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND | MB_TASKMODAL,
        );
    }
    Ok(())
}

fn update_dialog_opens_release(result: MESSAGEBOX_RESULT) -> bool {
    result == IDYES
}

fn update_dialog_style(buttons: MESSAGEBOX_STYLE, icon: MESSAGEBOX_STYLE) -> MESSAGEBOX_STYLE {
    buttons | icon | MB_SETFOREGROUND | MB_TASKMODAL
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{
        compact_percent_text, confirm_usage_forecast_clear_with_presenter, glass_noise,
        material_surface_alpha, rounded_material_alpha, run_with_shell_com, should_open_tray_menu,
        should_open_widget_menu, show_diagnostic_summary_with_presenter,
        show_profile_dialog_error_with_presenter, show_update_notice_with_presenter,
        taskbar_indicator_color, taskbar_palette, update_dialog_in_progress,
        update_dialog_opens_release, update_dialog_style, NativeMessagePresenter, TaskbarIndicator,
        TaskbarLayoutMode, TaskbarRefreshSchedule, UpdateDialogGuard, NIN_SELECT, WM_CONTEXTMENU,
    };
    use crate::{
        windows::profile_dialog::ProfileMessageRoute, AvailableUpdate, Language, UpdateCheckNotice,
    };
    use windows::Win32::{
        Foundation::{COLORREF, HWND},
        UI::WindowsAndMessaging::{
            IDNO, IDOK, IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
            MB_TASKMODAL, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, WM_LBUTTONUP,
            WM_RBUTTONUP,
        },
    };

    #[derive(Debug, PartialEq, Eq)]
    enum PresentedNativeMessage {
        Profile(ProfileMessageRoute),
        Application {
            owner: HWND,
            message: String,
            title: String,
            style: MESSAGEBOX_STYLE,
        },
    }

    struct RecordingNativeMessagePresenter {
        messages: Vec<PresentedNativeMessage>,
        response: MESSAGEBOX_RESULT,
    }

    impl NativeMessagePresenter for RecordingNativeMessagePresenter {
        fn present_profile(
            &mut self,
            route: ProfileMessageRoute,
            _owner: HWND,
            _message: &str,
            _title: &str,
            _style: MESSAGEBOX_STYLE,
        ) -> io::Result<MESSAGEBOX_RESULT> {
            self.messages.push(PresentedNativeMessage::Profile(route));
            Ok(IDOK)
        }

        fn present_application(
            &mut self,
            owner: HWND,
            message: &str,
            title: &str,
            style: MESSAGEBOX_STYLE,
        ) -> io::Result<MESSAGEBOX_RESULT> {
            self.messages.push(PresentedNativeMessage::Application {
                owner,
                message: message.to_owned(),
                title: title.to_owned(),
                style,
            });
            Ok(self.response)
        }
    }

    #[test]
    fn native_profile_operation_error_uses_the_profile_message_boundary() {
        let mut presenter = RecordingNativeMessagePresenter {
            messages: Vec::new(),
            response: IDOK,
        };

        show_profile_dialog_error_with_presenter(
            HWND(201_usize as _),
            Language::English,
            &mut presenter,
        );

        assert_eq!(
            presenter.messages,
            vec![PresentedNativeMessage::Profile(
                ProfileMessageRoute::NativeOperationError
            )]
        );
    }

    #[test]
    fn usage_forecast_clear_confirmation_uses_the_owner_and_localized_warning() {
        let mut presenter = RecordingNativeMessagePresenter {
            messages: Vec::new(),
            response: IDYES,
        };

        assert!(confirm_usage_forecast_clear_with_presenter(
            HWND(202_usize as _),
            Language::English,
            &mut presenter,
        )
        .unwrap());

        let [PresentedNativeMessage::Application {
            owner,
            message,
            style,
            ..
        }] = presenter.messages.as_slice()
        else {
            panic!("expected one confirmation message");
        };
        assert_eq!(*owner, HWND(202_usize as _));
        assert_eq!(message, "Clear the usage forecast history?");
        assert_eq!(
            *style,
            MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND | MB_TASKMODAL
        );
    }

    #[test]
    fn diagnostic_summary_stays_outside_the_profile_message_boundary() {
        let mut presenter = RecordingNativeMessagePresenter {
            messages: Vec::new(),
            response: IDOK,
        };

        show_diagnostic_summary_with_presenter("Diagnostics", "Ready", &mut presenter).unwrap();

        assert_eq!(
            presenter.messages,
            vec![PresentedNativeMessage::Application {
                owner: HWND::default(),
                message: "Ready".to_owned(),
                title: "Diagnostics".to_owned(),
                style: MB_OK | MB_ICONINFORMATION,
            }]
        );
    }

    #[test]
    fn update_notice_uses_the_owner_and_requires_explicit_confirmation() {
        let owner = HWND(201_usize as _);
        let update = UpdateCheckNotice::Available(AvailableUpdate {
            version: semver::Version::parse("2.0.0").unwrap(),
            release_url: "https://github.com/owner/repo/releases/tag/v2.0.0".to_owned(),
        });
        let mut presenter = RecordingNativeMessagePresenter {
            messages: Vec::new(),
            response: IDNO,
        };
        let mut opened = None;
        show_update_notice_with_presenter(
            owner,
            &update,
            Language::English,
            &mut presenter,
            |url| {
                opened = Some(url.to_owned());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(opened, None);
        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(presenter.messages[0].owner(), owner);

        presenter.messages.clear();
        presenter.response = IDYES;
        show_update_notice_with_presenter(
            owner,
            &update,
            Language::English,
            &mut presenter,
            |url| {
                opened = Some(url.to_owned());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            opened,
            Some("https://github.com/owner/repo/releases/tag/v2.0.0".to_owned())
        );
    }

    #[test]
    fn update_notice_current_and_failed_results_use_localized_information() {
        for (notice, warning) in [
            (UpdateCheckNotice::Current, false),
            (UpdateCheckNotice::Failed, true),
        ] {
            let mut presenter = RecordingNativeMessagePresenter {
                messages: Vec::new(),
                response: IDOK,
            };
            show_update_notice_with_presenter(
                HWND(201_usize as _),
                &notice,
                Language::English,
                &mut presenter,
                |_| Ok(()),
            )
            .unwrap();

            assert_eq!(presenter.messages.len(), 1);
            let PresentedNativeMessage::Application { message, style, .. } = &presenter.messages[0]
            else {
                panic!("update result must use the application message boundary");
            };
            assert!(!message.is_empty());
            assert_eq!(style.0 & MB_YESNO.0, 0);
            if warning {
                assert_eq!(
                    style.0 & windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING.0,
                    windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING.0
                );
            }
        }
    }

    #[test]
    fn browser_shell_operation_runs_inside_a_com_apartment() {
        assert!(run_with_shell_com(|| Ok::<_, io::Error>(true)).unwrap());
    }

    impl PresentedNativeMessage {
        fn owner(&self) -> HWND {
            match self {
                Self::Profile(_) => HWND::default(),
                Self::Application { owner, .. } => *owner,
            }
        }
    }

    #[test]
    fn tray_menu_uses_only_version_4_activation_events() {
        assert!(should_open_tray_menu(WM_CONTEXTMENU));
        assert!(should_open_tray_menu(NIN_SELECT));
        assert!(!should_open_tray_menu(WM_RBUTTONUP));
        assert!(!should_open_tray_menu(WM_LBUTTONUP));
    }

    #[test]
    fn widget_menu_uses_the_standard_context_menu_message() {
        assert!(should_open_widget_menu(WM_CONTEXTMENU));
        assert!(!should_open_widget_menu(NIN_SELECT));
    }

    #[test]
    fn update_dialog_opens_release_only_for_an_explicit_yes() {
        assert!(update_dialog_opens_release(IDYES));
        assert!(!update_dialog_opens_release(IDNO));
    }

    #[test]
    fn update_dialog_style_is_task_modal_for_an_ownerless_dialog() {
        let style = update_dialog_style(MB_YESNO, MB_ICONINFORMATION);

        assert_eq!(style.0 & MB_TASKMODAL.0, MB_TASKMODAL.0);
    }

    #[test]
    fn update_dialog_guard_is_scoped_and_rejects_reentry() {
        assert!(!update_dialog_in_progress());

        let guard = UpdateDialogGuard::acquire().expect("first update dialog may enter");
        assert!(update_dialog_in_progress());
        assert!(UpdateDialogGuard::acquire().is_none());

        drop(guard);
        assert!(!update_dialog_in_progress());
    }

    #[test]
    fn taskbar_refresh_schedule_retries_startup_then_uses_the_safety_interval() {
        let mut schedule = TaskbarRefreshSchedule::new();
        for _ in 0..5 {
            assert!(schedule.tick());
        }
        for _ in 0..29 {
            assert!(!schedule.tick());
        }
        assert!(schedule.tick());
    }

    #[test]
    fn minimal_layout_keeps_error_state_visible() {
        assert_eq!(
            compact_percent_text(
                TaskbarLayoutMode::Minimal,
                TaskbarIndicator::Error,
                Some("42%")
            ),
            "!"
        );
        assert_eq!(
            compact_percent_text(
                TaskbarLayoutMode::Full,
                TaskbarIndicator::Error,
                Some("42%")
            ),
            "42%"
        );
    }

    #[test]
    fn taskbar_indicator_colors_are_stable() {
        assert_eq!(
            taskbar_indicator_color(TaskbarIndicator::Comfortable),
            COLORREF(0x0074_c748)
        );
        assert_eq!(
            taskbar_indicator_color(TaskbarIndicator::Normal),
            COLORREF(0x0023_a6f5)
        );
        assert_eq!(
            taskbar_indicator_color(TaskbarIndicator::Fast),
            COLORREF(0x005c_5cff)
        );
        assert_eq!(
            taskbar_indicator_color(TaskbarIndicator::Neutral),
            COLORREF(0x0097_9797)
        );
        assert_eq!(
            taskbar_indicator_color(TaskbarIndicator::Error),
            COLORREF(0x005c_5cff)
        );
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
    fn transparent_material_keeps_the_whole_widget_hit_testable() {
        assert_eq!(material_surface_alpha(0, 0), 0);
        assert_eq!(material_surface_alpha(0, 1), 1);
        assert_eq!(material_surface_alpha(0, 255), 1);
        assert_eq!(material_surface_alpha(255, 255), 28);
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
