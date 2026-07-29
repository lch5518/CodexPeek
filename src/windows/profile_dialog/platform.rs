use std::{
    cell::RefCell,
    ffi::c_void,
    io,
    panic::{catch_unwind, AssertUnwindSafe},
};

use windows::{
    core::{w, BOOL, PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
            RECT, WPARAM,
        },
        Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        Graphics::Gdi::{
            CreateFontW, CreateSolidBrush, DeleteObject, FillRect, GetMonitorInfoW, GetStockObject,
            InvalidateRect, MonitorFromPoint, MonitorFromWindow, SetBkColor, SetTextColor,
            CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH, FF_SWISS,
            FW_MEDIUM, FW_NORMAL, HBRUSH, HDC, HFONT, HGDIOBJ, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, OUT_DEFAULT_PRECIS, PROOF_QUALITY,
        },
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Controls::{
                SetWindowTheme, EM_SETLIMITTEXT, TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS,
                TTM_ADDTOOLW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
            },
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetFocus},
            WindowsAndMessaging::{
                CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
                EnumChildWindows, GetClientRect, GetCursorPos, GetDlgItem, GetMessageW,
                GetWindowLongPtrW, GetWindowRect, GetWindowTextW, IsDialogMessageW, IsWindow,
                IsWindowVisible, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW,
                SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
                SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx,
                BS_DEFPUSHBUTTON, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
                GWLP_USERDATA, HCBT_ACTIVATE, HHOOK, HMENU, IDCANCEL, IDC_ARROW, IDOK, IDYES,
                LBN_SELCHANGE, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL, LB_SETCURSEL, MB_ICONERROR,
                MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE,
                MSG, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOW, WH_CBT, WINDOW_STYLE,
                WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
                WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_NCCREATE,
                WM_NCDESTROY, WM_SETFONT, WM_SETTINGCHANGE, WM_THEMECHANGED, WNDCLASSW, WS_BORDER,
                WS_CAPTION, WS_CHILD, WS_EX_DLGMODALFRAME, WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
                WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
            },
        },
    },
};

use crate::windows::{
    design::{scale_logical, DialogPalette, DialogTheme},
    theme,
};
use crate::{localized_text, Language, LocalizationKey, ProfileValidationError};

