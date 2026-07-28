use std::{ffi::c_void, io};

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::EM_SETLIMITTEXT,
            Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetFocus},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem,
                GetMessageW, GetWindowLongPtrW, GetWindowTextW, IsDialogMessageW, IsWindow,
                LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW, SendMessageW,
                SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
                TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
                GWLP_USERDATA, HMENU, IDCANCEL, IDC_ARROW, IDOK, IDYES, LBN_SELCHANGE, LBS_NOTIFY,
                LB_ADDSTRING, LB_GETCURSEL, LB_SETCURSEL, MB_ICONERROR, MB_ICONWARNING, MB_OK,
                MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
                WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
                WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
};

use crate::{localized_text, Language, LocalizationKey};

use super::{
    profile_delete_confirmation, profile_dialog_keyboard_result, profile_login_confirmation,
    ModalCleanupAction, ModalDialogLifecycle, ProfileDialogAction, ProfileDialogCommand,
    ProfileDialogController, ProfileDialogKeyboardCommand, ProfileDialogKeyboardResult,
    UsageProfileView, PROFILE_LABEL_MAX_UTF16_UNITS,
};

const DIALOG_CLASS: PCWSTR = w!("CodexUsageMonitor.ProfileDialog.v1");
const PROFILE_LIST_ID: i32 = 4100;
const PROFILE_NAME_ID: i32 = 4101;
const ADD_ID: i32 = 4102;
const RENAME_ID: i32 = 4103;
const LOGIN_ID: i32 = 4104;
const LOGOUT_ID: i32 = 4105;
const DELETE_ID: i32 = 4106;
const CLOSE_ID: i32 = 4107;

struct DialogState {
    controller: ProfileDialogController,
    language: Language,
    result: Option<ProfileDialogAction>,
    list: HWND,
    edit: HWND,
}

struct ModalWindowGuard {
    dialog: HWND,
    owner: HWND,
    lifecycle: ModalDialogLifecycle,
}

impl ModalWindowGuard {
    unsafe fn new(dialog: HWND, owner: HWND) -> Self {
        let owner_present = owner != HWND::default() && IsWindow(Some(owner)).as_bool();
        let owner_was_enabled = owner_present && IsWindowEnabled(owner).as_bool();
        let mut lifecycle = ModalDialogLifecycle::new(owner_present, owner_was_enabled);
        lifecycle.window_created();
        Self {
            dialog,
            owner,
            lifecycle,
        }
    }

    unsafe fn disable_owner(&mut self) {
        if self.lifecycle.should_disable_owner() {
            let _ = EnableWindow(self.owner, false);
            self.lifecycle.owner_disabled();
        }
    }
}

impl Drop for ModalWindowGuard {
    fn drop(&mut self) {
        unsafe {
            if !IsWindow(Some(self.dialog)).as_bool() {
                self.lifecycle.window_destroyed();
            }
            for action in self.lifecycle.cleanup_actions() {
                match action {
                    ModalCleanupAction::ClearWindowState => {
                        SetWindowLongPtrW(self.dialog, GWLP_USERDATA, 0);
                    }
                    ModalCleanupAction::DestroyWindow => {
                        let _ = DestroyWindow(self.dialog);
                    }
                    ModalCleanupAction::RestoreOwner => {
                        let _ = EnableWindow(self.owner, true);
                        let _ = SetForegroundWindow(self.owner);
                    }
                }
            }
        }
    }
}

