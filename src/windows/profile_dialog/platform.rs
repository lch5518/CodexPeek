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
                TranslateMessage, BS_DEFPUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
                CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDCANCEL, IDC_ARROW, IDOK, IDYES,
                LBN_SELCHANGE, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL, LB_SETCURSEL, MB_ICONERROR,
                MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WINDOW_STYLE, WM_CLOSE,
                WM_COMMAND, WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WNDCLASSW, WS_BORDER,
                WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
                WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
};

use crate::{localized_text, Language, LocalizationKey};

use super::{
    add_profile_prompt_result, profile_delete_confirmation, profile_dialog_keyboard_result,
    profile_login_confirmation, profile_manager_row_label, AddProfilePromptCommand,
    ModalCleanupAction, ModalDialogLifecycle, ProfileDialogAction, ProfileDialogCommand,
    ProfileDialogController, ProfileDialogKeyboardCommand, ProfileDialogKeyboardResult,
    ProfileManagerControl, UsageProfileView, PROFILE_LABEL_MAX_UTF16_UNITS,
    PROFILE_MANAGER_CONTROLS,
};

const DIALOG_CLASS: PCWSTR = w!("CodexUsageMonitor.ProfileDialog.v1");
const ADD_DIALOG_CLASS: PCWSTR = w!("CodexUsageMonitor.AddProfileDialog.v1");
const PROFILE_LIST_ID: i32 = 4100;
const PROFILE_NAME_ID: i32 = 4101;
const OPEN_ADD_ID: i32 = 4102;
const RENAME_ID: i32 = 4103;
const LOGIN_ID: i32 = 4104;
const LOGOUT_ID: i32 = 4105;
const DELETE_ID: i32 = 4106;
const ADD_PROFILE_NAME_ID: i32 = 4200;

struct DialogState {
    controller: ProfileDialogController,
    language: Language,
    result: Option<ProfileDialogAction>,
    list: HWND,
    edit: HWND,
}

struct AddDialogState {
    edit: HWND,
    language: Language,
    result: Option<ProfileDialogAction>,
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

    run_modal_message_loop(dialog)?;
    Ok(state.result.take())
}

/// 활성 프로필 관리자를 소유자로 사용해 프로필 이름 추가 입력창을 표시합니다.
///
/// `can_add`가 거짓이면 창을 만들지 않고 `None`을 반환합니다. 호출자는 유효한 관리자 HWND를
/// 전달해야 하며, 반환 작업은 검증된 UI 의도일 뿐 파일·설정·로그인 I/O를 수행하지 않습니다.
pub(super) unsafe fn show_add_profile_prompt_owned(
    owner: HWND,
    can_add: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>> {
    if !can_add {
        return Ok(None);
    }
    if owner == HWND::default() || !IsWindow(Some(owner)).as_bool() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "add-profile prompt requires a live owner",
        ));
    }

    let module = GetModuleHandleW(None).map_err(win_error)?;
    let instance = HINSTANCE(module.0);
    register_add_dialog_class(instance)?;

    let mut state = Box::new(AddDialogState {
        edit: HWND::default(),
        language,
        result: None,
    });
    let state_pointer = (&mut *state as *mut AddDialogState).cast::<c_void>();
    let title = wide(localized_text(
        LocalizationKey::MenuAddUsageProfile,
        language,
    ));
    let dialog = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        ADD_DIALOG_CLASS,
        PCWSTR(title.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        560,
        180,
        Some(owner),
        None,
        Some(instance),
        Some(state_pointer.cast_const()),
    )
    .map_err(win_error)?;
    let mut window_guard = ModalWindowGuard::new(dialog, owner);

    setup_add_dialog_controls(dialog, instance, &mut state)?;

    window_guard.disable_owner();
    let _ = ShowWindow(dialog, SW_SHOW);
    let _ = SetForegroundWindow(dialog);
    let _ = SetFocus(Some(state.edit));

    run_modal_message_loop(dialog)?;
    Ok(state.result.take())
}