use super::{
    add_profile_dialog_monitor_anchor, add_profile_prompt_result, centered_dialog_origin,
    profile_delete_confirmation, profile_dialog_keyboard_result, profile_login_confirmation,
    profile_manager_control_enabled, profile_manager_control_spec,
    profile_manager_dialog_monitor_anchor, profile_manager_row_label, show_profile_message,
    AddProfilePromptCommand, AddProfilePromptState, CenteredMessageBoxRequest,
    CenteredMessageBoxRequestState, DialogMonitorAnchor, DialogWindowSize, DialogWorkArea,
    ModalCleanupAction, ModalDialogLifecycle, ProfileDialogAction, ProfileDialogCommand,
    ProfileDialogController, ProfileDialogKeyboardCommand, ProfileDialogKeyboardResult,
    ProfileManagerControl, ProfileManagerDialogState, ProfileMessageRoute, UsageProfileView,
    PROFILE_LABEL_MAX_UTF16_UNITS, PROFILE_MANAGER_CONTROLS,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogFontFace {
    SegoeUiVariable,
    SegoeUi,
}

/// 대화상자 전용 GDI 자원을 생성하고 해제하는 Win32 경계를 분리합니다.
///
/// 테스트 대역은 실제 HWND나 GDI 객체 없이 동일한 소유권 전이를 검증합니다. 구현은 0 핸들을
/// 생성 실패로 취급하며, `delete_object`에는 이 경계가 생성해 소유한 유효 핸들만 전달합니다.
trait DialogResourceBackend {
    fn create_font(&mut self, dpi: u32, heading: bool, face: DialogFontFace) -> HFONT;
    fn stock_font(&mut self) -> HFONT;
    fn create_brush(&mut self, colorref: u32) -> HBRUSH;
    fn delete_object(&mut self, object: HGDIOBJ);
}

struct WindowsDialogResourceBackend;

impl DialogResourceBackend for WindowsDialogResourceBackend {
    fn create_font(&mut self, dpi: u32, heading: bool, face: DialogFontFace) -> HFONT {
        let height = if heading { 18 } else { 14 };
        let weight = if heading { FW_MEDIUM } else { FW_NORMAL };
        let face = match face {
            DialogFontFace::SegoeUiVariable => w!("Segoe UI Variable"),
            DialogFontFace::SegoeUi => w!("Segoe UI"),
        };
        // SAFETY: 고정된 글꼴 속성과 정적 UTF-16 글꼴 이름만 전달하며 반환 핸들의 소유권은
        // DialogVisualResources가 즉시 인수합니다.
        unsafe {
            CreateFontW(
                -scale_logical(height, dpi),
                0,
                0,
                0,
                weight.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                PROOF_QUALITY,
                u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
                face,
            )
        }
    }

    fn stock_font(&mut self) -> HFONT {
        // SAFETY: DEFAULT_GUI_FONT는 프로세스가 소유하지 않는 시스템 stock 객체이며, 반환 핸들은
        // 소유 플래그를 설정하지 않아 DeleteObject에 전달되지 않습니다.
        unsafe { HFONT(GetStockObject(DEFAULT_GUI_FONT).0) }
    }

    fn create_brush(&mut self, colorref: u32) -> HBRUSH {
        // SAFETY: COLORREF 값만 전달하고 반환된 브러시의 소유권을 호출자가 즉시 인수합니다.
        unsafe { CreateSolidBrush(windows::Win32::Foundation::COLORREF(colorref)) }
    }

    fn delete_object(&mut self, object: HGDIOBJ) {
        // SAFETY: 호출자는 이 backend가 생성한 유효하고 아직 해제되지 않은 객체만 전달합니다.
        unsafe {
            let _ = DeleteObject(object);
        }
    }
}

struct DialogResourceSet {
    body_font: HFONT,
    heading_font: HFONT,
    background_brush: HBRUSH,
    surface_brush: HBRUSH,
    owns_body_font: bool,
    owns_heading_font: bool,
    owns_background_brush: bool,
    owns_surface_brush: bool,
}

struct DialogVisualResources {
    dpi: u32,
    palette: DialogPalette,
    body_font: HFONT,
    heading_font: HFONT,
    background_brush: HBRUSH,
    surface_brush: HBRUSH,
    owns_body_font: bool,
    owns_heading_font: bool,
    owns_background_brush: bool,
    owns_surface_brush: bool,
    backend: Box<dyn DialogResourceBackend>,
}

impl DialogVisualResources {
    /// 지정한 DPI와 Windows 테마에 맞는 대화상자 글꼴 및 브러시를 생성합니다.
    ///
    /// 글꼴 생성에 모두 실패하면 삭제하면 안 되는 시스템 기본 글꼴을 빌리며, 생성한 GDI 객체는
    /// 반환값이 소유해 재구성 또는 드롭 시 정확히 한 번 해제합니다.
    fn new(dpi: u32, theme: DialogTheme) -> Self {
        Self::new_with_backend(dpi, theme, Box::new(WindowsDialogResourceBackend))
    }

    fn new_with_backend(
        dpi: u32,
        theme: DialogTheme,
        mut backend: Box<dyn DialogResourceBackend>,
    ) -> Self {
        let palette = DialogPalette::for_theme(theme);
        let resources = Self::allocate(&mut *backend, dpi, palette);
        Self {
            dpi,
            palette,
            body_font: resources.body_font,
            heading_font: resources.heading_font,
            background_brush: resources.background_brush,
            surface_brush: resources.surface_brush,
            owns_body_font: resources.owns_body_font,
            owns_heading_font: resources.owns_heading_font,
            owns_background_brush: resources.owns_background_brush,
            owns_surface_brush: resources.owns_surface_brush,
            backend,
        }
    }

    /// DPI 또는 테마가 바뀐 대화상자의 GDI 자원을 교체합니다.
    ///
    /// 새 자원을 먼저 만든 뒤 이전에 소유한 자원만 해제합니다. stock 글꼴은 빌린 핸들이므로
    /// 해제하지 않으며, 새 자원은 이후 재구성 또는 드롭까지 이 객체가 소유합니다.
    fn rebuild_for_dpi(&mut self, dpi: u32, theme: DialogTheme) {
        let palette = DialogPalette::for_theme(theme);
        if self.dpi == dpi && self.palette == palette {
            return;
        }
        let resources = Self::allocate(&mut *self.backend, dpi, palette);
        self.release_owned();
        self.dpi = dpi;
        self.palette = palette;
        self.body_font = resources.body_font;
        self.heading_font = resources.heading_font;
        self.background_brush = resources.background_brush;
        self.surface_brush = resources.surface_brush;
        self.owns_body_font = resources.owns_body_font;
        self.owns_heading_font = resources.owns_heading_font;
        self.owns_background_brush = resources.owns_background_brush;
        self.owns_surface_brush = resources.owns_surface_brush;
    }

    fn allocate(
        backend: &mut dyn DialogResourceBackend,
        dpi: u32,
        palette: DialogPalette,
    ) -> DialogResourceSet {
        let (body_font, owns_body_font) = create_dialog_font(backend, dpi, false);
        let (heading_font, owns_heading_font) = create_dialog_font(backend, dpi, true);
        let background_brush = backend.create_brush(palette.background.colorref);
        let surface_brush = backend.create_brush(palette.surface.colorref);
        DialogResourceSet {
            body_font,
            heading_font,
            background_brush,
            surface_brush,
            owns_body_font,
            owns_heading_font,
            owns_background_brush: !background_brush.0.is_null(),
            owns_surface_brush: !surface_brush.0.is_null(),
        }
    }

    fn release_owned(&mut self) {
        for (object, owned) in [
            (HGDIOBJ(self.body_font.0), &mut self.owns_body_font),
            (HGDIOBJ(self.heading_font.0), &mut self.owns_heading_font),
            (
                HGDIOBJ(self.background_brush.0),
                &mut self.owns_background_brush,
            ),
            (HGDIOBJ(self.surface_brush.0), &mut self.owns_surface_brush),
        ] {
            if *owned && !object.0.is_null() {
                self.backend.delete_object(object);
                *owned = false;
            }
        }
    }
}

impl Drop for DialogVisualResources {
    fn drop(&mut self) {
        self.release_owned();
    }
}

fn create_dialog_font(
    backend: &mut dyn DialogResourceBackend,
    dpi: u32,
    heading: bool,
) -> (HFONT, bool) {
    for face in [DialogFontFace::SegoeUiVariable, DialogFontFace::SegoeUi] {
        let font = backend.create_font(dpi, heading, face);
        if !font.0.is_null() {
            return (font, true);
        }
    }
    (backend.stock_font(), false)
}

fn current_dialog_theme() -> DialogTheme {
    if theme::system_uses_light_theme() {
        DialogTheme::Light
    } else {
        DialogTheme::Dark
    }
}

struct DialogChildVisualContext {
    body_font: HFONT,
    dark: bool,
}

/// 대화상자와 모든 기본 자식 컨트롤에 현재 글꼴 및 Windows 컨트롤 테마를 적용합니다.
///
/// `dialog`과 열거되는 자식 HWND는 호출 동안 유효해야 하며, `resources`의 글꼴은 컨트롤보다
/// 오래 살아 있어야 합니다. DWM 또는 개별 컨트롤 테마 적용 실패는 지원되지 않는 Windows
/// 버전의 시각적 폴백으로 취급하고 모달 생성을 중단하지 않습니다.
unsafe fn apply_dialog_visuals(dialog: HWND, resources: &DialogVisualResources) {
    let dark = resources.palette == DialogPalette::for_theme(DialogTheme::Dark);
    let dark_attribute = i32::from(dark);
    let _ = DwmSetWindowAttribute(
        dialog,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        (&dark_attribute as *const i32).cast(),
        std::mem::size_of_val(&dark_attribute) as u32,
    );
    let _ = SendMessageW(
        dialog,
        WM_SETFONT,
        Some(WPARAM(resources.heading_font.0 as usize)),
        Some(LPARAM(1)),
    );
    let context = DialogChildVisualContext {
        body_font: resources.body_font,
        dark,
    };
    let _ = EnumChildWindows(
        Some(dialog),
        Some(apply_dialog_child_visuals),
        LPARAM((&context as *const DialogChildVisualContext) as isize),
    );
}

/// `lparam`은 `EnumChildWindows` 호출 동안 살아 있는 `DialogChildVisualContext`를 가리켜야
/// 하며, 콜백은 포인터를 보관하지 않습니다.
unsafe extern "system" fn apply_dialog_child_visuals(child: HWND, lparam: LPARAM) -> BOOL {
    let context = &*(lparam.0 as *const DialogChildVisualContext);
    let _ = SendMessageW(
        child,
        WM_SETFONT,
        Some(WPARAM(context.body_font.0 as usize)),
        Some(LPARAM(1)),
    );
    let sub_app = if context.dark {
        w!("DarkMode_Explorer")
    } else {
        w!("Explorer")
    };
    let _ = SetWindowTheme(child, sub_app, PCWSTR::null());
    BOOL(1)
}

/// 시스템 테마 또는 DPI 변경 후 대화상자 GDI 자원을 교체하고 자식 컨트롤을 다시 스타일링합니다.
///
/// HWND는 살아 있는 대화상자여야 하며 `resources`는 해당 HWND의 모든 자식보다 오래 유지됩니다.
/// 레지스트리, DWM, 테마 API 실패는 어두운 테마 또는 기본 제목 표시줄로 안전하게 폴백합니다.
unsafe fn rebuild_dialog_visuals(dialog: HWND, resources: *mut DialogVisualResources) {
    let dpi = GetDpiForWindow(dialog).max(96);
    // SAFETY: 호출자가 보장한 유효 포인터를 사용하며, 가변 참조는 재진입 가능한 컨트롤 메시지를
    // 보내기 전에 버립니다.
    (&mut *resources).rebuild_for_dpi(dpi, current_dialog_theme());
    apply_dialog_visuals(dialog, &*resources);
    let _ = InvalidateRect(Some(dialog), None, true);
}

unsafe fn dialog_control_color(
    resources: &DialogVisualResources,
    message: u32,
    wparam: WPARAM,
) -> LRESULT {
    let dc = HDC(wparam.0 as *mut c_void);
    let _ = SetTextColor(
        dc,
        windows::Win32::Foundation::COLORREF(resources.palette.text.colorref),
    );
    let background = if message == WM_CTLCOLORSTATIC || message == WM_CTLCOLORBTN {
        resources.palette.background.colorref
    } else {
        resources.palette.surface.colorref
    };
    let _ = SetBkColor(dc, windows::Win32::Foundation::COLORREF(background));
    let brush = if message == WM_CTLCOLORSTATIC || message == WM_CTLCOLORBTN {
        resources.background_brush
    } else {
        resources.surface_brush
    };
    LRESULT(brush.0 as isize)
}

unsafe fn erase_dialog_background(
    dialog: HWND,
    resources: &DialogVisualResources,
    wparam: WPARAM,
) -> LRESULT {
    let mut client = RECT::default();
    if GetClientRect(dialog, &mut client).is_ok() {
        let _ = FillRect(
            HDC(wparam.0 as *mut c_void),
            &client,
            resources.background_brush,
        );
    }
    LRESULT(1)
}

struct DialogState {
    controller: ProfileDialogController,
    interaction: ProfileManagerDialogState,
    language: Language,
    list: HWND,
    edit: HWND,
    add_tooltip: HWND,
    add_tooltip_text: Vec<u16>,
    resources: DialogVisualResources,
}

struct AddDialogState {
    edit: HWND,
    language: Language,
    result: Option<ProfileDialogAction>,
    interaction: AddProfilePromptState,
    resources: DialogVisualResources,
}

/// 프로필 흐름이 선택한 메시지 경로를 실제 표시 경계로 전달합니다.
///
/// 테스트 백엔드는 Win32 UI를 열지 않고 경로와 결과 매핑을 관찰하며, 제품 백엔드는 기존
/// 가운데 배치 `MessageBoxW` 경계를 호출합니다.
trait ProfileMessagePresenter {
    fn present(
        &mut self,
        route: ProfileMessageRoute,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT>;
}

struct CenteredProfileMessagePresenter;

impl ProfileMessagePresenter for CenteredProfileMessagePresenter {
    fn present(
        &mut self,
        route: ProfileMessageRoute,
        owner: HWND,
        message: &str,
        title: &str,
        style: MESSAGEBOX_STYLE,
    ) -> io::Result<MESSAGEBOX_RESULT> {
        // SAFETY: 제품 호출자는 기존과 동일하게 살아 있는 모달 소유자 또는 0 핸들을 전달하며,
        // presenter는 입력 문자열을 공통 경계에서 UTF-16 소유 버퍼로 복사합니다.
        unsafe { show_profile_message(route, owner, message, title, style) }
    }
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

thread_local! {
    /// 현재 UI 스레드에서 다음 프로필 `MessageBoxW` 활성화가 소비할 작업 영역입니다.
    static CENTERED_MESSAGE_BOX_REQUEST: RefCell<CenteredMessageBoxRequestState> =
        RefCell::new(CenteredMessageBoxRequestState::default());
}

/// 한 번의 프로필 `MessageBoxW` 호출에만 적용되는 현재 스레드 CBT 훅입니다.
///
/// 생성 시 이전 스레드 로컬 요청을 값으로 저장하며, 소멸 시 훅 해제 성공 여부와 관계없이 그
/// 요청을 복원합니다. 따라서 중첩 호출도 바깥쪽 요청을 잃지 않습니다.
trait CenteredMessageBoxHookBackend {
    type Hook;

    /// 현재 스레드에 CBT 훅을 설치하고 소유권을 나타내는 핸들을 반환합니다.
    fn install(&mut self) -> Option<Self::Hook>;

    /// 이 백엔드가 설치한 훅을 정확히 한 번 해제합니다.
    fn unhook(&mut self, hook: Self::Hook);
}

struct WindowsCenteredMessageBoxHookBackend;

impl CenteredMessageBoxHookBackend for WindowsCenteredMessageBoxHookBackend {
    type Hook = HHOOK;

    fn install(&mut self) -> Option<Self::Hook> {
        // SAFETY: 정적 콜백을 현재 호출 스레드에만 연결하며 모듈 간 훅을 설치하지 않습니다.
        unsafe {
            SetWindowsHookExW(
                WH_CBT,
                Some(centered_message_box_hook),
                None,
                GetCurrentThreadId(),
            )
            .ok()
        }
    }

    fn unhook(&mut self, hook: Self::Hook) {
        // SAFETY: `hook`은 같은 백엔드가 성공적으로 설치해 가드에 넘긴 핸들입니다.
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

struct CenteredMessageBoxHookGuard<
    B: CenteredMessageBoxHookBackend = WindowsCenteredMessageBoxHookBackend,
> {
    hook: Option<B::Hook>,
    backend: B,
    previous_request: Option<CenteredMessageBoxRequest>,
}

impl CenteredMessageBoxHookGuard<WindowsCenteredMessageBoxHookBackend> {
    /// 이미 해석한 작업 영역을 설치하고 현재 스레드에만 CBT 훅을 연결합니다.
    ///
    /// 훅 설치나 스레드 로컬 접근이 실패하면 요청을 남기지 않고 `None`을 반환합니다. 호출자는
    /// 이 경우에도 `MessageBoxW`를 호출해 Windows 기본 배치를 유지해야 합니다.
    fn install(work_area: DialogWorkArea) -> Option<Self> {
        Self::install_with_backend(work_area, WindowsCenteredMessageBoxHookBackend)
    }
}

impl<B: CenteredMessageBoxHookBackend> CenteredMessageBoxHookGuard<B> {
    /// 작업 영역 요청과 주입된 훅 백엔드를 하나의 RAII 수명으로 묶습니다.
    ///
    /// 백엔드 설치 실패 시 새 요청을 남기지 않고 이전 스레드 로컬 값을 즉시 복원합니다.
    fn install_with_backend(work_area: DialogWorkArea, mut backend: B) -> Option<Self> {
        let request = CenteredMessageBoxRequest::new(work_area);
        let previous_request = CENTERED_MESSAGE_BOX_REQUEST.with(|state| {
            state
                .try_borrow_mut()
                .ok()
                .map(|mut state| state.install(request))
        })?;

        let hook = match backend.install() {
            Some(hook) => hook,
            None => {
                CENTERED_MESSAGE_BOX_REQUEST.with(|state| {
                    state.borrow_mut().restore(previous_request);
                });
                return None;
            }
        };

        Some(Self {
            hook: Some(hook),
            backend,
            previous_request,
        })
    }
}

impl<B: CenteredMessageBoxHookBackend> Drop for CenteredMessageBoxHookGuard<B> {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            self.backend.unhook(hook);
        }
        CENTERED_MESSAGE_BOX_REQUEST.with(|state| {
            state.borrow_mut().restore(self.previous_request);
        });
    }
}

/// 현재 스레드의 첫 `HCBT_ACTIVATE` 요청을 소비해 프로필 메시지 상자를 가운데로 옮깁니다.
///
/// # Safety
///
/// Windows가 `WH_CBT` 훅 계약에 따라 호출해야 합니다. 콜백 내부 작업은 패닉 경계로 감싸 FFI
/// 밖으로 언와인드하지 않으며, 모든 코드 경로에서 다음 훅을 호출합니다.
unsafe extern "system" fn centered_message_box_hook(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if code != HCBT_ACTIVATE as i32 {
            return;
        }
        let request = consume_centered_message_box_request();
        if let Some(request) = request {
            // SAFETY: `HCBT_ACTIVATE`의 `wparam`은 활성화되는 창의 HWND 값입니다. 요청은 여기서
            // 이미 값으로 소비되어 중첩 메시지 루프에 Rust 참조를 유지하지 않습니다.
            unsafe {
                center_window_in_work_area(HWND(wparam.0 as *mut c_void), request.work_area);
            }
        }
    }));

    // SAFETY: Win32 훅 계약상 처리 여부와 관계없이 체인의 다음 훅으로 원래 인자를 전달합니다.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 현재 스레드의 가운데 배치 요청을 정확히 한 번 값으로 소비합니다.
///
/// 훅 콜백과 테스트 가능한 RAII 생명주기가 같은 스레드 로컬 전이를 사용하며, 재진입 중 이미
/// 빌린 상태이면 창 이동을 포기하고 `None`을 반환합니다.
fn consume_centered_message_box_request() -> Option<CenteredMessageBoxRequest> {
    CENTERED_MESSAGE_BOX_REQUEST.with(|state| {
        state
            .try_borrow_mut()
            .ok()
            .and_then(|mut state| state.consume())
    })
}

/// 지정한 기준에 맞는 모니터 작업 영역의 가운데로 창을 이동합니다.
///
/// 배치에 필요한 Win32 조회나 이동이 실패하면 창의 기존 기본 위치를 유지합니다. 첫 이동 뒤 DPI
/// 전환으로 외곽 크기가 달라진 경우에만 한 번 더 이동하며, 창 크기나 Z 순서는 바꾸지 않습니다.
///
/// # Safety
///
/// `dialog`는 호출 동안 유효한 최상위 창이어야 합니다. 이 함수는 창 핸들을 소유하지 않고 파괴하지
/// 않으며, 실패를 호출자에게 전파하지 않습니다.
unsafe fn center_window(dialog: HWND, anchor: DialogMonitorAnchor) {
    let Some(work_area) = dialog_work_area(anchor) else {
        return;
    };
    center_window_in_work_area(dialog, work_area);
}

/// 미리 해석한 작업 영역 가운데로 창을 옮기며 DPI 변화로 크기가 바뀐 경우 한 번 보정합니다.
///
/// # Safety
///
/// `dialog`는 호출 시점에 유효한 최상위 창이어야 합니다. 조회나 이동 실패는 기존 창 위치를
/// 보존하며 호출자에게 전파하지 않습니다.
unsafe fn center_window_in_work_area(dialog: HWND, work_area: DialogWorkArea) {
    let Some(initial_size) = live_window_size(dialog) else {
        return;
    };

    move_window_to_center(dialog, work_area, initial_size);

    if let Some(current_size) = live_window_size(dialog) {
        if current_size != initial_size {
            move_window_to_center(dialog, work_area, current_size);
        }
    }
}

/// 기준 정책에서 실제 모니터의 작업 영역을 안전하게 읽습니다.
///
/// 소유자 기준이 더 이상 유효하지 않으면 커서 모니터를 사용하고, 모니터 정보를 읽지 못하면 `None`을
/// 반환해 호출자가 기존 창 위치를 유지하게 합니다.
unsafe fn dialog_work_area(anchor: DialogMonitorAnchor) -> Option<DialogWorkArea> {
    let monitor = match anchor {
        DialogMonitorAnchor::Cursor => cursor_monitor(),
        DialogMonitorAnchor::Owner(owner) => {
            if live_window_size(owner).is_some() {
                let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
                if monitor != Default::default() {
                    monitor
                } else {
                    cursor_monitor()
                }
            } else {
                cursor_monitor()
            }
        }
    };
    if monitor == Default::default() {
        return None;
    }

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return None;
    }
    Some(DialogWorkArea::new(
        info.rcWork.left,
        info.rcWork.top,
        info.rcWork.right,
        info.rcWork.bottom,
    ))
}