pub(super) fn show_profile_manager(
    profiles: &[UsageProfileView],
    mutation_pending: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>> {
    unsafe { show_profile_manager_owned(HWND::default(), profiles, mutation_pending, language) }
}

pub(super) unsafe fn show_profile_manager_owned(
    owner: HWND,
    profiles: &[UsageProfileView],
    mutation_pending: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>> {
    let module = GetModuleHandleW(None).map_err(win_error)?;
    let instance = HINSTANCE(module.0);
    register_dialog_class(instance)?;

    let mut state = Box::new(DialogState {
        controller: ProfileDialogController::new(profiles, mutation_pending),
        language,
        result: None,
        list: HWND::default(),
        edit: HWND::default(),
    });
    let state_pointer = (&mut *state as *mut DialogState).cast::<c_void>();
    let title = wide(localized_text(
        LocalizationKey::MenuManageUsageProfiles,
        language,
    ));
    let parent = (owner != HWND::default()).then_some(owner);
    let dialog = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        DIALOG_CLASS,
        PCWSTR(title.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        650,
        410,
        parent,
        None,
        Some(instance),
        Some(state_pointer.cast_const()),
    )
    .map_err(win_error)?;
    let mut window_guard = ModalWindowGuard::new(dialog, owner);

    setup_controls(dialog, instance, profiles, &mut state)?;

    window_guard.disable_owner();
    let _ = ShowWindow(dialog, SW_SHOW);
    let _ = SetForegroundWindow(dialog);
    let _ = SetFocus(Some(state.edit));

    let mut quit_code = None;
    while IsWindow(Some(dialog)).as_bool() {
        let mut message = MSG::default();
        let status = GetMessageW(&mut message, None, 0, 0);
        if status.0 == -1 {
            return Err(io::Error::last_os_error());
        }
        if status.0 == 0 {
            quit_code = Some(message.wParam.0 as i32);
            break;
        }
        if !IsDialogMessageW(dialog, &message).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    if let Some(code) = quit_code {
        PostQuitMessage(code);
    }
    Ok(state.result.take())
}

pub(super) fn confirm_profile_login(language: Language) -> io::Result<bool> {
    unsafe {
        confirm_profile_login_owned(
            HWND::default(),
            localized_text(LocalizationKey::UsageProfileDisplayed, language),
            language,
        )
    }
}

pub(super) fn confirm_profile_delete(label: &str, language: Language) -> io::Result<bool> {
    unsafe { confirm_profile_delete_owned(HWND::default(), label, language) }
}

unsafe fn register_dialog_class(instance: HINSTANCE) -> io::Result<()> {
    let cursor = LoadCursorW(None, IDC_ARROW).map_err(win_error)?;
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(dialog_proc),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: DIALOG_CLASS,
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

unsafe fn setup_controls(
    dialog: HWND,
    instance: HINSTANCE,
    profiles: &[UsageProfileView],
    state: &mut DialogState,
) -> io::Result<()> {
    state.list = create_control(
        dialog,
        instance,
        w!("LISTBOX"),
        "",
        PROFILE_LIST_ID,
        16,
        16,
        602,
        190,
        WS_CHILD
            | WS_VISIBLE
            | WS_BORDER
            | WS_VSCROLL
            | WS_TABSTOP
            | WINDOW_STYLE(LBS_NOTIFY as u32),
    )?;

    let name_label = localized_text(LocalizationKey::UsageProfileName, state.language);
    let _ = create_control(
        dialog,
        instance,
        w!("STATIC"),
        name_label,
        0,
        16,
        220,
        120,
        22,
        WS_CHILD | WS_VISIBLE,
    )?;
    state.edit = create_control(
        dialog,
        instance,
        w!("EDIT"),
        "",
        PROFILE_NAME_ID,
        144,
        216,
        474,
        26,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
    )?;
    let _ = SendMessageW(
        state.edit,
        EM_SETLIMITTEXT,
        Some(WPARAM(PROFILE_LABEL_MAX_UTF16_UNITS)),
        None,
    );

    for (id, key, x, width) in [
        (ADD_ID, LocalizationKey::MenuAddUsageProfile, 16, 92),
        (RENAME_ID, LocalizationKey::UsageProfileRename, 116, 92),
        (LOGIN_ID, LocalizationKey::UsageProfileLogin, 216, 92),
        (LOGOUT_ID, LocalizationKey::UsageProfileLogout, 316, 92),
        (DELETE_ID, LocalizationKey::UsageProfileDelete, 416, 92),
        (CLOSE_ID, LocalizationKey::UsageProfileClose, 526, 92),
    ] {
        let _ = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            localized_text(key, state.language),
            id,
            x,
            270,
            width,
            30,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        )?;
    }

    for profile in profiles {
        let line = if profile.summary.trim().is_empty() {
            profile.label.clone()
        } else {
            format!("{} — {}", profile.label, profile.summary)
        };
        let line = wide(&line);
        let result = SendMessageW(
            state.list,
            LB_ADDSTRING,
            None,
            Some(LPARAM(line.as_ptr() as isize)),
        );
        if result.0 < 0 {
            return Err(io::Error::other("profile list item could not be displayed"));
        }
    }
    if let Some(index) = profiles
        .iter()
        .position(|profile| profile.selected)
        .or_else(|| (!profiles.is_empty()).then_some(0))
    {
        let _ = SendMessageW(state.list, LB_SETCURSEL, Some(WPARAM(index)), None);
    }
    update_controls(dialog, state);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_control(
    parent: HWND,
    instance: HINSTANCE,
    class: PCWSTR,
    text: &str,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    style: WINDOW_STYLE,
) -> io::Result<HWND> {
    let text = wide(text);
    CreateWindowExW(
        Default::default(),
        class,
        PCWSTR(text.as_ptr()),
        style,
        x,
        y,
        width,
        height,
        Some(parent),
        (id != 0).then_some(HMENU(id as usize as *mut c_void)),
        Some(instance),
        None,
    )
    .map_err(win_error)
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if state.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_COMMAND => {
            handle_command(hwnd, &mut *state, wparam);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn handle_command(hwnd: HWND, state: &mut DialogState, wparam: WPARAM) {
    let control_id = (wparam.0 & 0xffff) as i32;
    let notification = ((wparam.0 >> 16) & 0xffff) as u32;
    let keyboard_command = if control_id == IDCANCEL.0 {
        Some(ProfileDialogKeyboardCommand::Cancel)
    } else if control_id == IDOK.0 {
        Some(ProfileDialogKeyboardCommand::Accept)
    } else {
        None
    };
    if let Some(command) = keyboard_command {
        if profile_dialog_keyboard_result(command)
            == ProfileDialogKeyboardResult::CloseWithoutAction
        {
            let _ = DestroyWindow(hwnd);
        }
        return;
    }
    if control_id == PROFILE_LIST_ID && notification == LBN_SELCHANGE {
        let selected = SendMessageW(state.list, LB_GETCURSEL, None, None).0;
        if selected >= 0 && state.controller.select(selected as usize) {
            update_controls(hwnd, state);
        }
        return;
    }

    let action = match control_id {
        ADD_ID => submit_label(hwnd, state, true),
        RENAME_ID => submit_label(hwnd, state, false),
        LOGIN_ID => Ok(state
            .controller
            .confirmed_command(ProfileDialogCommand::Login, true)),
        LOGOUT_ID => Ok(state
            .controller
            .confirmed_command(ProfileDialogCommand::Logout, true)),
        DELETE_ID => {
            let Some(profile) = state.controller.selected_profile() else {
                return;
            };
            match confirm_profile_delete_owned(hwnd, &profile.label, state.language) {
                Ok(confirmed) => Ok(state
                    .controller
                    .confirmed_command(ProfileDialogCommand::Delete, confirmed)),
                Err(error) => Err(error),
            }
        }
        CLOSE_ID => {
            let _ = DestroyWindow(hwnd);
            return;
        }
        _ => return,
    };

    match action {
        Ok(Some(action)) => {
            state.result = Some(action);
            let _ = DestroyWindow(hwnd);
        }
        Ok(None) => {}
        Err(_) => show_safe_error(hwnd, state.language),
    }
}

unsafe fn submit_label(
    hwnd: HWND,
    state: &DialogState,
    add: bool,
) -> Result<Option<ProfileDialogAction>, io::Error> {
    let mut buffer = [0_u16; PROFILE_LABEL_MAX_UTF16_UNITS + 1];
    let length = GetWindowTextW(state.edit, &mut buffer);
    if length < 0 {
        return Err(io::Error::last_os_error());
    }
    let value = String::from_utf16(&buffer[..length as usize])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid profile label"))?;
    let action = if add {
        state.controller.submit_add(&value)
    } else {
        state.controller.submit_rename(&value)
    };
    match action {
        Ok(action) => Ok(action),
        Err(_) => {
            show_message(
                hwnd,
                localized_text(LocalizationKey::UsageProfileInvalidLabel, state.language),
                localized_text(LocalizationKey::WindowTitle, state.language),
                MB_OK | MB_ICONWARNING,
            )?;
            Ok(None)
        }
    }
}

unsafe fn update_controls(hwnd: HWND, state: &DialogState) {
    set_enabled(hwnd, ADD_ID, state.controller.can_add());
    set_enabled(
        hwnd,
        RENAME_ID,
        state
            .controller
            .command_enabled(ProfileDialogCommand::Rename),
    );
    set_enabled(
        hwnd,
        LOGIN_ID,
        state
            .controller
            .command_enabled(ProfileDialogCommand::Login),
    );
    set_enabled(
        hwnd,
        LOGOUT_ID,
        state
            .controller
            .command_enabled(ProfileDialogCommand::Logout),
    );
    set_enabled(
        hwnd,
        DELETE_ID,
        state
            .controller
            .command_enabled(ProfileDialogCommand::Delete),
    );
    if let Some(profile) = state.controller.selected_profile() {
        let label = wide(&profile.label);
        let _ = SetWindowTextW(state.edit, PCWSTR(label.as_ptr()));
    }
}

unsafe fn set_enabled(hwnd: HWND, id: i32, enabled: bool) {
    if let Ok(control) = GetDlgItem(Some(hwnd), id) {
        let _ = EnableWindow(control, enabled);
    }
}

pub(super) unsafe fn confirm_profile_login_owned(
    owner: HWND,
    label: &str,
    language: Language,
) -> io::Result<bool> {
    let message = profile_login_confirmation(label, language);
    let title = localized_text(LocalizationKey::UsageProfileLogin, language);
    show_message(owner, &message, title, MB_OKCANCEL | MB_ICONWARNING).map(|result| result == IDOK)
}

unsafe fn confirm_profile_delete_owned(
    owner: HWND,
    label: &str,
    language: Language,
) -> io::Result<bool> {
    let message = profile_delete_confirmation(label, language);
    let title = localized_text(LocalizationKey::UsageProfileDelete, language);
    show_message(owner, &message, title, MB_YESNO | MB_ICONWARNING).map(|result| result == IDYES)
}

unsafe fn show_safe_error(owner: HWND, language: Language) {
    let _ = show_message(
        owner,
        localized_text(LocalizationKey::UsageProfileOperationFailed, language),
        localized_text(LocalizationKey::WindowTitle, language),
        MB_OK | MB_ICONERROR,
    );
}

unsafe fn show_message(
    owner: HWND,
    message: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> io::Result<windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT> {
    let message = wide(message);
    let title = wide(title);
    let parent = (owner != HWND::default()).then_some(owner);
    let result = MessageBoxW(
        parent,
        PCWSTR(message.as_ptr()),
        PCWSTR(title.as_ptr()),
        style,
    );
    if result.0 == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn win_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}