/// 지정 창이 살아 있는 동안 표준 모달 키보드 변환을 포함한 메시지 루프를 실행합니다.
///
/// 스레드 종료 메시지를 소비하면 바깥 메시지 루프가 같은 종료 코드를 받을 수 있도록 다시
/// 게시합니다. 창 수명과 소유자 복원은 호출자가 보유한 `ModalWindowGuard`가 담당합니다.
unsafe fn run_modal_message_loop(dialog: HWND) -> io::Result<()> {
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
    Ok(())
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

/// 추가 입력창의 Win32 클래스를 현재 모듈에 한 번 등록합니다.
///
/// 같은 클래스가 이미 등록된 경우는 정상으로 처리하며, 그 밖의 등록 실패는 운영체제 오류로
/// 반환합니다.
unsafe fn register_add_dialog_class(instance: HINSTANCE) -> io::Result<()> {
    let cursor = LoadCursorW(None, IDC_ARROW).map_err(win_error)?;
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(add_dialog_proc),
        hInstance: instance,
        hCursor: cursor,
        lpszClassName: ADD_DIALOG_CLASS,
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
        64,
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
        188,
        216,
        430,
        26,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
    )?;
    let _ = SendMessageW(
        state.edit,
        EM_SETLIMITTEXT,
        Some(WPARAM(PROFILE_LABEL_MAX_UTF16_UNITS)),
        None,
    );

    for control in PROFILE_MANAGER_CONTROLS {
        let (id, text, x, y, width, height) = manager_control_spec(control, state.language);
        let _ = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            text,
            id,
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        )?;
    }

    for profile in profiles {
        let label = profile_manager_row_label(profile, state.language);
        let line = if profile.summary.trim().is_empty() {
            label
        } else {
            format!("{label} — {}", profile.summary)
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

/// 공유 관리자 컨트롤 계약을 Win32 ID, 문구와 고정 배치로 변환합니다.
///
/// 추가 버튼은 목록 바로 아래의 작은 `+`로 배치되고 나머지 네 작업만 하단 행에 배치됩니다.
fn manager_control_spec(
    control: ProfileManagerControl,
    language: Language,
) -> (i32, &'static str, i32, i32, i32, i32) {
    match control {
        ProfileManagerControl::AddBelowList => (OPEN_ADD_ID, "+", 16, 216, 36, 26),
        ProfileManagerControl::Rename => (
            RENAME_ID,
            localized_text(LocalizationKey::UsageProfileRename, language),
            116,
            270,
            92,
            30,
        ),
        ProfileManagerControl::Login => (
            LOGIN_ID,
            localized_text(LocalizationKey::UsageProfileLogin, language),
            216,
            270,
            92,
            30,
        ),
        ProfileManagerControl::Logout => (
            LOGOUT_ID,
            localized_text(LocalizationKey::UsageProfileLogout, language),
            316,
            270,
            92,
            30,
        ),
        ProfileManagerControl::Delete => (
            DELETE_ID,
            localized_text(LocalizationKey::UsageProfileDelete, language),
            416,
            270,
            92,
            30,
        ),
    }
}

/// 추가 입력창의 이름 필드와 명시적인 추가·취소 버튼을 생성합니다.
///
/// 일부 컨트롤 생성이 실패하면 오류를 반환하며, 호출자의 모달 가드가 이미 생성된 자식과 창을
/// 함께 정리합니다.
unsafe fn setup_add_dialog_controls(
    dialog: HWND,
    instance: HINSTANCE,
    state: &mut AddDialogState,
) -> io::Result<()> {
    let _ = create_control(
        dialog,
        instance,
        w!("STATIC"),
        localized_text(LocalizationKey::UsageProfileName, state.language),
        0,
        16,
        20,
        112,
        22,
        WS_CHILD | WS_VISIBLE,
    )?;
    state.edit = create_control(
        dialog,
        instance,
        w!("EDIT"),
        "",
        ADD_PROFILE_NAME_ID,
        132,
        16,
        396,
        26,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
    )?;
    let _ = SendMessageW(
        state.edit,
        EM_SETLIMITTEXT,
        Some(WPARAM(PROFILE_LABEL_MAX_UTF16_UNITS)),
        None,
    );

    for (id, key, x, width, extra_style) in [
        (
            IDOK.0,
            LocalizationKey::MenuAddUsageProfile,
            188,
            220,
            WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        ),
        (
            IDCANCEL.0,
            LocalizationKey::UsageProfileCancel,
            416,
            108,
            WINDOW_STYLE(0),
        ),
    ] {
        let _ = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            localized_text(key, state.language),
            id,
            x,
            70,
            width,
            30,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | extra_style,
        )?;
    }
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
        // SAFETY: WM_NCCREATE는 CreateWindowExW가 전달한 CREATESTRUCTW 포인터를 보장합니다.
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

unsafe extern "system" fn add_dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: WM_NCCREATE는 CreateWindowExW가 전달한 CREATESTRUCTW 포인터를 보장합니다.
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AddDialogState;
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if state.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    match message {
        WM_COMMAND => {
            // SAFETY: GWLP_USERDATA는 모달 호출 동안 살아 있는 Box<AddDialogState>를 가리킵니다.
            handle_add_dialog_command(hwnd, &mut *state, wparam);
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: 상태 포인터는 WM_NCDESTROY 전까지 모달 호출의 Box에 의해 유지됩니다.
            cancel_add_dialog(hwnd, &mut *state);
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
        OPEN_ADD_ID => {
            show_add_profile_prompt_owned(hwnd, state.controller.can_add(), state.language)
        }
        RENAME_ID => submit_rename_label(hwnd, state),
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

unsafe fn submit_rename_label(
    hwnd: HWND,
    state: &DialogState,
) -> Result<Option<ProfileDialogAction>, io::Error> {
    let value = read_profile_label(state.edit)?;
    match state.controller.submit_rename(&value) {
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

/// 추가 입력창의 명시적 추가 또는 취소 명령을 처리합니다.
///
/// 추가는 edit 텍스트를 한 번만 읽어 공유 검증 계약으로 전달합니다. 검증 실패는 창을 유지하고
/// 안전한 지역화 경고를 표시하며, 성공 또는 취소만 결과를 확정하고 창을 닫습니다.
unsafe fn handle_add_dialog_command(hwnd: HWND, state: &mut AddDialogState, wparam: WPARAM) {
    let control_id = (wparam.0 & 0xffff) as i32;
    match control_id {
        id if id == IDOK.0 => {
            let value = match read_profile_label(state.edit) {
                Ok(value) => value,
                Err(_) => {
                    show_safe_error(hwnd, state.language);
                    return;
                }
            };
            match add_profile_prompt_result(&value, AddProfilePromptCommand::Submit) {
                Ok(Some(action)) => {
                    state.result = Some(action);
                    let _ = DestroyWindow(hwnd);
                }
                Ok(None) => {}
                Err(_) => {
                    let _ = show_message(
                        hwnd,
                        localized_text(LocalizationKey::UsageProfileInvalidLabel, state.language),
                        localized_text(LocalizationKey::WindowTitle, state.language),
                        MB_OK | MB_ICONWARNING,
                    );
                }
            }
        }
        id if id == IDCANCEL.0 => cancel_add_dialog(hwnd, state),
        _ => {}
    }
}

/// 추가 입력창을 변경 작업 없이 닫도록 공유 취소 계약을 적용합니다.
unsafe fn cancel_add_dialog(hwnd: HWND, state: &mut AddDialogState) {
    if let Ok(result) = add_profile_prompt_result("", AddProfilePromptCommand::Cancel) {
        state.result = result;
        let _ = DestroyWindow(hwnd);
    }
}

/// 제한된 edit 컨트롤의 UTF-16 표시 이름을 한 번 읽어 Rust 문자열로 변환합니다.
fn read_profile_label(edit: HWND) -> io::Result<String> {
    let mut buffer = [0_u16; PROFILE_LABEL_MAX_UTF16_UNITS + 1];
    // SAFETY: 버퍼는 널 종료 단위를 포함하며 edit는 살아 있는 현재 모달의 자식 컨트롤입니다.
    let length = unsafe { GetWindowTextW(edit, &mut buffer) };
    let length = usize::try_from(length)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid profile label length"))?;
    String::from_utf16(&buffer[..length])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid profile label"))
}

unsafe fn update_controls(hwnd: HWND, state: &DialogState) {
    set_enabled(hwnd, OPEN_ADD_ID, state.controller.can_add());
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