/// 현재 커서 위치의 가장 가까운 모니터를 찾고, 조회 실패 시 기본 모니터를 사용합니다.
unsafe fn cursor_monitor() -> windows::Win32::Graphics::Gdi::HMONITOR {
    let mut point = POINT::default();
    if GetCursorPos(&mut point).is_ok() {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if monitor != Default::default() {
            return monitor;
        }
    }
    MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
}

/// 창의 현재 외곽 크기를 읽어 배치에 사용할 수 있는지 확인합니다.
///
/// 유효하지 않은 핸들, 조회 실패, 0 이하의 크기는 `None`으로 변환해 소유자 기준을 커서 기준으로
/// 안전하게 되돌릴 수 있게 합니다.
unsafe fn live_window_size(window: HWND) -> Option<DialogWindowSize> {
    if window == HWND::default() || !IsWindow(Some(window)).as_bool() {
        return None;
    }

    let mut rect = RECT::default();
    GetWindowRect(window, &mut rect).ok()?;
    let width = i64::from(rect.right) - i64::from(rect.left);
    let height = i64::from(rect.bottom) - i64::from(rect.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    Some(DialogWindowSize::new(
        width.min(i64::from(i32::MAX)) as i32,
        height.min(i64::from(i32::MAX)) as i32,
    ))
}

/// 프로필 메시지 상자가 사용할 모니터 작업 영역을 호출 전에 결정합니다.
///
/// 보이고 크기가 유효한 소유자만 소유자 모니터를 선택하며, 숨김 창, 0 핸들, 잘못된 창,
/// 조회 실패는 커서 모니터 정책으로 대체합니다. 최종 모니터 정보 실패는 `None`입니다.
unsafe fn profile_message_work_area(owner: HWND) -> Option<DialogWorkArea> {
    let owner_size = if owner != HWND::default() && IsWindowVisible(owner).as_bool() {
        live_window_size(owner)
    } else {
        None
    };
    dialog_work_area(add_profile_dialog_monitor_anchor(owner, owner_size))
}

/// 현재 외곽 크기를 보존한 채 창의 좌상단을 작업 영역 가운데로 옮깁니다.
///
/// `SetWindowPos` 실패는 무시해 창 생성이나 프로필 작업을 중단하지 않습니다.
unsafe fn move_window_to_center(
    dialog: HWND,
    work_area: DialogWorkArea,
    window_size: DialogWindowSize,
) {
    let origin = centered_dialog_origin(work_area, window_size);
    let _ = SetWindowPos(
        dialog,
        None,
        origin.x,
        origin.y,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
    );
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
    let dialog_theme = current_dialog_theme();

    let mut state = Box::new(DialogState {
        controller: ProfileDialogController::new(profiles, mutation_pending),
        interaction: ProfileManagerDialogState::new(),
        language,
        list: HWND::default(),
        edit: HWND::default(),
        add_tooltip: HWND::default(),
        add_tooltip_text: Vec::new(),
        resources: DialogVisualResources::new(96, dialog_theme),
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
    rebuild_dialog_visuals(dialog, std::ptr::addr_of_mut!(state.resources));
    center_window(dialog, profile_manager_dialog_monitor_anchor());

    window_guard.disable_owner();
    let _ = ShowWindow(dialog, SW_SHOW);
    let _ = SetForegroundWindow(dialog);
    let _ = SetFocus(Some(state.edit));

    run_modal_message_loop(dialog)?;
    Ok(state.interaction.take_result())
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
    let dialog_theme = current_dialog_theme();

    let mut state = Box::new(AddDialogState {
        edit: HWND::default(),
        language,
        result: None,
        interaction: AddProfilePromptState::new(),
        resources: DialogVisualResources::new(96, dialog_theme),
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
    rebuild_dialog_visuals(dialog, std::ptr::addr_of_mut!(state.resources));
    center_window(
        dialog,
        add_profile_dialog_monitor_anchor(owner, live_window_size(owner)),
    );

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
        let copy = profile_manager_control_spec(control, state.language);
        let (id, x, y, width, height) = manager_control_layout(control);
        let control_window = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            copy.visible_text,
            id,
            x,
            y,
            width,
            height,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        )?;
        if let Some(description) = copy.accessible_description {
            create_add_control_tooltip(dialog, instance, control_window, description, state)?;
        }
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

/// 공유 관리자 컨트롤 계약을 Win32 ID와 고정 배치로 변환합니다.
///
/// 화면 문구와 접근성 설명은 플랫폼 독립 `profile_manager_control_spec`에서 가져오며, 추가 버튼은
/// 목록 바로 아래에, 나머지 네 작업은 하단 행에 배치됩니다.
fn manager_control_layout(control: ProfileManagerControl) -> (i32, i32, i32, i32, i32) {
    match control {
        ProfileManagerControl::AddBelowList => (OPEN_ADD_ID, 16, 216, 36, 26),
        ProfileManagerControl::Rename => (RENAME_ID, 116, 270, 92, 30),
        ProfileManagerControl::Login => (LOGIN_ID, 216, 270, 92, 30),
        ProfileManagerControl::Logout => (LOGOUT_ID, 316, 270, 92, 30),
        ProfileManagerControl::Delete => (DELETE_ID, 416, 270, 92, 30),
    }
}

/// 목록 아래 `+` 컨트롤에 지역화된 설명 tooltip을 연결합니다.
///
/// tooltip 컨트롤은 관리자 창이 소유하므로 관리자 파괴와 함께 정리됩니다. `TTM_ADDTOOLW`가
/// 문자열 포인터를 참조하는 동안 버퍼가 이동하지 않도록 텍스트는 `DialogState`에 보관합니다.
/// 등록이 실패하면 생성한 tooltip만 즉시 파괴하고 오류를 반환해 호출자의 모달 가드가 나머지
/// 부분 생성 상태를 정리하게 합니다.
unsafe fn create_add_control_tooltip(
    dialog: HWND,
    instance: HINSTANCE,
    control: HWND,
    description: &str,
    state: &mut DialogState,
) -> io::Result<()> {
    let tooltip = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        TOOLTIPS_CLASSW,
        PCWSTR::null(),
        WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        Some(dialog),
        None,
        Some(instance),
        None,
    )
    .map_err(win_error)?;
    state.add_tooltip = tooltip;
    state.add_tooltip_text = wide(description);
    let tool = TTTOOLINFOW {
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: dialog,
        uId: control.0 as usize,
        hinst: instance,
        lpszText: PWSTR(state.add_tooltip_text.as_mut_ptr()),
        ..Default::default()
    };
    let added = SendMessageW(
        tooltip,
        TTM_ADDTOOLW,
        Some(WPARAM(0)),
        Some(LPARAM((&tool as *const TTTOOLINFOW) as isize)),
    );
    if added.0 == 0 {
        let _ = DestroyWindow(tooltip);
        state.add_tooltip = HWND::default();
        state.add_tooltip_text.clear();
        return Err(io::Error::last_os_error());
    }
    Ok(())
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
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DPICHANGED => {
            rebuild_dialog_visuals(hwnd, std::ptr::addr_of_mut!((*state).resources));
            LRESULT(0)
        }
        WM_ERASEBKGND => erase_dialog_background(hwnd, &(*state).resources, wparam),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            dialog_control_color(&(*state).resources, message, wparam)
        }
        WM_COMMAND => {
            if manager_accepts_commands(hwnd, state) {
                // SAFETY: 원시 포인터만 전달하며 중첩 메시지 루프 전후에 참조를 보관하지 않습니다.
                handle_command(hwnd, state, wparam);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if manager_accepts_commands(hwnd, state) {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// 현재 관리자 HWND와 순수 중첩 상태가 사용자 명령을 모두 허용하는지 확인합니다.
///
/// 자식 모달 또는 메시지 상자가 소유자를 비활성화했거나 추가 입력창 전이가 진행 중이면 명령을
/// 거부합니다. `state`는 `GWLP_USERDATA`에서 읽은 살아 있는 관리자 상태여야 합니다. 반환 전에
/// 임시 공유 참조를 버리므로 이후 중첩 메시지 루프와 겹치지 않습니다.
unsafe fn manager_accepts_commands(hwnd: HWND, state: *mut DialogState) -> bool {
    IsWindowEnabled(hwnd).as_bool() && (&*state).interaction.accepts_manager_commands()
}

/// 현재 추가 입력창이 활성화되어 있고 중첩 처리 없이 새 명령을 받을 수 있는지 확인합니다.
///
/// `state`는 `GWLP_USERDATA`가 가리키는 살아 있는 모달 상태여야 합니다. 함수 안에서 만든 공유
/// 참조는 즉시 버리므로 이후 경고 메시지 상자의 중첩 메시지 루프와 겹치지 않습니다.
unsafe fn add_dialog_accepts_commands(hwnd: HWND, state: *mut AddDialogState) -> bool {
    IsWindowEnabled(hwnd).as_bool() && (&*state).interaction.accepts_commands()
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
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DPICHANGED => {
            rebuild_dialog_visuals(hwnd, std::ptr::addr_of_mut!((*state).resources));
            LRESULT(0)
        }
        WM_ERASEBKGND => erase_dialog_background(hwnd, &(*state).resources, wparam),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            dialog_control_color(&(*state).resources, message, wparam)
        }
        WM_COMMAND => {
            if add_dialog_accepts_commands(hwnd, state) {
                // SAFETY: 원시 포인터만 전달하며 중첩 경고 전후에 Rust 참조를 보관하지 않습니다.
                handle_add_dialog_command(hwnd, state, wparam);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if add_dialog_accepts_commands(hwnd, state) {
                // SAFETY: 상태 포인터는 WM_NCDESTROY 전까지 모달 호출의 Box에 의해 유지됩니다.
                cancel_add_dialog(hwnd, state);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// 관리자 명령을 처리하되 중첩 모달 호출을 가로질러 Rust 참조를 보관하지 않습니다.
///
/// `state`는 관리자 모달 호출의 `Box<DialogState>`를 가리키며 `GWLP_USERDATA`가 제거되기 전의
/// 같은 UI 스레드에서만 호출해야 합니다. 중첩 호출 전 필요한 값만 복사하고, 반환 후 새로
/// 참조를 만들기 때문에 재진입 시 별칭 가능한 `&mut DialogState`가 남지 않습니다.
unsafe fn handle_command(hwnd: HWND, state: *mut DialogState, wparam: WPARAM) {
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
        let list = (&*state).list;
        let selected = SendMessageW(list, LB_GETCURSEL, None, None).0;
        if selected >= 0 {
            let state = &mut *state;
            if state.controller.select(selected as usize) {
                update_controls(hwnd, state);
            }
        }
        return;
    }

    let action = match control_id {
        OPEN_ADD_ID => {
            open_add_profile_prompt(hwnd, state);
            return;
        }
        RENAME_ID => submit_rename_label(hwnd, state),
        LOGIN_ID => Ok((&*state)
            .controller
            .confirmed_command(ProfileDialogCommand::Login, true)),
        LOGOUT_ID => Ok((&*state)
            .controller
            .confirmed_command(ProfileDialogCommand::Logout, true)),
        DELETE_ID => submit_delete(hwnd, state),
        _ => return,
    };

    match action {
        Ok(Some(action)) => {
            if (&mut *state).interaction.close_with_action(action) {
                let _ = DestroyWindow(hwnd);
            }
        }
        Ok(None) => {}
        Err(_) => {
            let language = (&*state).language;
            show_safe_error(hwnd, language);
        }
    }
}

/// 추가 입력창을 단일 자식 전이로 실행하고 결과를 관리자에 한 번 반영합니다.
///
/// 순수 상태를 먼저 `AddPromptActive`로 전환한 뒤 모든 Rust 참조를 버리고 중첩 메시지 루프에
/// 진입합니다. 오류와 취소는 활성 관리자로 복귀하며, Add 결과만 관리자를 닫습니다.
unsafe fn open_add_profile_prompt(hwnd: HWND, state: *mut DialogState) {
    let (can_add, language) = {
        let state = &mut *state;
        let can_add = state.controller.can_add();
        if !state.interaction.begin_add_prompt(can_add) {
            return;
        }
        (can_add, state.language)
    };

    let prompt_result = show_add_profile_prompt_owned(hwnd, can_add, language);
    match prompt_result {
        Ok(result) => {
            let (accepted, close_manager) = {
                let state = &mut *state;
                let accepted = state.interaction.finish_add_prompt(result);
                let close_manager = !state.interaction.accepts_manager_commands();
                (accepted, close_manager)
            };
            if !accepted {
                show_safe_error(hwnd, language);
            } else if close_manager {
                let _ = DestroyWindow(hwnd);
            }
        }
        Err(_) => {
            let _ = (&mut *state).interaction.finish_add_prompt(None);
            show_safe_error(hwnd, language);
        }
    }
}

/// 관리자 상태 참조를 보관하지 않고 이름 변경 입력을 검증합니다.
///
/// `state`는 모달 호출의 살아 있는 상태 포인터여야 하며, 경고 메시지 상자 전에 edit와 언어를
/// 값으로 복사하고 검증 결과를 소유 값으로 변환합니다.
unsafe fn submit_rename_label(
    hwnd: HWND,
    state: *mut DialogState,
) -> Result<Option<ProfileDialogAction>, io::Error> {
    let (edit, language) = {
        let state = &*state;
        (state.edit, state.language)
    };
    let value = read_profile_label(edit)?;
    let action = (&*state).controller.submit_rename(&value);
    let mut presenter = CenteredProfileMessagePresenter;
    handle_manager_rename_result_with_presenter(action, hwnd, language, &mut presenter)
}

/// 관리자 이름 변경 결과를 그대로 반환하거나 검증 실패를 프로필 경고 경로로 표시합니다.
///
/// `action`은 공용 프로필 검증을 이미 거친 결과이며, presenter 오류만 I/O 오류로 전파합니다.
fn handle_manager_rename_result_with_presenter<P: ProfileMessagePresenter>(
    action: Result<Option<ProfileDialogAction>, ProfileValidationError>,
    owner: HWND,
    language: Language,
    presenter: &mut P,
) -> Result<Option<ProfileDialogAction>, io::Error> {
    match action {
        Ok(action) => Ok(action),
        Err(_) => {
            presenter.present(
                ProfileMessageRoute::ValidationWarning,
                owner,
                localized_text(LocalizationKey::UsageProfileInvalidLabel, language),
                localized_text(LocalizationKey::WindowTitle, language),
                MB_OK | MB_ICONWARNING,
            )?;
            Ok(None)
        }
    }
}

/// 삭제 확인 중 관리자 상태 참조를 보관하지 않고 확인 결과를 현재 선택에 적용합니다.
///
/// `state`는 모달 호출의 살아 있는 상태 포인터여야 합니다. 확인 전에 표시 이름과 언어만
/// 복사하고, 중첩 메시지 상자가 닫힌 뒤 새 참조로 명령 가용성을 다시 확인합니다.
unsafe fn submit_delete(
    hwnd: HWND,
    state: *mut DialogState,
) -> Result<Option<ProfileDialogAction>, io::Error> {
    let (label, language) = {
        let state = &*state;
        let Some(profile) = state.controller.selected_profile() else {
            return Ok(None);
        };
        (profile.label.clone(), state.language)
    };
    let confirmed = confirm_profile_delete_owned(hwnd, &label, language)?;
    Ok((&*state)
        .controller
        .confirmed_command(ProfileDialogCommand::Delete, confirmed))
}

/// 추가 입력창의 명시적 추가 또는 취소 명령을 원시 상태 포인터로 처리합니다.
///
/// `state`는 모달 호출의 `Box<AddDialogState>`를 가리켜야 합니다. 각 하위 처리는 명령을 먼저
/// 순수 상태에 예약하고, 중첩 경고 전에 필요한 HWND·언어만 복사하므로 재진입 가능한 `&mut`
/// 참조를 메시지 루프 너머로 보관하지 않습니다.
unsafe fn handle_add_dialog_command(hwnd: HWND, state: *mut AddDialogState, wparam: WPARAM) {
    let control_id = (wparam.0 & 0xffff) as i32;
    match control_id {
        id if id == IDOK.0 => submit_add_dialog(hwnd, state),
        id if id == IDCANCEL.0 => cancel_add_dialog(hwnd, state),
        _ => {}
    }
}

/// 이름을 한 번 읽고 검증해 추가 결과를 확정하거나 경고 뒤 활성 입력 상태로 복구합니다.
///
/// 텍스트 읽기와 검증은 `Handling` 상태에서 수행하고, 경고가 필요한 경우 언어와 메시지 종류만
/// 복사한 뒤 참조를 버립니다. 성공 결과는 `Closed` 전이가 성공한 경우에만 한 번 저장합니다.
unsafe fn submit_add_dialog(hwnd: HWND, state: *mut AddDialogState) {
    let edit = {
        let state = &mut *state;
        if !state.interaction.begin_command() {
            return;
        }
        state.edit
    };
    let value = match read_profile_label(edit) {
        Ok(value) => value,
        Err(_) => {
            show_add_dialog_warning(
                hwnd,
                state,
                LocalizationKey::UsageProfileOperationFailed,
                MB_OK | MB_ICONERROR,
            );
            return;
        }
    };
    let result = add_profile_prompt_result(&value, AddProfilePromptCommand::Submit);
    let mut presenter = CenteredProfileMessagePresenter;
    handle_add_profile_prompt_result_with_presenter(hwnd, state, result, &mut presenter);
}

/// 추가 이름 검증 결과를 닫기 또는 경고 상태 전이로 적용합니다.
///
/// # Safety
///
/// `state`는 처리 중 단계의 살아 있는 `AddDialogState`를 가리켜야 하며 다른 가변 참조와 별칭되면
/// 안 됩니다. 성공 시에만 결과를 저장하고 창을 닫으며, 실패는 주입된 presenter로 표시합니다.
unsafe fn handle_add_profile_prompt_result_with_presenter<P: ProfileMessagePresenter>(
    hwnd: HWND,
    state: *mut AddDialogState,
    result: Result<Option<ProfileDialogAction>, ProfileValidationError>,
    presenter: &mut P,
) {
    match result {
        Ok(Some(action)) => {
            let should_close = {
                let state = &mut *state;
                if state.interaction.finish_close() {
                    state.result = Some(action);
                    true
                } else {
                    false
                }
            };
            if should_close {
                let _ = DestroyWindow(hwnd);
            }
        }
        Ok(None) => {
            show_add_dialog_warning_with_presenter(
                hwnd,
                state,
                LocalizationKey::UsageProfileOperationFailed,
                MB_OK | MB_ICONERROR,
                presenter,
            );
        }
        Err(_) => {
            show_add_dialog_warning_with_presenter(
                hwnd,
                state,
                LocalizationKey::UsageProfileInvalidLabel,
                MB_OK | MB_ICONWARNING,
                presenter,
            );
        }
    }
}

/// 추가 입력창을 변경 작업 없이 한 번만 닫도록 공유 취소 계약을 적용합니다.
unsafe fn cancel_add_dialog(hwnd: HWND, state: *mut AddDialogState) {
    let should_close = {
        let state = &mut *state;
        if !state.interaction.begin_command() {
            return;
        }
        let Ok(result) = add_profile_prompt_result("", AddProfilePromptCommand::Cancel) else {
            return;
        };
        if state.interaction.finish_close() {
            state.result = result;
            true
        } else {
            false
        }
    };
    if should_close {
        let _ = DestroyWindow(hwnd);
    }
}

/// 추가 입력창 상태를 경고 단계로 전환하고 지역화 메시지를 표시한 뒤 다시 활성화합니다.
///
/// `MessageBoxW`는 중첩 메시지 루프를 실행하므로 호출 전 언어만 복사하고 모든 Rust 참조를
/// 버립니다. `Warning` 단계와 비활성 HWND 검사는 중첩된 제출·취소·닫기를 거부하며, 반환 뒤에만
/// 새 가변 참조를 만들어 `Live`로 복구합니다.
unsafe fn show_add_dialog_warning(
    hwnd: HWND,
    state: *mut AddDialogState,
    message_key: LocalizationKey,
    style: MESSAGEBOX_STYLE,
) {
    let mut presenter = CenteredProfileMessagePresenter;
    show_add_dialog_warning_with_presenter(hwnd, state, message_key, style, &mut presenter);
}

/// 추가 입력창의 경고 상태 전이와 메시지 경로 선택을 하나의 테스트 가능한 경계로 실행합니다.
///
/// # Safety
///
/// `state`는 호출 동안 살아 있는 `AddDialogState`를 가리켜야 하며 다른 가변 참조와 별칭되면 안
/// 됩니다. presenter 호출 전후로만 짧은 참조를 만들고 중첩 메시지 루프에는 보관하지 않습니다.
unsafe fn show_add_dialog_warning_with_presenter<P: ProfileMessagePresenter>(
    hwnd: HWND,
    state: *mut AddDialogState,
    message_key: LocalizationKey,
    style: MESSAGEBOX_STYLE,
    presenter: &mut P,
) {
    let language = {
        let state = &mut *state;
        if !state.interaction.begin_warning() {
            return;
        }
        state.language
    };
    let route = if message_key == LocalizationKey::UsageProfileInvalidLabel {
        ProfileMessageRoute::ValidationWarning
    } else {
        ProfileMessageRoute::AddPromptSafeError
    };
    let _ = presenter.present(
        route,
        hwnd,
        localized_text(message_key, language),
        localized_text(LocalizationKey::WindowTitle, language),
        style,
    );
    let _ = (&mut *state).interaction.finish_warning();
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
    for control in PROFILE_MANAGER_CONTROLS {
        let (id, _, _, _, _) = manager_control_layout(control);
        set_enabled(
            hwnd,
            id,
            profile_manager_control_enabled(&state.controller, control),
        );
    }
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
    let mut presenter = CenteredProfileMessagePresenter;
    confirm_profile_login_with_presenter(owner, label, language, &mut presenter)
}

/// 로그인 확인 문구를 프로필 로그인 경로로 표시하고 기존 OK/취소 매핑을 반환합니다.
fn confirm_profile_login_with_presenter<P: ProfileMessagePresenter>(
    owner: HWND,
    label: &str,
    language: Language,
    presenter: &mut P,
) -> io::Result<bool> {
    let message = profile_login_confirmation(label, language);
    let title = localized_text(LocalizationKey::UsageProfileLogin, language);
    presenter
        .present(
            ProfileMessageRoute::LoginConfirmation,
            owner,
            &message,
            title,
            MB_OKCANCEL | MB_ICONWARNING,
        )
        .map(|result| result == IDOK)
}

unsafe fn confirm_profile_delete_owned(
    owner: HWND,
    label: &str,
    language: Language,
) -> io::Result<bool> {
    let mut presenter = CenteredProfileMessagePresenter;
    confirm_profile_delete_with_presenter(owner, label, language, &mut presenter)
}

/// 삭제 확인 문구를 프로필 삭제 경로로 표시하고 기존 Yes/No 매핑을 반환합니다.
fn confirm_profile_delete_with_presenter<P: ProfileMessagePresenter>(
    owner: HWND,
    label: &str,
    language: Language,
    presenter: &mut P,
) -> io::Result<bool> {
    let message = profile_delete_confirmation(label, language);
    let title = localized_text(LocalizationKey::UsageProfileDelete, language);
    presenter
        .present(
            ProfileMessageRoute::DeleteConfirmation,
            owner,
            &message,
            title,
            MB_YESNO | MB_ICONWARNING,
        )
        .map(|result| result == IDYES)
}

unsafe fn show_safe_error(owner: HWND, language: Language) {
    let mut presenter = CenteredProfileMessagePresenter;
    show_safe_error_with_presenter(owner, language, &mut presenter);
}

/// 관리자 작업 실패를 안전한 고정 문구와 관리자 오류 경로로 표시합니다.
fn show_safe_error_with_presenter<P: ProfileMessagePresenter>(
    owner: HWND,
    language: Language,
    presenter: &mut P,
) {
    let _ = presenter.present(
        ProfileMessageRoute::ManagerSafeError,
        owner,
        localized_text(LocalizationKey::UsageProfileOperationFailed, language),
        localized_text(LocalizationKey::WindowTitle, language),
        MB_OK | MB_ICONERROR,
    );
}

pub(super) unsafe fn show_centered_profile_message(
    owner: HWND,
    message: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> io::Result<windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT> {
    let work_area = profile_message_work_area(owner);
    let message = wide(message);
    let title = wide(title);
    let parent = (owner != HWND::default()).then_some(owner);
    let centering_guard = work_area.and_then(CenteredMessageBoxHookGuard::install);
    let result = MessageBoxW(
        parent,
        PCWSTR(message.as_ptr()),
        PCWSTR(title.as_ptr()),
        style,
    );
    let error = (result.0 == 0).then(io::Error::last_os_error);
    drop(centering_guard);
    if let Some(error) = error {
        Err(error)
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, ffi::c_void, io, rc::Rc, thread};

    use windows::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{HBRUSH, HFONT, HGDIOBJ},
        UI::WindowsAndMessaging::{
            IDCANCEL, IDOK, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MESSAGEBOX_RESULT,
            MESSAGEBOX_STYLE,
        },
    };

    use crate::{windows::design::DialogTheme, Language, LocalizationKey, UsageProfileId};

    use super::{
        add_profile_prompt_result, confirm_profile_delete_with_presenter,
        confirm_profile_login_with_presenter, consume_centered_message_box_request,
        handle_add_profile_prompt_result_with_presenter,
        handle_manager_rename_result_with_presenter, show_add_dialog_warning_with_presenter,
        show_safe_error_with_presenter, AddDialogState, AddProfilePromptCommand,
        CenteredMessageBoxHookBackend, CenteredMessageBoxHookGuard, DialogFontFace,
        DialogResourceBackend, DialogVisualResources, DialogWorkArea, ProfileDialogController,
        ProfileMessagePresenter, ProfileMessageRoute, UsageProfileView,
    };

    #[derive(Default)]
    struct ResourceCalls {
        font_faces: Vec<DialogFontFace>,
        deleted: Vec<usize>,
    }

    struct RecordingResourceBackend {
        calls: Rc<RefCell<ResourceCalls>>,
        fonts: VecDeque<usize>,
        brushes: VecDeque<usize>,
        stock_font: usize,
    }

    impl DialogResourceBackend for RecordingResourceBackend {
        fn create_font(&mut self, _dpi: u32, _heading: bool, face: DialogFontFace) -> HFONT {
            self.calls.borrow_mut().font_faces.push(face);
            font_handle(self.fonts.pop_front().unwrap_or_default())
        }

        fn stock_font(&mut self) -> HFONT {
            font_handle(self.stock_font)
        }

        fn create_brush(&mut self, _colorref: u32) -> HBRUSH {
            brush_handle(self.brushes.pop_front().unwrap_or_default())
        }

        fn delete_object(&mut self, object: HGDIOBJ) {
            self.calls.borrow_mut().deleted.push(object.0 as usize);
        }
    }

    fn font_handle(value: usize) -> HFONT {
        HFONT(value as *mut c_void)
    }

    fn brush_handle(value: usize) -> HBRUSH {
        HBRUSH(value as *mut c_void)
    }

    fn test_visual_resources() -> DialogVisualResources {
        DialogVisualResources::new_with_backend(
            96,
            DialogTheme::Dark,
            Box::new(RecordingResourceBackend {
                calls: Rc::new(RefCell::new(ResourceCalls::default())),
                fonts: VecDeque::new(),
                brushes: VecDeque::new(),
                stock_font: 99,
            }),
        )
    }

    #[test]
    fn font_creation_retries_segoe_ui_before_using_stock_font() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([0, 0, 0, 0]),
            brushes: VecDeque::from([21, 22]),
            stock_font: 99,
        };

        let resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));
        drop(resources);

        assert_eq!(
            calls.borrow().font_faces,
            [
                DialogFontFace::SegoeUiVariable,
                DialogFontFace::SegoeUi,
                DialogFontFace::SegoeUiVariable,
                DialogFontFace::SegoeUi,
            ]
        );
        assert!(!calls.borrow().deleted.contains(&99));
    }

    #[test]
    fn rebuild_and_teardown_delete_each_owned_handle_once() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([11, 0, 0, 31, 32]),
            brushes: VecDeque::from([21, 22, 41, 42]),
            stock_font: 99,
        };
        let mut resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));

        resources.rebuild_for_dpi(144, DialogTheme::Light);
        drop(resources);

        let mut deleted = calls.borrow().deleted.clone();
        deleted.sort_unstable();
        assert_eq!(deleted, [11, 21, 22, 31, 32, 41, 42]);
        assert!(!deleted.contains(&99));
    }

    #[derive(Debug, PartialEq, Eq)]
    struct PresentedProfileMessage {
        route: ProfileMessageRoute,
        owner: HWND,
        message: String,
        style: MESSAGEBOX_STYLE,
    }

    struct RecordingProfileMessagePresenter {
        messages: Vec<PresentedProfileMessage>,
        result: MESSAGEBOX_RESULT,
    }

    impl RecordingProfileMessagePresenter {
        fn returning(result: MESSAGEBOX_RESULT) -> Self {
            Self {
                messages: Vec::new(),
                result,
            }
        }
    }

    impl ProfileMessagePresenter for RecordingProfileMessagePresenter {
        fn present(
            &mut self,
            route: ProfileMessageRoute,
            owner: HWND,
            message: &str,
            _title: &str,
            style: MESSAGEBOX_STYLE,
        ) -> io::Result<MESSAGEBOX_RESULT> {
            self.messages.push(PresentedProfileMessage {
                route,
                owner,
                message: message.to_string(),
                style,
            });
            Ok(self.result)
        }
    }

    #[test]
    fn manager_validation_path_presents_the_validation_route() {
        let profile = UsageProfileView {
            id: UsageProfileId::Managed(7),
            label: "Work".to_string(),
            summary: "Ready".to_string(),
            selected: true,
            login_required: false,
            used_percent: None,
            usage_status: None,
            managed: true,
        };
        let controller = ProfileDialogController::new(&[profile], false);
        let invalid_submission = controller.submit_rename("   ");
        let owner = HWND(101_usize as _);
        let mut presenter = RecordingProfileMessagePresenter::returning(IDOK);

        let result = handle_manager_rename_result_with_presenter(
            invalid_submission,
            owner,
            Language::English,
            &mut presenter,
        );

        assert_eq!(result.unwrap(), None);
        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::ValidationWarning
        );
        assert_eq!(presenter.messages[0].owner, owner);
        assert_eq!(presenter.messages[0].style, MB_OK | MB_ICONWARNING);
    }

    #[test]
    fn add_validation_path_presents_the_validation_route_and_restores_commands() {
        let owner = HWND(102_usize as _);
        let mut state = AddDialogState {
            edit: HWND::default(),
            language: Language::English,
            result: None,
            interaction: Default::default(),
            resources: test_visual_resources(),
        };
        assert!(state.interaction.begin_command());
        let mut presenter = RecordingProfileMessagePresenter::returning(IDOK);
        let result = add_profile_prompt_result("   ", AddProfilePromptCommand::Submit);

        unsafe {
            handle_add_profile_prompt_result_with_presenter(
                owner,
                &mut state,
                result,
                &mut presenter,
            );
        }

        assert!(state.interaction.accepts_commands());
        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::ValidationWarning
        );
        assert_eq!(presenter.messages[0].owner, owner);
    }

    #[test]
    fn add_safe_error_path_presents_the_add_prompt_route() {
        let owner = HWND(103_usize as _);
        let mut state = AddDialogState {
            edit: HWND::default(),
            language: Language::English,
            result: None,
            interaction: Default::default(),
            resources: test_visual_resources(),
        };
        assert!(state.interaction.begin_command());
        let mut presenter = RecordingProfileMessagePresenter::returning(IDOK);

        unsafe {
            show_add_dialog_warning_with_presenter(
                owner,
                &mut state,
                LocalizationKey::UsageProfileOperationFailed,
                MB_OK | MB_ICONERROR,
                &mut presenter,
            );
        }

        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::AddPromptSafeError
        );
    }

    #[test]
    fn login_confirmation_path_presents_the_login_route_and_maps_ok() {
        let owner = HWND(104_usize as _);
        let mut presenter = RecordingProfileMessagePresenter::returning(IDOK);

        let confirmed =
            confirm_profile_login_with_presenter(owner, "Work", Language::English, &mut presenter)
                .unwrap();

        assert!(confirmed);
        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::LoginConfirmation
        );
        assert!(presenter.messages[0].message.contains("Work"));
    }

    #[test]
    fn delete_confirmation_path_presents_the_delete_route_and_maps_yes() {
        let owner = HWND(105_usize as _);
        let mut presenter = RecordingProfileMessagePresenter::returning(IDYES);

        let confirmed =
            confirm_profile_delete_with_presenter(owner, "Work", Language::English, &mut presenter)
                .unwrap();

        assert!(confirmed);
        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::DeleteConfirmation
        );
        assert!(presenter.messages[0].message.contains("Work"));
    }

    #[test]
    fn manager_safe_error_path_presents_the_manager_route() {
        let owner = HWND(106_usize as _);
        let mut presenter = RecordingProfileMessagePresenter::returning(IDCANCEL);

        show_safe_error_with_presenter(owner, Language::English, &mut presenter);

        assert_eq!(presenter.messages.len(), 1);
        assert_eq!(
            presenter.messages[0].route,
            ProfileMessageRoute::ManagerSafeError
        );
        assert_eq!(presenter.messages[0].style, MB_OK | MB_ICONERROR);
    }

    #[derive(Default)]
    struct HookCalls {
        installs: usize,
        unhooked: Vec<usize>,
    }

    struct RecordingHookBackend {
        calls: Rc<RefCell<HookCalls>>,
        install_result: Option<usize>,
    }

    impl RecordingHookBackend {
        fn succeeding(calls: Rc<RefCell<HookCalls>>, hook: usize) -> Self {
            Self {
                calls,
                install_result: Some(hook),
            }
        }

        fn failing(calls: Rc<RefCell<HookCalls>>) -> Self {
            Self {
                calls,
                install_result: None,
            }
        }
    }

    impl CenteredMessageBoxHookBackend for RecordingHookBackend {
        type Hook = usize;

        fn install(&mut self) -> Option<Self::Hook> {
            self.calls.borrow_mut().installs += 1;
            self.install_result
        }

        fn unhook(&mut self, hook: Self::Hook) {
            self.calls.borrow_mut().unhooked.push(hook);
        }
    }

    fn on_fresh_thread(test: impl FnOnce() + Send + 'static) {
        thread::spawn(test).join().unwrap();
    }

    #[test]
    fn hook_guard_installs_consumes_once_and_unhooks_on_drop() {
        on_fresh_thread(|| {
            let calls = Rc::new(RefCell::new(HookCalls::default()));
            let work_area = DialogWorkArea::new(-1600, 40, 0, 1040);
            let Some(guard) = CenteredMessageBoxHookGuard::install_with_backend(
                work_area,
                RecordingHookBackend::succeeding(Rc::clone(&calls), 41),
            ) else {
                panic!("recording hook should install");
            };

            assert_eq!(calls.borrow().installs, 1);
            assert_eq!(
                consume_centered_message_box_request(),
                Some(super::CenteredMessageBoxRequest::new(work_area))
            );
            assert_eq!(consume_centered_message_box_request(), None);

            drop(guard);

            assert_eq!(calls.borrow().unhooked, vec![41]);
            assert_eq!(consume_centered_message_box_request(), None);
        });
    }

    #[test]
    fn hook_install_failure_restores_the_outer_thread_local_request() {
        on_fresh_thread(|| {
            let outer_calls = Rc::new(RefCell::new(HookCalls::default()));
            let failed_calls = Rc::new(RefCell::new(HookCalls::default()));
            let outer_area = DialogWorkArea::new(0, 0, 1920, 1040);
            let inner_area = DialogWorkArea::new(1920, 0, 3520, 900);
            let Some(outer) = CenteredMessageBoxHookGuard::install_with_backend(
                outer_area,
                RecordingHookBackend::succeeding(Rc::clone(&outer_calls), 51),
            ) else {
                panic!("outer recording hook should install");
            };

            let failed = CenteredMessageBoxHookGuard::install_with_backend(
                inner_area,
                RecordingHookBackend::failing(Rc::clone(&failed_calls)),
            );

            assert!(failed.is_none());
            assert_eq!(failed_calls.borrow().installs, 1);
            assert!(failed_calls.borrow().unhooked.is_empty());
            assert_eq!(
                consume_centered_message_box_request(),
                Some(super::CenteredMessageBoxRequest::new(outer_area))
            );
            drop(outer);
            assert_eq!(outer_calls.borrow().unhooked, vec![51]);
        });
    }

    #[test]
    fn nested_hook_guard_drop_restores_outer_request_and_unhooks_each_hook() {
        on_fresh_thread(|| {
            let calls = Rc::new(RefCell::new(HookCalls::default()));
            let outer_area = DialogWorkArea::new(-1920, 0, 0, 1040);
            let inner_area = DialogWorkArea::new(0, -900, 1600, 0);
            let Some(outer) = CenteredMessageBoxHookGuard::install_with_backend(
                outer_area,
                RecordingHookBackend::succeeding(Rc::clone(&calls), 61),
            ) else {
                panic!("outer recording hook should install");
            };
            let Some(inner) = CenteredMessageBoxHookGuard::install_with_backend(
                inner_area,
                RecordingHookBackend::succeeding(Rc::clone(&calls), 62),
            ) else {
                panic!("inner recording hook should install");
            };

            assert_eq!(
                consume_centered_message_box_request(),
                Some(super::CenteredMessageBoxRequest::new(inner_area))
            );
            drop(inner);
            assert_eq!(
                consume_centered_message_box_request(),
                Some(super::CenteredMessageBoxRequest::new(outer_area))
            );
            drop(outer);

            assert_eq!(calls.borrow().unhooked, vec![62, 61]);
            assert_eq!(consume_centered_message_box_request(), None);
        });
    }
}
