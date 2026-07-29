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
            GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT,
            POINT, RECT, SIZE, WPARAM,
        },
        Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        Graphics::Gdi::{
            CreateFontW, CreateSolidBrush, DeleteObject, DrawFocusRect, DrawTextW, FillRect, GetDC,
            GetMonitorInfoW, GetStockObject, GetSysColor, GetSysColorBrush, GetTextExtentPoint32W,
            InvalidateRect, MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject,
            SetBkColor, SetBkMode, SetDCBrushColor, SetTextColor, BACKGROUND_MODE,
            CLIP_DEFAULT_PRECIS, CLR_INVALID, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
            COLOR_WINDOWTEXT, DC_BRUSH, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH,
            DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX, DT_RIGHT, DT_RTLREADING,
            DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_MEDIUM, FW_NORMAL, HBRUSH, HDC, HFONT, HGDIOBJ,
            MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, OUT_DEFAULT_PRECIS,
            PROOF_QUALITY, TRANSPARENT,
        },
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Controls::{
                SetWindowTheme, CDDS_PREPAINT, CDIS_DEFAULT, CDIS_DISABLED, CDIS_FOCUS, CDIS_HOT,
                CDIS_SELECTED, CDRF_SKIPDEFAULT, DRAWITEMSTRUCT, EM_SETLIMITTEXT, NMCUSTOMDRAW,
                NMCUSTOMDRAW_DRAW_STATE_FLAGS, NM_CUSTOMDRAW, ODS_FOCUS, ODS_SELECTED, ODT_LISTBOX,
                TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_RTLREADING, TTF_SUBCLASS, TTM_ADDTOOLW,
                TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
            },
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
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
                LBN_SELCHANGE, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY,
                LBS_OWNERDRAWFIXED, LB_ADDSTRING, LB_GETCURSEL, LB_SETCURSEL, LB_SETITEMHEIGHT,
                MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_YESNO, MESSAGEBOX_RESULT,
                MESSAGEBOX_STYLE, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SW_SHOW, WH_CBT, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
                WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY,
                WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_NOTIFY,
                WM_SETFONT, WM_SETTINGCHANGE, WM_THEMECHANGED, WNDCLASSW, WS_BORDER, WS_CAPTION,
                WS_CHILD, WS_EX_DLGMODALFRAME, WS_EX_LAYOUTRTL, WS_EX_NOINHERITLAYOUT,
                WS_EX_RTLREADING, WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
                WS_VSCROLL,
            },
        },
    },
};

use crate::windows::{
    design::{
        add_profile_layout, profile_manager_layout, scale_logical, DialogColor, DialogLayoutInput,
        DialogPalette, DialogTheme, LogicalRect,
    },
    theme, ProfileUsageStatus,
};
use crate::{localized_text, Language, LocalizationKey, ProfileValidationError};

use super::{
    add_profile_dialog_monitor_anchor, add_profile_prompt_result, centered_dialog_origin,
    profile_delete_confirmation, profile_dialog_button_labels, profile_dialog_keyboard_result,
    profile_login_confirmation, profile_manager_accessible_row_text,
    profile_manager_control_enabled, profile_manager_dialog_monitor_anchor,
    profile_manager_row_text, show_profile_message, AddProfilePromptCommand, AddProfilePromptState,
    CenteredMessageBoxRequest, CenteredMessageBoxRequestState, DialogMonitorAnchor,
    DialogWindowSize, DialogWorkArea, ModalCleanupAction, ModalDialogLifecycle,
    ProfileDialogAction, ProfileDialogCommand, ProfileDialogController,
    ProfileDialogKeyboardCommand, ProfileDialogKeyboardResult, ProfileManagerControl,
    ProfileManagerDialogState, ProfileManagerRowText, ProfileMessageRoute, UsageProfileView,
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

impl DialogResourceSet {
    fn has_complete_brush_set(&self) -> bool {
        self.owns_background_brush
            && self.owns_surface_brush
            && !self.background_brush.0.is_null()
            && !self.surface_brush.0.is_null()
    }
}

#[derive(Clone, Copy)]
struct DialogVisualSnapshot {
    body_font: HFONT,
    heading_font: HFONT,
    dark: bool,
}

#[derive(Clone, Copy)]
struct ProfileRowPaintResources {
    dpi: u32,
    palette: DialogPalette,
    body_font: HFONT,
}

struct StagedDialogVisualResources {
    dpi: u32,
    palette: DialogPalette,
    resources: DialogResourceSet,
}

impl StagedDialogVisualResources {
    fn snapshot(&self) -> DialogVisualSnapshot {
        DialogVisualSnapshot {
            body_font: self.resources.body_font,
            heading_font: self.resources.heading_font,
            dark: self.palette == DialogPalette::for_theme(DialogTheme::Dark),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogVisualUpdateOutcome {
    Applied,
    Coalesced,
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
    update_in_progress: bool,
    pending_update: Option<(u32, DialogTheme)>,
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
            update_in_progress: false,
            pending_update: None,
        }
    }

    /// DPI 또는 테마가 바뀐 대화상자의 새 GDI 자원을 활성 자원과 분리해 준비합니다.
    ///
    /// 반환된 staged 집합은 아직 컨트롤에 적용되거나 활성 상태에 저장되지 않습니다. 호출자는
    /// 새 글꼴을 모든 컨트롤에 먼저 적용한 뒤 `commit_staged`로 교체하고 이전 집합을 해제해야
    /// 합니다. DPI와 팔레트가 같으면 별도 할당 없이 `None`을 반환합니다.
    fn rebuild_for_dpi(
        &mut self,
        dpi: u32,
        theme: DialogTheme,
    ) -> Option<StagedDialogVisualResources> {
        let palette = DialogPalette::for_theme(theme);
        if self.dpi == dpi && self.palette == palette {
            return None;
        }
        let resources = Self::allocate(&mut *self.backend, dpi, palette);
        if !resources.has_complete_brush_set() {
            release_dialog_resource_set(&mut *self.backend, resources);
            return None;
        }
        Some(StagedDialogVisualResources {
            dpi,
            palette,
            resources,
        })
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

    fn snapshot(&self) -> DialogVisualSnapshot {
        DialogVisualSnapshot {
            body_font: self.body_font,
            heading_font: self.heading_font,
            dark: self.palette == DialogPalette::for_theme(DialogTheme::Dark),
        }
    }

    /// owner-draw 행에 필요한 값만 복사해 자원 객체의 borrow와 GDI 호출을 분리합니다.
    fn profile_row_snapshot(&self) -> ProfileRowPaintResources {
        ProfileRowPaintResources {
            dpi: self.dpi,
            palette: self.palette,
            body_font: self.body_font,
        }
    }

    /// 완전히 커밋된 현재 DPI에 대응하는 owner-draw 행 높이를 반환합니다.
    ///
    /// 중첩된 시각 자원 갱신이 더 최신 DPI를 커밋한 경우에도 최종 `self.dpi`만 사용하며,
    /// 반환값은 Win32 호출 전에 복사할 수 있는 물리 픽셀 높이입니다.
    fn profile_row_height(&self) -> i32 {
        scale_logical(crate::windows::design::ROW_HEIGHT, self.dpi).max(1)
    }

    fn detach_active(&mut self) -> DialogResourceSet {
        let resources = DialogResourceSet {
            body_font: self.body_font,
            heading_font: self.heading_font,
            background_brush: self.background_brush,
            surface_brush: self.surface_brush,
            owns_body_font: self.owns_body_font,
            owns_heading_font: self.owns_heading_font,
            owns_background_brush: self.owns_background_brush,
            owns_surface_brush: self.owns_surface_brush,
        };
        self.owns_body_font = false;
        self.owns_heading_font = false;
        self.owns_background_brush = false;
        self.owns_surface_brush = false;
        resources
    }

    fn commit_staged(&mut self, staged: StagedDialogVisualResources) -> DialogResourceSet {
        let previous = self.detach_active();
        self.dpi = staged.dpi;
        self.palette = staged.palette;
        self.body_font = staged.resources.body_font;
        self.heading_font = staged.resources.heading_font;
        self.background_brush = staged.resources.background_brush;
        self.surface_brush = staged.resources.surface_brush;
        self.owns_body_font = staged.resources.owns_body_font;
        self.owns_heading_font = staged.resources.owns_heading_font;
        self.owns_background_brush = staged.resources.owns_background_brush;
        self.owns_surface_brush = staged.resources.owns_surface_brush;
        previous
    }

    fn release_set(&mut self, resources: DialogResourceSet) {
        release_dialog_resource_set(&mut *self.backend, resources);
    }
}

/// 소유한 대화상자 GDI 객체만 정확히 한 번 해제합니다.
///
/// null 핸들과 빌린 stock 객체는 건너뛰며, 부분적으로 생성된 staged 집합을 폐기할 때도
/// 생성에 성공한 객체만 정리합니다.
fn release_dialog_resource_set(
    backend: &mut dyn DialogResourceBackend,
    resources: DialogResourceSet,
) {
    for (object, owned) in [
        (HGDIOBJ(resources.body_font.0), resources.owns_body_font),
        (
            HGDIOBJ(resources.heading_font.0),
            resources.owns_heading_font,
        ),
        (
            HGDIOBJ(resources.background_brush.0),
            resources.owns_background_brush,
        ),
        (
            HGDIOBJ(resources.surface_brush.0),
            resources.owns_surface_brush,
        ),
    ] {
        if owned && !object.0.is_null() {
            backend.delete_object(object);
        }
    }
}

impl Drop for DialogVisualResources {
    fn drop(&mut self) {
        let active = self.detach_active();
        self.release_set(active);
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

#[derive(Clone, Copy)]
struct DialogChildVisualContext {
    body_font: HFONT,
    dark: bool,
}

/// 대화상자 시각 자원 교체를 staged 적용 순서로 실행하고 중첩 요청은 마지막 값으로 합칩니다.
///
/// 새 집합을 만든 뒤 Copy 스냅샷만 `apply`에 전달하고, 적용이 반환된 다음 활성 집합과 교체해
/// 이전 소유 자원을 해제합니다. 적용 중 같은 포인터로 다시 호출되면 자원을 바꾸지 않고 최신
/// DPI/테마 요청만 기록해 바깥 호출이 안전한 시점에 이어서 처리합니다.
///
/// # Safety
///
/// `resources`는 호출 전체와 모든 중첩 호출 동안 같은 UI 스레드에서 살아 있는 단일
/// `DialogVisualResources`를 가리켜야 합니다. `apply`는 전달된 Copy 값만 사용해야 하며 자원
/// 객체의 Rust 참조를 보관하면 안 됩니다.
unsafe fn update_dialog_visual_resources<F>(
    resources: *mut DialogVisualResources,
    mut dpi: u32,
    mut theme: DialogTheme,
    mut apply: F,
) -> DialogVisualUpdateOutcome
where
    F: FnMut(DialogVisualSnapshot),
{
    {
        let resources = &mut *resources;
        if resources.update_in_progress {
            resources.pending_update = Some((dpi, theme));
            return DialogVisualUpdateOutcome::Coalesced;
        }
        resources.update_in_progress = true;
    }

    loop {
        let staged = (&mut *resources).rebuild_for_dpi(dpi, theme);
        let visual = staged
            .as_ref()
            .map(StagedDialogVisualResources::snapshot)
            .unwrap_or_else(|| (&*resources).snapshot());

        apply(visual);

        if let Some(staged) = staged {
            let previous = (&mut *resources).commit_staged(staged);
            (&mut *resources).release_set(previous);
        }

        let pending = {
            let resources = &mut *resources;
            match resources.pending_update.take() {
                Some(pending) => Some(pending),
                None => {
                    resources.update_in_progress = false;
                    None
                }
            }
        };
        let Some((pending_dpi, pending_theme)) = pending else {
            break;
        };
        dpi = pending_dpi;
        theme = pending_theme;
    }

    DialogVisualUpdateOutcome::Applied
}

/// 대화상자와 모든 기본 자식 컨트롤에 현재 글꼴 및 Windows 컨트롤 테마를 적용합니다.
///
/// `dialog`과 열거되는 자식 HWND는 호출 동안 유효해야 하며, 스냅샷의 글꼴은 호출자가 staged
/// 또는 활성 집합으로 계속 소유해야 합니다. 함수는 Copy 값만 사용하므로 동기 Win32 호출 중
/// 원본 자원 객체의 Rust 참조를 유지하지 않습니다. DWM 또는 개별 컨트롤 테마 적용 실패는
/// 지원되지 않는 Windows 버전의 시각적 폴백으로 취급하고 모달 생성을 중단하지 않습니다.
unsafe fn apply_dialog_visuals(dialog: HWND, visual: DialogVisualSnapshot) {
    let dark_attribute = i32::from(visual.dark);
    let _ = DwmSetWindowAttribute(
        dialog,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        (&dark_attribute as *const i32).cast(),
        std::mem::size_of_val(&dark_attribute) as u32,
    );
    let _ = SendMessageW(
        dialog,
        WM_SETFONT,
        Some(WPARAM(visual.heading_font.0 as usize)),
        Some(LPARAM(1)),
    );
    let context = DialogChildVisualContext {
        body_font: visual.body_font,
        dark: visual.dark,
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
    let context = *(lparam.0 as *const DialogChildVisualContext);
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
    rebuild_dialog_visuals_for_dpi(dialog, resources, GetDpiForWindow(dialog).max(96));
}

/// 지정 DPI의 staged 자원을 모든 자식에 적용한 뒤 커밋하고 행 높이를 최종 DPI에 맞춥니다.
///
/// `resources`는 `dialog`보다 오래 살아 있는 같은 UI 스레드의 자원 집합이어야 합니다. 중첩된
/// DPI·테마 갱신은 기존 coalescing 경계에서 합쳐지며, 행 높이와 invalidate는 최종 커밋된 DPI만
/// 사용합니다. 지원되지 않는 DWM·테마 표면 실패는 대화상자 동작을 중단하지 않습니다.
unsafe fn rebuild_dialog_visuals_for_dpi(
    dialog: HWND,
    resources: *mut DialogVisualResources,
    requested_dpi: u32,
) {
    let outcome = update_dialog_visual_resources(
        resources,
        requested_dpi.max(96),
        current_dialog_theme(),
        |visual| {
            // SAFETY: Copy 스냅샷만 전달하며 자원 객체의 Rust 참조는 외부 호출 동안 존재하지 않습니다.
            unsafe { apply_dialog_visuals(dialog, visual) };
        },
    );
    if outcome == DialogVisualUpdateOutcome::Applied {
        let row_height = {
            // SAFETY: 갱신 완료 뒤 committed DPI만 Copy하며 외부 호출 전에 자원 borrow를 끝냅니다.
            (&*resources).profile_row_height()
        };
        if let Ok(list) = GetDlgItem(Some(dialog), PROFILE_LIST_ID) {
            let _ = SendMessageW(
                list,
                LB_SETITEMHEIGHT,
                Some(WPARAM(0)),
                Some(LPARAM(row_height as isize)),
            );
        }
        let _ = InvalidateRect(Some(dialog), None, true);
    }
}

const PREFERRED_DIALOG_CLIENT_WIDTH: i32 = 620;

fn dialog_window_style() -> WINDOW_STYLE {
    WS_POPUP | WS_CAPTION | WS_SYSMENU
}

fn dialog_root_ex_style(language: Language) -> WINDOW_EX_STYLE {
    let mut style = WS_EX_DLGMODALFRAME;
    if language == Language::Arabic {
        style = style | WS_EX_LAYOUTRTL | WS_EX_NOINHERITLAYOUT;
    }
    style
}

fn dialog_child_text_ex_style(language: Language) -> WINDOW_EX_STYLE {
    if language == Language::Arabic {
        WS_EX_RTLREADING
    } else {
        WINDOW_EX_STYLE::default()
    }
}

fn physical_to_logical_floor(value: i32, dpi: u32) -> i32 {
    let dpi = dpi.max(96);
    ((i64::from(value.max(0)) * 96) / i64::from(dpi)) as i32
}

/// 현재 대화상자 본문 글꼴로 한 줄 문자열의 실제 물리 픽셀 너비를 측정합니다.
///
/// `hdc`와 `font`는 호출 동안 유효해야 하며, 함수는 선택했던 GDI 객체를 반환 전에 복원합니다.
/// `text`는 줄바꿈 없는 지역화 버튼 문구여야 합니다. 측정 실패는 운영체제 오류로 반환합니다.
unsafe fn measure_text_width(hdc: HDC, font: HFONT, text: &str) -> io::Result<i32> {
    if hdc.0.is_null() || font.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "dialog text measurement requires a live DC and font",
        ));
    }
    if text.is_empty() {
        return Ok(0);
    }

    let previous_font = SelectObject(hdc, HGDIOBJ(font.0));
    if previous_font.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    let text = text.encode_utf16().collect::<Vec<_>>();
    let mut size = SIZE::default();
    let measured = GetTextExtentPoint32W(hdc, &text, &mut size).as_bool();
    let _ = SelectObject(hdc, previous_font);
    if measured {
        Ok(size.cx.max(0))
    } else {
        Err(io::Error::last_os_error())
    }
}

/// 활성 본문 글꼴로 공용 버튼 문구 여섯 개를 실제 표시 순서대로 측정합니다.
///
/// 창 DC는 성공·실패와 관계없이 반환하며 민감한 데이터나 외부 I/O를 사용하지 않습니다.
unsafe fn measure_profile_dialog_buttons(
    dialog: HWND,
    font: HFONT,
    language: Language,
) -> io::Result<[i32; 6]> {
    let dc = GetDC(Some(dialog));
    if dc.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    let labels = profile_dialog_button_labels(language);
    let result = (|| {
        let mut widths = [0; 6];
        for (index, label) in labels.iter().enumerate() {
            widths[index] = measure_text_width(dc, font, label)?;
        }
        Ok(widths)
    })();
    let _ = ReleaseDC(Some(dialog), dc);
    result
}

unsafe fn adjusted_dialog_outer_size(
    client: LogicalRect,
    dpi: u32,
    language: Language,
) -> io::Result<DialogWindowSize> {
    let mut outer = RECT {
        left: 0,
        top: 0,
        right: client.width(),
        bottom: client.height(),
    };
    AdjustWindowRectExForDpi(
        &mut outer,
        dialog_window_style(),
        false,
        dialog_root_ex_style(language),
        dpi.max(96),
    )
    .map_err(win_error)?;
    Ok(DialogWindowSize::new(
        outer.right - outer.left,
        outer.bottom - outer.top,
    ))
}

unsafe fn requested_client_width_for_work_area(
    work_area: Option<DialogWorkArea>,
    dpi: u32,
    language: Language,
) -> i32 {
    let Some(work_area) = work_area else {
        return PREFERRED_DIALOG_CLIENT_WIDTH;
    };
    let work_width = (i64::from(work_area.right) - i64::from(work_area.left))
        .clamp(0, i64::from(i32::MAX)) as i32;
    let empty_client = LogicalRect::default();
    let frame_width = adjusted_dialog_outer_size(empty_client, dpi, language)
        .map(|outer| outer.width.max(0))
        .unwrap_or_default();
    physical_to_logical_floor((work_width - frame_width).max(1), dpi)
        .clamp(1, PREFERRED_DIALOG_CLIENT_WIDTH)
}

unsafe fn maximum_client_height_for_work_area(
    work_area: DialogWorkArea,
    dpi: u32,
    language: Language,
) -> i32 {
    let work_height = (i64::from(work_area.bottom) - i64::from(work_area.top))
        .clamp(0, i64::from(i32::MAX)) as i32;
    let frame_height = adjusted_dialog_outer_size(LogicalRect::default(), dpi, language)
        .map(|outer| outer.height.max(0))
        .unwrap_or_default();
    (work_height - frame_height).max(0)
}

fn translate_rect_y(rect: LogicalRect, offset: i32) -> LogicalRect {
    LogicalRect::new(
        rect.left,
        rect.top + offset,
        rect.right,
        rect.bottom + offset,
    )
}

/// 관리자의 고정 높이 컨트롤을 보존하면서 목록 viewport만 완전한 행 단위로 줄입니다.
///
/// `maximum_client_height`에 1개 행과 나머지 컨트롤이 들어갈 수 있으면 1~3개 완전한 행만
/// 선택합니다. 그보다 작은 작업 영역에서는 컨트롤 높이를 훼손하지 않도록 1개 행의 최소 레이아웃을
/// 반환하고, 최종 외곽 제한 단계가 보이는 작업 영역에 창 자체를 제한합니다.
fn fit_manager_layout_to_client_height(
    mut layout: crate::windows::design::ProfileManagerLayout,
    maximum_client_height: i32,
    dpi: u32,
) -> crate::windows::design::ProfileManagerLayout {
    let row_height = scale_logical(crate::windows::design::ROW_HEIGHT, dpi).max(1);
    let original_rows = (layout.list.height() / row_height).max(1);
    let fixed_height = layout.client.height() - layout.list.height();
    let fitting_rows =
        ((maximum_client_height - fixed_height) / row_height).clamp(1, original_rows);
    let new_list_height = fitting_rows * row_height;
    let reduction = layout.list.height() - new_list_height;
    if reduction <= 0 {
        return layout;
    }

    layout.list.bottom -= reduction;
    layout.selection_edge.bottom = layout.list.bottom;
    for rect in [
        &mut layout.add_control,
        &mut layout.name_label,
        &mut layout.name_edit,
    ] {
        *rect = translate_rect_y(*rect, -reduction);
    }
    for rect in &mut layout.action_buttons {
        *rect = translate_rect_y(*rect, -reduction);
    }
    layout.client.bottom -= reduction;
    layout.content.bottom -= reduction;
    layout
}

/// 요청 외곽 크기를 목적지 작업 영역의 양 축에 제한하고 현재 좌상단을 보이는 범위로 옮깁니다.
///
/// 음수 좌표 모니터와 작업 영역보다 큰 요청을 지원합니다. 반환 RECT는 작업 영역을 넘지 않으며,
/// 폭·높이는 가능한 범위에서 요청값을 유지합니다.
fn bound_dialog_outer_rect(
    current: RECT,
    desired: DialogWindowSize,
    work_area: DialogWorkArea,
) -> RECT {
    let work_width = (i64::from(work_area.right) - i64::from(work_area.left))
        .clamp(0, i64::from(i32::MAX)) as i32;
    let work_height = (i64::from(work_area.bottom) - i64::from(work_area.top))
        .clamp(0, i64::from(i32::MAX)) as i32;
    let width = desired.width.clamp(0, work_width);
    let height = desired.height.clamp(0, work_height);
    let maximum_left = work_area.right - width;
    let maximum_top = work_area.bottom - height;
    let left = current.left.clamp(work_area.left, maximum_left);
    let top = current.top.clamp(work_area.top, maximum_top);
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

unsafe fn resize_dialog_for_client(
    dialog: HWND,
    client: LogicalRect,
    dpi: u32,
    language: Language,
    work_area: Option<DialogWorkArea>,
) -> io::Result<()> {
    let outer = adjusted_dialog_outer_size(client, dpi, language)?;
    let mut current = RECT::default();
    let has_current = GetWindowRect(dialog, &mut current).is_ok();
    let bounded = work_area.map(|work_area| {
        if !has_current {
            current.left = work_area.left;
            current.top = work_area.top;
        }
        bound_dialog_outer_rect(current, outer, work_area)
    });
    let (x, y, width, height, flags) = if let Some(rect) = bounded {
        (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    } else {
        (
            0,
            0,
            outer.width,
            outer.height,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
        )
    };
    SetWindowPos(dialog, None, x, y, width, height, flags).map_err(win_error)
}

/// 순수 레이아웃의 물리적 사각형을 부모 창의 `SetWindowPos` 좌표계로 변환합니다.
///
/// `mirrored_parent`가 참이면 `WS_EX_LAYOUTRTL` 부모가 다시 좌우 반전할 것을 상쇄해 최종 화면
/// 사각형이 입력 `rect`와 정확히 같아지게 합니다. 세로 좌표와 크기는 변경하지 않습니다.
fn dialog_child_setpos_rect(
    rect: LogicalRect,
    parent_client_width: i32,
    mirrored_parent: bool,
) -> LogicalRect {
    if mirrored_parent {
        LogicalRect::new(
            parent_client_width - rect.right,
            rect.top,
            parent_client_width - rect.left,
            rect.bottom,
        )
    } else {
        rect
    }
}

unsafe fn move_dialog_control(
    control: HWND,
    desired_physical_rect: LogicalRect,
    parent_client_width: i32,
    mirrored_parent: bool,
) -> io::Result<()> {
    let rect =
        dialog_child_setpos_rect(desired_physical_rect, parent_client_width, mirrored_parent);
    SetWindowPos(
        control,
        None,
        rect.left,
        rect.top,
        rect.width(),
        rect.height(),
        SWP_NOACTIVATE | SWP_NOZORDER,
    )
    .map_err(win_error)
}

/// 커밋된 글꼴과 DPI로 관리자 버튼을 다시 측정하고 모든 기존 자식 HWND를 이동합니다.
///
/// `state`는 같은 관리자 창의 살아 있는 상태를 가리켜야 합니다. Win32 호출 전에 필요한 핸들과
/// Copy 값만 추출하므로 `SetWindowPos`가 메시지를 재진입시켜도 Rust 참조를 유지하지 않습니다.
unsafe fn relayout_manager_dialog(
    dialog: HWND,
    state: *mut DialogState,
    requested_client_width: i32,
    work_area: Option<DialogWorkArea>,
) -> io::Result<()> {
    let (language, dpi, body_font, list, name_label, edit) = {
        let state = &*state;
        (
            state.language,
            state.resources.dpi,
            state.resources.body_font,
            state.list,
            state.name_label,
            state.edit,
        )
    };
    let measured = measure_profile_dialog_buttons(dialog, body_font, language)?;
    let mut layout = profile_manager_layout(DialogLayoutInput::new(
        requested_client_width,
        dpi,
        language == Language::Arabic,
        measured[..4].try_into().expect("four manager widths"),
    ));
    if let Some(work_area) = work_area {
        layout = fit_manager_layout_to_client_height(
            layout,
            maximum_client_height_for_work_area(work_area, dpi, language),
            dpi,
        );
    }
    resize_dialog_for_client(dialog, layout.client, dpi, language, work_area)?;
    let rtl = language == Language::Arabic;
    let client_width = layout.client.width();

    move_dialog_control(list, layout.list, client_width, rtl)?;
    if let Ok(add) = GetDlgItem(Some(dialog), OPEN_ADD_ID) {
        move_dialog_control(add, layout.add_control, client_width, rtl)?;
    }
    move_dialog_control(name_label, layout.name_label, client_width, rtl)?;
    move_dialog_control(edit, layout.name_edit, client_width, rtl)?;
    for (control, rect) in [
        (RENAME_ID, layout.action_buttons[0]),
        (LOGIN_ID, layout.action_buttons[1]),
        (LOGOUT_ID, layout.action_buttons[2]),
        (DELETE_ID, layout.action_buttons[3]),
    ] {
        if let Ok(control) = GetDlgItem(Some(dialog), control) {
            move_dialog_control(control, rect, client_width, rtl)?;
        }
    }
    let _ = InvalidateRect(Some(dialog), None, true);
    Ok(())
}

/// 커밋된 글꼴과 DPI로 추가 대화상자의 두 작업을 측정하고 기존 자식 HWND를 이동합니다.
///
/// `state`에서 Copy 값만 읽은 뒤 크기 변경과 자식 이동을 수행해 DPI·테마 메시지 재진입 시 가변
/// 참조가 겹치지 않습니다. 버튼은 항상 한 줄 문구의 실제 측정 너비와 공통 여백을 보존합니다.
unsafe fn relayout_add_dialog(
    dialog: HWND,
    state: *mut AddDialogState,
    requested_client_width: i32,
    work_area: Option<DialogWorkArea>,
) -> io::Result<()> {
    let (language, dpi, body_font, name_label, edit) = {
        let state = &*state;
        (
            state.language,
            state.resources.dpi,
            state.resources.body_font,
            state.name_label,
            state.edit,
        )
    };
    let measured = measure_profile_dialog_buttons(dialog, body_font, language)?;
    let layout = add_profile_layout(DialogLayoutInput::new(
        requested_client_width,
        dpi,
        language == Language::Arabic,
        [measured[4], measured[5], 0, 0],
    ));
    resize_dialog_for_client(dialog, layout.client, dpi, language, work_area)?;
    let rtl = language == Language::Arabic;
    let client_width = layout.client.width();

    move_dialog_control(name_label, layout.name_label, client_width, rtl)?;
    move_dialog_control(edit, layout.name_edit, client_width, rtl)?;
    for (control, rect) in [
        (IDOK.0, layout.action_buttons[0]),
        (IDCANCEL.0, layout.action_buttons[1]),
    ] {
        if let Ok(control) = GetDlgItem(Some(dialog), control) {
            move_dialog_control(control, rect, client_width, rtl)?;
        }
    }
    let _ = InvalidateRect(Some(dialog), None, true);
    Ok(())
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
) -> bool {
    let mut client = RECT::default();
    if GetClientRect(dialog, &mut client).is_err() {
        return false;
    }
    erase_dialog_background_with(resources, |brush| {
        if FillRect(HDC(wparam.0 as *mut c_void), &client, brush) == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
}

/// 배경 brush가 유효하고 실제 채우기가 성공한 경우에만 erase 메시지를 처리한 것으로 봅니다.
///
/// 초기 brush 할당 실패 또는 `FillRect` 실패에서는 `false`를 반환하여 호출자가 Windows 기본
/// 처리로 넘길 수 있게 합니다. `fill`에는 소유권을 이전하지 않은 활성 brush만 전달합니다.
fn erase_dialog_background_with<F>(resources: &DialogVisualResources, fill: F) -> bool
where
    F: FnOnce(HBRUSH) -> io::Result<()>,
{
    let brush = resources.background_brush;
    !brush.0.is_null() && fill(brush).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryButtonSurface {
    Normal,
    Hot,
    Pressed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryButtonCue {
    None,
    Focus,
    DefaultBorder,
    DefaultBorderAndFocus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrimaryButtonPaintState {
    surface: PrimaryButtonSurface,
    cue: PrimaryButtonCue,
}

/// 네이티브 custom-draw 플래그를 기본 버튼의 의미 있는 시각 상태로 변환합니다.
///
/// 비활성 버튼은 `None`으로 네이티브 중립 렌더링을 유지합니다. 기본 버튼 테두리와 일반 포커스
/// 사각형을 별도로 구분하며 두 플래그가 함께 있으면 두 cue를 모두 보존합니다.
fn primary_button_paint_state(
    item_state: NMCUSTOMDRAW_DRAW_STATE_FLAGS,
    enabled: bool,
) -> Option<PrimaryButtonPaintState> {
    if !enabled || item_state.contains(CDIS_DISABLED) {
        return None;
    }
    let surface = if item_state.contains(CDIS_SELECTED) {
        PrimaryButtonSurface::Pressed
    } else if item_state.contains(CDIS_HOT) {
        PrimaryButtonSurface::Hot
    } else {
        PrimaryButtonSurface::Normal
    };
    let cue = match (
        item_state.contains(CDIS_DEFAULT),
        item_state.contains(CDIS_FOCUS),
    ) {
        (true, true) => PrimaryButtonCue::DefaultBorderAndFocus,
        (true, false) => PrimaryButtonCue::DefaultBorder,
        (false, true) => PrimaryButtonCue::Focus,
        (false, false) => PrimaryButtonCue::None,
    };
    Some(PrimaryButtonPaintState { surface, cue })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryButtonPaintStage {
    DefaultBorder,
    Border,
    Surface,
}

/// 기본 버튼 fill에서 DC brush 색상 적용·복원을 관찰하는 Win32 하위 경계입니다.
///
/// `set_brush_color`는 실제 `SetDCBrushColor`처럼 이전 색 또는 `CLR_INVALID`를 반환합니다.
/// `fill_rect`는 이미 선택된 stock DC brush로 지정 범위를 채우며 DC 색상을 변경하지 않습니다.
trait PrimaryButtonBrushBackend {
    fn set_brush_color(&mut self, color: COLORREF) -> COLORREF;
    fn fill_rect(&mut self, rect: &RECT) -> io::Result<()>;
}

/// 기본 버튼 사각형을 채우고 DC brush 색상을 성공·실패 모든 가능한 경로에서 복원합니다.
///
/// 색상 적용이 실패하면 채우기를 수행하지 않습니다. 적용에 성공하면 `FillRect` 실패 여부와
/// 관계없이 이전 색 복원을 시도하며, 채우기 또는 복원 중 하나라도 실패하면 오류를 반환해 호출자가
/// 네이티브 기본 그리기로 대체하게 합니다.
fn fill_primary_button_rect<B: PrimaryButtonBrushBackend>(
    backend: &mut B,
    rect: &RECT,
    color: u32,
) -> io::Result<()> {
    let previous = backend.set_brush_color(COLORREF(color));
    if previous.0 == CLR_INVALID {
        return Err(io::Error::other(
            "primary button brush color could not be applied",
        ));
    }

    let fill_result = backend.fill_rect(rect);
    let restored = backend.set_brush_color(previous);
    if restored.0 == CLR_INVALID {
        return Err(io::Error::other(
            "primary button brush color could not be restored",
        ));
    }
    fill_result
}

struct WindowsPrimaryButtonBrushBackend {
    dc: HDC,
    brush: HBRUSH,
}

impl PrimaryButtonBrushBackend for WindowsPrimaryButtonBrushBackend {
    fn set_brush_color(&mut self, color: COLORREF) -> COLORREF {
        // SAFETY: dc는 현재 NM_CUSTOMDRAW notification 동안 유효합니다.
        unsafe { SetDCBrushColor(self.dc, color) }
    }

    fn fill_rect(&mut self, rect: &RECT) -> io::Result<()> {
        // SAFETY: dc, stock brush, RECT는 현재 동기 custom paint 호출 동안 유효합니다.
        if unsafe { FillRect(self.dc, rect, self.brush) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// 기본 버튼 paint 단계와 DC 상태 저장·복원을 분리하는 GDI 경계입니다.
///
/// 실제 구현은 현재 notification HDC만 사용하며 테스트 구현은 각 필수 단계를 결정적으로 실패시킬
/// 수 있습니다. 성공한 선택·색상 변경은 호출자가 반드시 대응 restore 메서드로 복원합니다.
trait PrimaryButtonPaintBackend {
    fn fill_rect(
        &mut self,
        stage: PrimaryButtonPaintStage,
        rect: &RECT,
        color: u32,
    ) -> io::Result<()>;
    fn select_font(&mut self, font: HFONT) -> io::Result<HGDIOBJ>;
    fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE>;
    fn set_text_color(&mut self, color: COLORREF) -> io::Result<COLORREF>;
    fn draw_text(&mut self, text: &str, rect: &mut RECT, rtl: bool) -> io::Result<()>;
    fn draw_focus(&mut self, rect: &RECT) -> io::Result<()>;
    fn restore_font(&mut self, font: HGDIOBJ) -> io::Result<()>;
    fn restore_background(&mut self, mode: BACKGROUND_MODE) -> io::Result<()>;
    fn restore_text_color(&mut self, color: COLORREF) -> io::Result<()>;
}

struct WindowsPrimaryButtonPaintBackend {
    dc: HDC,
}

impl PrimaryButtonPaintBackend for WindowsPrimaryButtonPaintBackend {
    fn fill_rect(
        &mut self,
        _stage: PrimaryButtonPaintStage,
        rect: &RECT,
        color: u32,
    ) -> io::Result<()> {
        // SAFETY: DC_BRUSH는 프로세스가 소유하지 않는 stock 객체이며 삭제하지 않습니다.
        let brush = unsafe { HBRUSH(GetStockObject(DC_BRUSH).0) };
        if brush.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut backend = WindowsPrimaryButtonBrushBackend { dc: self.dc, brush };
        fill_primary_button_rect(&mut backend, rect, color)
    }

    fn select_font(&mut self, font: HFONT) -> io::Result<HGDIOBJ> {
        // SAFETY: font는 커밋된 대화상자 자원이며 notification이 끝날 때까지 살아 있습니다.
        let previous = unsafe { SelectObject(self.dc, HGDIOBJ(font.0)) };
        if previous.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(previous)
        }
    }

    fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE> {
        // SAFETY: dc는 현재 notification 동안 유효합니다.
        let previous = unsafe { SetBkMode(self.dc, TRANSPARENT) };
        if previous == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(BACKGROUND_MODE(previous as u32))
        }
    }

    fn set_text_color(&mut self, color: COLORREF) -> io::Result<COLORREF> {
        // SAFETY: dc는 현재 notification 동안 유효합니다.
        let previous = unsafe { SetTextColor(self.dc, color) };
        if previous.0 == CLR_INVALID {
            Err(io::Error::last_os_error())
        } else {
            Ok(previous)
        }
    }

    fn draw_text(&mut self, text: &str, rect: &mut RECT, rtl: bool) -> io::Result<()> {
        let mut text = text.encode_utf16().collect::<Vec<_>>();
        let mut format = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
        if rtl {
            format |= DT_RTLREADING;
        }
        // SAFETY: UTF-16 버퍼와 RECT는 동기 DrawTextW 호출 동안 살아 있습니다.
        if unsafe { DrawTextW(self.dc, &mut text, rect, format) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn draw_focus(&mut self, rect: &RECT) -> io::Result<()> {
        // SAFETY: dc와 RECT는 현재 notification 범위에서 유효합니다.
        if unsafe { DrawFocusRect(self.dc, rect) }.as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn restore_font(&mut self, font: HGDIOBJ) -> io::Result<()> {
        // SAFETY: font는 같은 backend가 이번 paint에서 저장한 이전 선택 객체입니다.
        if unsafe { SelectObject(self.dc, font) }.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn restore_background(&mut self, mode: BACKGROUND_MODE) -> io::Result<()> {
        // SAFETY: mode는 같은 backend가 이번 paint에서 저장한 이전 배경 모드입니다.
        if unsafe { SetBkMode(self.dc, mode) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn restore_text_color(&mut self, color: COLORREF) -> io::Result<()> {
        // SAFETY: color는 같은 backend가 이번 paint에서 저장한 이전 텍스트 색입니다.
        if unsafe { SetTextColor(self.dc, color) }.0 == CLR_INVALID {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// 기본 버튼 custom paint의 모든 필수 단계를 실행하고 완전 성공 여부만 반환합니다.
///
/// 실패 지점과 관계없이 이미 획득한 font/background/text DC 상태를 역순으로 복원합니다. `false`는
/// 호출자가 `CDRF_SKIPDEFAULT`를 반환하지 않고 네이티브 기본 렌더링을 사용해야 함을 뜻합니다.
fn paint_primary_button<B: PrimaryButtonPaintBackend>(
    backend: &mut B,
    rect: RECT,
    label: &str,
    font: HFONT,
    palette: DialogPalette,
    rtl: bool,
    state: PrimaryButtonPaintState,
) -> bool {
    if rect.right <= rect.left || rect.bottom <= rect.top || label.is_empty() || font.0.is_null() {
        return false;
    }
    let overlay = match state.surface {
        PrimaryButtonSurface::Pressed => Some(palette.pressed),
        PrimaryButtonSurface::Hot => Some(palette.hover),
        PrimaryButtonSurface::Normal => None,
    };
    let surface = overlay
        .map(|overlay| composite_dialog_color(overlay, palette.healthy))
        .unwrap_or(palette.healthy.colorref);

    let mut border_rect = rect;
    let mut previous_font = None;
    let mut previous_background = None;
    let mut previous_text = None;
    let painted = (|| -> io::Result<()> {
        if matches!(
            state.cue,
            PrimaryButtonCue::DefaultBorder | PrimaryButtonCue::DefaultBorderAndFocus
        ) {
            backend.fill_rect(
                PrimaryButtonPaintStage::DefaultBorder,
                &border_rect,
                palette.focus.colorref,
            )?;
            border_rect.left += 2;
            border_rect.top += 2;
            border_rect.right -= 2;
            border_rect.bottom -= 2;
        }
        backend.fill_rect(
            PrimaryButtonPaintStage::Border,
            &border_rect,
            palette.border.colorref,
        )?;
        let mut inner = RECT {
            left: border_rect.left + 1,
            top: border_rect.top + 1,
            right: border_rect.right - 1,
            bottom: border_rect.bottom - 1,
        };
        backend.fill_rect(PrimaryButtonPaintStage::Surface, &inner, surface)?;
        previous_font = Some(backend.select_font(font)?);
        previous_background = Some(backend.set_transparent_background()?);
        previous_text = Some(backend.set_text_color(COLORREF(palette.primary_text.colorref))?);
        backend.draw_text(label, &mut inner, rtl)?;
        if matches!(
            state.cue,
            PrimaryButtonCue::Focus | PrimaryButtonCue::DefaultBorderAndFocus
        ) {
            let focus = RECT {
                left: rect.left + 4,
                top: rect.top + 4,
                right: rect.right - 4,
                bottom: rect.bottom - 4,
            };
            backend.draw_focus(&focus)?;
        }
        Ok(())
    })();

    let mut restored = true;
    if let Some(color) = previous_text {
        restored &= backend.restore_text_color(color).is_ok();
    }
    if let Some(mode) = previous_background {
        restored &= backend.restore_background(mode).is_ok();
    }
    if let Some(font) = previous_font {
        restored &= backend.restore_font(font).is_ok();
    }
    painted.is_ok() && restored
}

/// 활성 기본 작업 버튼의 `NM_CUSTOMDRAW`를 건강 상태 녹색으로 한 줄 그립니다.
///
/// 알림이 대상 버튼·prepaint 단계가 아니거나 버튼이 비활성 상태이면 기본 네이티브 렌더링을
/// 유지합니다. `lparam`은 현재 `WM_NOTIFY`가 제공한 `NMCUSTOMDRAW`여야 하며, GDI 선택 객체와
/// 색상·배경 모드는 반환 전에 복원합니다. 지원되지 않는 custom draw는 비치명적으로 무시됩니다.
unsafe fn draw_primary_button_notification(
    lparam: LPARAM,
    expected_id: i32,
    label: &str,
    font: HFONT,
    palette: DialogPalette,
    rtl: bool,
) -> LRESULT {
    if lparam.0 == 0 {
        return LRESULT(0);
    }
    let draw = &*(lparam.0 as *const NMCUSTOMDRAW);
    if draw.hdr.code != NM_CUSTOMDRAW
        || draw.hdr.idFrom != expected_id as usize
        || draw.dwDrawStage != CDDS_PREPAINT
        || draw.hdc.0.is_null()
    {
        return LRESULT(0);
    }
    let Some(state) = primary_button_paint_state(
        draw.uItemState,
        IsWindowEnabled(draw.hdr.hwndFrom).as_bool(),
    ) else {
        return LRESULT(0);
    };
    let mut backend = WindowsPrimaryButtonPaintBackend { dc: draw.hdc };
    if paint_primary_button(&mut backend, draw.rc, label, font, palette, rtl, state) {
        LRESULT(CDRF_SKIPDEFAULT as isize)
    } else {
        LRESULT(0)
    }
}

struct DialogState {
    controller: ProfileDialogController,
    interaction: ProfileManagerDialogState,
    language: Language,
    list: HWND,
    name_label: HWND,
    edit: HWND,
    add_tooltip: HWND,
    add_tooltip_text: Vec<u16>,
    resources: DialogVisualResources,
}

struct AddDialogState {
    name_label: HWND,
    edit: HWND,
    language: Language,
    result: Option<ProfileDialogAction>,
    interaction: AddProfilePromptState,
    resources: DialogVisualResources,
}

/// 프로필 행 배경에 적용할 디자인 팔레트 역할입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileRowSurfaceRole {
    /// 선택되지 않은 목록 행의 기본 표면입니다.
    Neutral,
    /// 네이티브 목록 선택이 적용된 올라온 표면입니다.
    Selected,
}

/// owner-draw 프로필 행에 전달할 순수 시각 상태입니다.
///
/// 색상 자체나 GDI 핸들을 보관하지 않으며, 네이티브 선택·포커스와 표시 모델의 typed usage 및
/// 텍스트 표식 여부만 담습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProfileRowVisualState {
    surface: ProfileRowSurfaceRole,
    progress: Option<(u8, ProfileUsageStatus)>,
    system_marker: bool,
    current_marker: bool,
    focused: bool,
}

/// 프로필 표시 모델과 네이티브 항목 상태를 owner-draw 결정으로 변환합니다.
///
/// 로그인 필요 상태 또는 percent/status 쌍이 불완전한 상태에는 진행 표시를 만들지 않습니다.
/// 유효한 percent는 손상된 외부 입력에도 행 너비를 넘지 않도록 100으로 제한합니다. 함수는
/// 문자열을 파싱하거나 전역 `UsageLevel` 정책을 변경하지 않습니다.
fn profile_row_visual_state(
    profile: &UsageProfileView,
    selected: bool,
    focused: bool,
) -> ProfileRowVisualState {
    let progress = if profile.login_required {
        None
    } else {
        profile
            .used_percent
            .zip(profile.usage_status)
            .map(|(percent, status)| (percent.min(100), status))
    };
    ProfileRowVisualState {
        surface: if selected {
            ProfileRowSurfaceRole::Selected
        } else {
            ProfileRowSurfaceRole::Neutral
        },
        progress,
        system_marker: profile.id == crate::UsageProfileId::System,
        current_marker: profile.selected,
        focused,
    }
}

/// 반투명 대화 상자 색상을 불투명 행 표면 위의 Win32 `COLORREF`로 합성합니다.
///
/// `foreground.opacity`를 각 BGR 채널에 동일하게 적용하며 반환값은 GDI solid brush에 바로
/// 전달할 수 있는 불투명 색상입니다. 함수는 부동소수점이나 외부 상태를 사용하지 않습니다.
fn composite_dialog_color(foreground: DialogColor, background: DialogColor) -> u32 {
    let alpha = u32::from(foreground.opacity);
    let inverse = u32::from(u8::MAX) - alpha;
    let blend = |shift: u32| {
        let foreground_channel = (foreground.colorref >> shift) & 0xff;
        let background_channel = (background.colorref >> shift) & 0xff;
        (foreground_channel * alpha + background_channel * inverse + 127) / 255
    };
    blend(0) | (blend(8) << 8) | (blend(16) << 16)
}

const SELECTED_ROW_TINT_OPACITY: u8 = 20;

/// 프로필 행의 선택 상태에 대응하는 불투명 Win32 표면색을 반환합니다.
///
/// 선택되지 않은 행은 팔레트의 중립 surface를 그대로 사용하고, 선택 행은 healthy green을
/// 20/255 불투명도로 같은 중립 surface 위에 합성합니다. 밝은·어두운 테마 모두 같은 의미와
/// 강도를 유지하며 GDI 자원을 만들거나 외부 상태를 변경하지 않습니다.
fn profile_row_surface_color(palette: DialogPalette, role: ProfileRowSurfaceRole) -> u32 {
    match role {
        ProfileRowSurfaceRole::Neutral => palette.surface.colorref,
        ProfileRowSurfaceRole::Selected => composite_dialog_color(
            DialogColor::translucent(palette.healthy.colorref, SELECTED_ROW_TINT_OPACITY),
            palette.surface,
        ),
    }
}

/// 프로필 행 콘텐츠에 대화상자 공통 바깥 여백을 현재 DPI로 적용합니다.
///
/// `dpi`가 96보다 작으면 공통 스케일 규칙에 따라 96 DPI로 보정하며, 반환값은 owner-draw
/// 행의 텍스트와 진행 표시가 사용하는 물리 픽셀 단위 좌우 여백입니다.
fn profile_row_content_padding(dpi: u32) -> i32 {
    scale_logical(crate::windows::design::OUTER_PADDING, dpi)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProfileRowFirstLineLayout {
    name: LogicalRect,
    markers: Option<LogicalRect>,
}

/// 첫 줄에서 역할 표식의 실제 폭을 먼저 확보하고 남은 영역만 이름에 배정합니다.
///
/// `line`과 `marker_width`는 현재 DPI의 물리 픽셀입니다. LTR에서는 표식을 오른쪽에,
/// RTL에서는 왼쪽에 두며 이름과 표식 사이에 DPI 배율 간격을 둡니다. 폭이 극단적으로 좁으면
/// 표식을 우선하고 이름 영역을 0까지 줄여 역할 의미가 이름 말줄임에 함께 사라지지 않게 합니다.
fn profile_row_first_line_layout(
    line: LogicalRect,
    marker_width: i32,
    dpi: u32,
    rtl: bool,
) -> ProfileRowFirstLineLayout {
    let width = line.width().max(0);
    let marker_width = marker_width.clamp(0, width);
    if marker_width == 0 {
        return ProfileRowFirstLineLayout {
            name: line,
            markers: None,
        };
    }
    let gap = scale_logical(crate::windows::design::GAP_8, dpi).min(width - marker_width);
    if rtl {
        let markers = LogicalRect::new(line.left, line.top, line.left + marker_width, line.bottom);
        ProfileRowFirstLineLayout {
            name: LogicalRect::new(markers.right + gap, line.top, line.right, line.bottom),
            markers: Some(markers),
        }
    } else {
        let markers =
            LogicalRect::new(line.right - marker_width, line.top, line.right, line.bottom);
        ProfileRowFirstLineLayout {
            name: LogicalRect::new(line.left, line.top, markers.left - gap, line.bottom),
            markers: Some(markers),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileRowFillStage {
    Surface,
    SelectionEdge,
    ProgressTrack,
    ProgressFill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileRowTextStage {
    Name,
    Markers,
    Summary,
    Fallback,
}

/// owner-draw 행의 GDI 상태 변경과 stock fallback을 관찰 가능한 경계로 분리합니다.
///
/// 구현은 DC brush 색 적용·복원, 텍스트 상태 적용·역순 복원, 시스템 색 brush 및 빌린
/// `DEFAULT_GUI_FONT` 사용을 보장해야 합니다. 테스트 구현은 실제 HDC 없이 각 실패 지점을
/// 결정적으로 주입합니다.
trait ProfileRowPaintBackend {
    fn apply_fill_color(&mut self, color: COLORREF) -> io::Result<COLORREF>;
    fn fill_custom_rect(&mut self, stage: ProfileRowFillStage, rect: &RECT) -> io::Result<()>;
    fn restore_fill_color(&mut self, color: COLORREF) -> io::Result<()>;
    fn fill_system_rect(&mut self, rect: &RECT, selected: bool) -> io::Result<COLORREF>;
    fn default_gui_font(&mut self) -> io::Result<HFONT>;
    fn select_font(&mut self, font: HFONT) -> io::Result<HGDIOBJ>;
    fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE>;
    fn set_text_color(&mut self, color: COLORREF) -> io::Result<COLORREF>;
    fn measure_text_width(&mut self, text: &str) -> io::Result<i32>;
    fn draw_text(
        &mut self,
        stage: ProfileRowTextStage,
        text: &str,
        rect: &mut RECT,
        format: DRAW_TEXT_FORMAT,
    ) -> io::Result<()>;
    fn draw_focus(&mut self, rect: &RECT) -> io::Result<()>;
    fn restore_font(&mut self, font: HGDIOBJ) -> io::Result<()>;
    fn restore_background(&mut self, mode: BACKGROUND_MODE) -> io::Result<()>;
    fn restore_text_color(&mut self, color: COLORREF) -> io::Result<()>;
}

struct WindowsProfileRowPaintBackend {
    dc: HDC,
    dc_brush: HBRUSH,
}

impl WindowsProfileRowPaintBackend {
    /// 현재 `WM_DRAWITEM` HDC를 빌리고 stock DC brush 핸들을 보관합니다.
    ///
    /// stock 객체의 소유권을 취하지 않으며 null stock brush는 custom fill 실패로 전달되어
    /// 시스템 색 brush fallback이 계속 시도됩니다.
    unsafe fn new(dc: HDC) -> Self {
        Self {
            dc,
            dc_brush: HBRUSH(GetStockObject(DC_BRUSH).0),
        }
    }
}

impl ProfileRowPaintBackend for WindowsProfileRowPaintBackend {
    fn apply_fill_color(&mut self, color: COLORREF) -> io::Result<COLORREF> {
        let previous = unsafe { SetDCBrushColor(self.dc, color) };
        if previous.0 == CLR_INVALID {
            Err(io::Error::other(
                "profile row brush color could not be applied",
            ))
        } else {
            Ok(previous)
        }
    }

    fn fill_custom_rect(&mut self, _stage: ProfileRowFillStage, rect: &RECT) -> io::Result<()> {
        if self.dc_brush.0.is_null() {
            return Err(io::Error::other(
                "profile row stock DC brush is unavailable",
            ));
        }
        if unsafe { FillRect(self.dc, rect, self.dc_brush) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn restore_fill_color(&mut self, color: COLORREF) -> io::Result<()> {
        if unsafe { SetDCBrushColor(self.dc, color) }.0 == CLR_INVALID {
            Err(io::Error::other(
                "profile row brush color could not be restored",
            ))
        } else {
            Ok(())
        }
    }

    fn fill_system_rect(&mut self, rect: &RECT, selected: bool) -> io::Result<COLORREF> {
        let (background, text) = if selected {
            (COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT)
        } else {
            (COLOR_WINDOW, COLOR_WINDOWTEXT)
        };
        let brush = unsafe { GetSysColorBrush(background) };
        if brush.0.is_null() || unsafe { FillRect(self.dc, rect, brush) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(COLORREF(unsafe { GetSysColor(text) }))
        }
    }

    fn default_gui_font(&mut self) -> io::Result<HFONT> {
        let font = unsafe { HFONT(GetStockObject(DEFAULT_GUI_FONT).0) };
        if font.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(font)
        }
    }

    fn select_font(&mut self, font: HFONT) -> io::Result<HGDIOBJ> {
        let previous = unsafe { SelectObject(self.dc, HGDIOBJ(font.0)) };
        if previous.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(previous)
        }
    }

    fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE> {
        let previous = unsafe { SetBkMode(self.dc, TRANSPARENT) };
        if previous == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(BACKGROUND_MODE(previous as u32))
        }
    }

    fn set_text_color(&mut self, color: COLORREF) -> io::Result<COLORREF> {
        let previous = unsafe { SetTextColor(self.dc, color) };
        if previous.0 == CLR_INVALID {
            Err(io::Error::last_os_error())
        } else {
            Ok(previous)
        }
    }

    fn measure_text_width(&mut self, text: &str) -> io::Result<i32> {
        if text.is_empty() {
            return Ok(0);
        }
        let text = text.encode_utf16().collect::<Vec<_>>();
        let mut size = SIZE::default();
        if unsafe { GetTextExtentPoint32W(self.dc, &text, &mut size) }.as_bool() {
            Ok(size.cx.max(0))
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn draw_text(
        &mut self,
        _stage: ProfileRowTextStage,
        text: &str,
        rect: &mut RECT,
        format: DRAW_TEXT_FORMAT,
    ) -> io::Result<()> {
        if text.is_empty() || rect.right <= rect.left || rect.bottom <= rect.top {
            return Ok(());
        }
        let mut text = text.encode_utf16().collect::<Vec<_>>();
        if unsafe { DrawTextW(self.dc, &mut text, rect, format) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn draw_focus(&mut self, rect: &RECT) -> io::Result<()> {
        if unsafe { DrawFocusRect(self.dc, rect) }.as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn restore_font(&mut self, font: HGDIOBJ) -> io::Result<()> {
        if unsafe { SelectObject(self.dc, font) }.0.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn restore_background(&mut self, mode: BACKGROUND_MODE) -> io::Result<()> {
        if unsafe { SetBkMode(self.dc, mode) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn restore_text_color(&mut self, color: COLORREF) -> io::Result<()> {
        if unsafe { SetTextColor(self.dc, color) }.0 == CLR_INVALID {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

fn paint_profile_row_custom_fill<B: ProfileRowPaintBackend>(
    backend: &mut B,
    stage: ProfileRowFillStage,
    rect: &RECT,
    color: u32,
) -> io::Result<()> {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return Ok(());
    }
    let previous = backend.apply_fill_color(COLORREF(color))?;
    let fill_result = backend.fill_custom_rect(stage, rect);
    let restore_result = backend.restore_fill_color(previous);
    match (fill_result, restore_result) {
        (Err(error), _) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn restore_profile_row_text_state<B: ProfileRowPaintBackend>(
    backend: &mut B,
    previous_font: Option<HGDIOBJ>,
    previous_background: Option<BACKGROUND_MODE>,
    previous_text: Option<COLORREF>,
) -> io::Result<()> {
    let mut first_error = None;
    if let Some(color) = previous_text {
        if let Err(error) = backend.restore_text_color(color) {
            first_error = Some(error);
        }
    }
    if let Some(mode) = previous_background {
        if let Err(error) = backend.restore_background(mode) {
            first_error.get_or_insert(error);
        }
    }
    if let Some(font) = previous_font {
        if let Err(error) = backend.restore_font(font) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn directional_profile_text_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let mut format = DT_SINGLELINE | DT_NOPREFIX;
    if rtl {
        format |= DT_RTLREADING | DT_RIGHT;
    }
    format
}

fn rect_from_logical(rect: LogicalRect) -> RECT {
    RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_profile_row_custom<B: ProfileRowPaintBackend>(
    backend: &mut B,
    rect: RECT,
    profile: &UsageProfileView,
    copy: &ProfileManagerRowText,
    visuals: ProfileRowPaintResources,
    rtl: bool,
    selected: bool,
    focused: bool,
) -> io::Result<()> {
    if rect.right <= rect.left || rect.bottom <= rect.top || visuals.body_font.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid profile row drawing target",
        ));
    }
    let visual = profile_row_visual_state(profile, selected, focused);
    let surface_color = profile_row_surface_color(visuals.palette, visual.surface);
    paint_profile_row_custom_fill(backend, ProfileRowFillStage::Surface, &rect, surface_color)?;

    let edge_width = scale_logical(crate::windows::design::SELECTION_EDGE, visuals.dpi).max(1);
    if selected {
        let edge = if rtl {
            RECT {
                left: rect.right - edge_width,
                right: rect.right,
                ..rect
            }
        } else {
            RECT {
                left: rect.left,
                right: rect.left + edge_width,
                ..rect
            }
        };
        paint_profile_row_custom_fill(
            backend,
            ProfileRowFillStage::SelectionEdge,
            &edge,
            visuals.palette.focus.colorref,
        )?;
    }

    let horizontal_padding = profile_row_content_padding(visuals.dpi);
    let leading_inset = horizontal_padding + edge_width;
    let content_left = rect.left
        + if rtl {
            horizontal_padding
        } else {
            leading_inset
        };
    let content_right = rect.right
        - if rtl {
            leading_inset
        } else {
            horizontal_padding
        };
    if content_right <= content_left {
        return Ok(());
    }

    if let Some((percent, status)) = visual.progress {
        let progress_height =
            scale_logical(crate::windows::design::PROGRESS_HEIGHT, visuals.dpi).max(1);
        let progress_bottom = rect.bottom - scale_logical(5, visuals.dpi);
        let progress_track = RECT {
            left: content_left,
            top: progress_bottom - progress_height,
            right: content_right,
            bottom: progress_bottom,
        };
        let track_color = composite_dialog_color(
            visuals.palette.progress_track,
            DialogColor::opaque(surface_color),
        );
        paint_profile_row_custom_fill(
            backend,
            ProfileRowFillStage::ProgressTrack,
            &progress_track,
            track_color,
        )?;
        let track_width = (content_right - content_left).max(0);
        let fill_width = (i64::from(track_width) * i64::from(percent.min(100)) / 100) as i32;
        if fill_width > 0 {
            let progress_fill = if rtl {
                RECT {
                    left: content_right - fill_width,
                    right: content_right,
                    ..progress_track
                }
            } else {
                RECT {
                    left: content_left,
                    right: content_left + fill_width,
                    ..progress_track
                }
            };
            paint_profile_row_custom_fill(
                backend,
                ProfileRowFillStage::ProgressFill,
                &progress_fill,
                visuals.palette.status(status),
            )?;
        }
    }

    let mut previous_font = None;
    let mut previous_background = None;
    let mut previous_text = None;
    let painted = (|| -> io::Result<()> {
        previous_font = Some(backend.select_font(visuals.body_font)?);
        previous_background = Some(backend.set_transparent_background()?);
        previous_text = Some(backend.set_text_color(COLORREF(visuals.palette.text.colorref))?);

        let base_format = directional_profile_text_format(rtl);
        let marker_text = copy.markers.join(" · ");
        let first_line = LogicalRect::new(
            content_left,
            rect.top + scale_logical(5, visuals.dpi),
            content_right,
            rect.top + scale_logical(24, visuals.dpi),
        );
        let marker_width = backend.measure_text_width(&marker_text)?;
        let first_line = profile_row_first_line_layout(first_line, marker_width, visuals.dpi, rtl);
        let mut name_rect = rect_from_logical(first_line.name);
        backend.draw_text(
            ProfileRowTextStage::Name,
            &copy.name,
            &mut name_rect,
            base_format | DT_END_ELLIPSIS,
        )?;

        backend.set_text_color(COLORREF(visuals.palette.secondary_text.colorref))?;
        if let Some(marker_rect) = first_line.markers {
            let mut marker_rect = rect_from_logical(marker_rect);
            backend.draw_text(
                ProfileRowTextStage::Markers,
                &marker_text,
                &mut marker_rect,
                base_format,
            )?;
        }
        let mut summary_rect = RECT {
            left: content_left,
            top: rect.top + scale_logical(26, visuals.dpi),
            right: content_right,
            bottom: rect.top + scale_logical(45, visuals.dpi),
        };
        backend.draw_text(
            ProfileRowTextStage::Summary,
            &copy.summary,
            &mut summary_rect,
            base_format | DT_END_ELLIPSIS,
        )?;
        if visual.focused {
            backend.draw_focus(&RECT {
                left: rect.left + 1,
                top: rect.top + 1,
                right: rect.right - 1,
                bottom: rect.bottom - 1,
            })?;
        }
        Ok(())
    })();
    let restored =
        restore_profile_row_text_state(backend, previous_font, previous_background, previous_text);
    painted.and(restored)
}

fn paint_profile_row_fallback<B: ProfileRowPaintBackend>(
    backend: &mut B,
    rect: RECT,
    accessible_text: &str,
    rtl: bool,
    selected: bool,
    focused: bool,
) -> io::Result<()> {
    let text_color = backend.fill_system_rect(&rect, selected)?;
    let font = backend.default_gui_font()?;
    let mut previous_font = None;
    let mut previous_background = None;
    let mut previous_text = None;
    let painted = (|| -> io::Result<()> {
        previous_font = Some(backend.select_font(font)?);
        previous_background = Some(backend.set_transparent_background()?);
        previous_text = Some(backend.set_text_color(text_color)?);
        let mut text_rect = RECT {
            left: rect.left + 2,
            top: rect.top,
            right: rect.right - 2,
            bottom: rect.bottom,
        };
        backend.draw_text(
            ProfileRowTextStage::Fallback,
            accessible_text,
            &mut text_rect,
            directional_profile_text_format(rtl) | DT_END_ELLIPSIS | DT_VCENTER,
        )?;
        if focused {
            backend.draw_focus(&RECT {
                left: rect.left + 1,
                top: rect.top + 1,
                right: rect.right - 1,
                bottom: rect.bottom - 1,
            })?;
        }
        Ok(())
    })();
    let restored =
        restore_profile_row_text_state(backend, previous_font, previous_background, previous_text);
    painted.and(restored)
}

/// 사용자 지정 행 렌더링이 실패하면 시스템 색과 stock 글꼴의 최소 렌더링으로 복구합니다.
///
/// custom 경로의 brush 적용, 채우기, 텍스트, focus 또는 상태 복원 오류는 fallback으로 전달됩니다.
/// 두 경로 중 하나가 모든 그리기와 복원을 끝낸 경우에만 `true`를 반환하므로 `WM_DRAWITEM`
/// 호출자는 실패한 owner-draw 행을 처리했다고 잘못 보고하지 않습니다.
#[allow(clippy::too_many_arguments)]
fn paint_profile_row_with_fallback<B: ProfileRowPaintBackend>(
    backend: &mut B,
    rect: RECT,
    profile: &UsageProfileView,
    copy: &ProfileManagerRowText,
    accessible_text: &str,
    visuals: ProfileRowPaintResources,
    rtl: bool,
    selected: bool,
    focused: bool,
) -> bool {
    paint_profile_row_custom(
        backend, rect, profile, copy, visuals, rtl, selected, focused,
    )
    .or_else(|_| paint_profile_row_fallback(backend, rect, accessible_text, rtl, selected, focused))
    .is_ok()
}

/// owner-draw 프로필 행을 현재 DPI와 팔레트에 맞춰 그리고 실패 시 stock GDI로 복구합니다.
///
/// `item`은 현재 `WM_DRAWITEM` 메시지가 제공한 살아 있는 listbox 항목이어야 합니다.
/// `profile`, `copy`, `accessible_text`는 표시 전용 소유 복사본이며 `visuals`는 대화 상자
/// 자원에서 미리 복사한 스냅샷입니다. 함수는 문자열을 파싱하지 않고 typed usage 쌍이 있을
/// 때만 진행 표시를 그리며, custom 경로 실패 시 전체 접근성 문자열로 최소 행을 다시 그립니다.
///
/// # Safety
///
/// `item.hDC`와 `item.rcItem`은 Windows가 현재 그리기 콜백에 허용한 범위여야 합니다. 호출자는
/// 동기 GDI 호출 전에 대화 상자 상태와 자원 객체의 모든 Rust borrow를 끝내야 합니다.
unsafe fn draw_profile_row(
    item: &DRAWITEMSTRUCT,
    profile: &UsageProfileView,
    copy: &ProfileManagerRowText,
    accessible_text: &str,
    visuals: ProfileRowPaintResources,
    rtl: bool,
) -> io::Result<()> {
    if item.hDC.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid profile row drawing target",
        ));
    }
    let mut backend = WindowsProfileRowPaintBackend::new(item.hDC);
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let focused = item.itemState.0 & ODS_FOCUS.0 != 0;
    if paint_profile_row_with_fallback(
        &mut backend,
        item.rcItem,
        profile,
        copy,
        accessible_text,
        visuals,
        rtl,
        selected,
        focused,
    ) {
        Ok(())
    } else {
        Err(io::Error::other(
            "profile row custom and fallback painting both failed",
        ))
    }
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

/// 현재 창이 실제로 놓인 목적지 모니터의 작업 영역을 반환합니다.
///
/// DPI suggested RECT를 적용한 뒤 호출하면 소유자나 커서가 아니라 이동된 창 자체의 모니터를
/// 기준으로 합니다. 조회 실패는 `None`으로 안전하게 대체됩니다.
unsafe fn dialog_window_work_area(dialog: HWND) -> Option<DialogWorkArea> {
    let monitor = MonitorFromWindow(dialog, MONITOR_DEFAULTTONEAREST);
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
        name_label: HWND::default(),
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
    let anchor = profile_manager_dialog_monitor_anchor();
    let work_area = dialog_work_area(anchor);
    let (initial_x, initial_y) = work_area
        .map(|area| (area.left, area.top))
        .unwrap_or((CW_USEDEFAULT, CW_USEDEFAULT));
    let dialog = CreateWindowExW(
        dialog_root_ex_style(language),
        DIALOG_CLASS,
        PCWSTR(title.as_ptr()),
        dialog_window_style(),
        initial_x,
        initial_y,
        1,
        1,
        parent,
        None,
        Some(instance),
        Some(state_pointer.cast_const()),
    )
    .map_err(win_error)?;
    let mut window_guard = ModalWindowGuard::new(dialog, owner);

    setup_controls(dialog, instance, profiles, &mut state)?;
    rebuild_dialog_visuals(dialog, std::ptr::addr_of_mut!(state.resources));
    let requested_width =
        requested_client_width_for_work_area(work_area, state.resources.dpi, language);
    relayout_manager_dialog(dialog, &mut *state, requested_width, work_area)?;
    if let Some(work_area) = work_area {
        center_window_in_work_area(dialog, work_area);
    } else {
        center_window(dialog, anchor);
    }

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
        name_label: HWND::default(),
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
    let anchor = add_profile_dialog_monitor_anchor(owner, live_window_size(owner));
    let work_area = dialog_work_area(anchor);
    let (initial_x, initial_y) = work_area
        .map(|area| (area.left, area.top))
        .unwrap_or((CW_USEDEFAULT, CW_USEDEFAULT));
    let dialog = CreateWindowExW(
        dialog_root_ex_style(language),
        ADD_DIALOG_CLASS,
        PCWSTR(title.as_ptr()),
        dialog_window_style(),
        initial_x,
        initial_y,
        1,
        1,
        Some(owner),
        None,
        Some(instance),
        Some(state_pointer.cast_const()),
    )
    .map_err(win_error)?;
    let mut window_guard = ModalWindowGuard::new(dialog, owner);

    setup_add_dialog_controls(dialog, instance, &mut state)?;
    rebuild_dialog_visuals(dialog, std::ptr::addr_of_mut!(state.resources));
    let requested_width =
        requested_client_width_for_work_area(work_area, state.resources.dpi, language);
    relayout_add_dialog(dialog, &mut *state, requested_width, work_area)?;
    if let Some(work_area) = work_area {
        center_window_in_work_area(dialog, work_area);
    } else {
        center_window(dialog, anchor);
    }

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
    let child_ex_style = dialog_child_text_ex_style(state.language);
    state.list = create_control(
        dialog,
        instance,
        w!("LISTBOX"),
        "",
        PROFILE_LIST_ID,
        0,
        0,
        1,
        1,
        WS_CHILD
            | WS_VISIBLE
            | WS_BORDER
            | WS_VSCROLL
            | WS_TABSTOP
            | WINDOW_STYLE(
                (LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32,
            ),
        child_ex_style,
    )?;
    let row_height = state.resources.profile_row_height();
    if SendMessageW(
        state.list,
        LB_SETITEMHEIGHT,
        Some(WPARAM(0)),
        Some(LPARAM(row_height as isize)),
    )
    .0 < 0
    {
        return Err(io::Error::other(
            "profile list row height could not be applied",
        ));
    }

    let name_label = localized_text(LocalizationKey::UsageProfileName, state.language);
    state.name_label = create_control(
        dialog,
        instance,
        w!("STATIC"),
        name_label,
        0,
        0,
        0,
        1,
        1,
        WS_CHILD | WS_VISIBLE,
        child_ex_style,
    )?;
    state.edit = create_control(
        dialog,
        instance,
        w!("EDIT"),
        "",
        PROFILE_NAME_ID,
        0,
        0,
        1,
        1,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
        child_ex_style,
    )?;
    let _ = SendMessageW(
        state.edit,
        EM_SETLIMITTEXT,
        Some(WPARAM(PROFILE_LABEL_MAX_UTF16_UNITS)),
        None,
    );

    let labels = profile_dialog_button_labels(state.language);
    for control in PROFILE_MANAGER_CONTROLS {
        let (text, description) = match control {
            ProfileManagerControl::AddBelowList => ("+", Some(labels[4])),
            ProfileManagerControl::Rename => (labels[0], None),
            ProfileManagerControl::Login => (labels[1], None),
            ProfileManagerControl::Logout => (labels[2], None),
            ProfileManagerControl::Delete => (labels[3], None),
        };
        let control_window = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            text,
            manager_control_id(control),
            0,
            0,
            1,
            1,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            child_ex_style,
        )?;
        if let Some(description) = description {
            create_add_control_tooltip(dialog, instance, control_window, description, state)?;
        }
    }

    for profile in profiles {
        let line = profile_manager_accessible_row_text(profile, state.language);
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

/// 공유 관리자 컨트롤 역할을 기존 Win32 명령 ID에 연결합니다.
///
/// 위치와 크기는 반환하지 않으며 모든 기하는 `profile_manager_layout` 결과만 사용합니다.
fn manager_control_id(control: ProfileManagerControl) -> i32 {
    match control {
        ProfileManagerControl::AddBelowList => OPEN_ADD_ID,
        ProfileManagerControl::Rename => RENAME_ID,
        ProfileManagerControl::Login => LOGIN_ID,
        ProfileManagerControl::Logout => LOGOUT_ID,
        ProfileManagerControl::Delete => DELETE_ID,
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
        uFlags: add_control_tooltip_flags(state.language),
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

/// 추가 컨트롤 tooltip이 사용할 방향 및 연결 플래그를 선택합니다.
///
/// 아랍어는 Windows tooltip 자체가 오른쪽에서 왼쪽으로 읽도록 `TTF_RTLREADING`을 더하고,
/// 다른 언어는 기존 HWND 식별자 및 자동 서브클래싱 플래그만 유지합니다.
fn add_control_tooltip_flags(language: Language) -> windows::Win32::UI::Controls::TOOLTIP_FLAGS {
    let flags = TTF_IDISHWND | TTF_SUBCLASS;
    if language == Language::Arabic {
        flags | TTF_RTLREADING
    } else {
        flags
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
    let child_ex_style = dialog_child_text_ex_style(state.language);
    state.name_label = create_control(
        dialog,
        instance,
        w!("STATIC"),
        localized_text(LocalizationKey::UsageProfileName, state.language),
        0,
        0,
        0,
        1,
        1,
        WS_CHILD | WS_VISIBLE,
        child_ex_style,
    )?;
    state.edit = create_control(
        dialog,
        instance,
        w!("EDIT"),
        "",
        ADD_PROFILE_NAME_ID,
        0,
        0,
        1,
        1,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP,
        child_ex_style,
    )?;
    let _ = SendMessageW(
        state.edit,
        EM_SETLIMITTEXT,
        Some(WPARAM(PROFILE_LABEL_MAX_UTF16_UNITS)),
        None,
    );

    let labels = profile_dialog_button_labels(state.language);
    for (id, text, extra_style) in [
        (IDOK.0, labels[4], WINDOW_STYLE(BS_DEFPUSHBUTTON as u32)),
        (IDCANCEL.0, labels[5], WINDOW_STYLE(0)),
    ] {
        let _ = create_control(
            dialog,
            instance,
            w!("BUTTON"),
            text,
            id,
            0,
            0,
            1,
            1,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | extra_style,
            child_ex_style,
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
    ex_style: WINDOW_EX_STYLE,
) -> io::Result<HWND> {
    let text = wide(text);
    CreateWindowExW(
        ex_style,
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
        WM_DPICHANGED => {
            if lparam.0 != 0 {
                let suggested = *(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            let requested_dpi = ((wparam.0 >> 16) & 0xffff) as u32;
            rebuild_dialog_visuals_for_dpi(
                hwnd,
                std::ptr::addr_of_mut!((*state).resources),
                requested_dpi,
            );
            let committed_dpi = (*state).resources.dpi;
            let work_area = dialog_window_work_area(hwnd);
            let requested_width =
                requested_client_width_for_work_area(work_area, committed_dpi, (*state).language);
            let _ = relayout_manager_dialog(hwnd, state, requested_width, work_area);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            rebuild_dialog_visuals(hwnd, std::ptr::addr_of_mut!((*state).resources));
            let committed_dpi = (*state).resources.dpi;
            let work_area = dialog_window_work_area(hwnd);
            let requested_width =
                requested_client_width_for_work_area(work_area, committed_dpi, (*state).language);
            let _ = relayout_manager_dialog(hwnd, state, requested_width, work_area);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            if erase_dialog_background(hwnd, &(*state).resources, wparam) {
                LRESULT(1)
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_DRAWITEM => handle_profile_row_draw(state, lparam),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            dialog_control_color(&(*state).resources, message, wparam)
        }
        WM_NOTIFY => {
            let (label, font, palette, rtl) = {
                let state = &*state;
                (
                    profile_dialog_button_labels(state.language)[1],
                    state.resources.body_font,
                    state.resources.palette,
                    state.language == Language::Arabic,
                )
            };
            draw_primary_button_notification(lparam, LOGIN_ID, label, font, palette, rtl)
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

/// `WM_DRAWITEM`의 항목 identity를 표시 모델에 연결하고 owner-draw 행을 처리합니다.
///
/// # Safety
///
/// `state`는 현재 관리자 HWND의 살아 있는 `DialogState`를 가리켜야 하고 `lparam`은 Windows가
/// 전달한 `DRAWITEMSTRUCT`여야 합니다. 프로필, 문자열, 팔레트·폰트·DPI를 모두 복사한 뒤 상태
/// borrow를 끝내므로 이어지는 동기 GDI 호출과 재진입 가능한 대화 상자 상태가 겹치지 않습니다.
unsafe fn handle_profile_row_draw(state: *mut DialogState, lparam: LPARAM) -> LRESULT {
    if lparam.0 == 0 {
        return LRESULT(0);
    }
    let item = *(lparam.0 as *const DRAWITEMSTRUCT);
    if item.CtlType != ODT_LISTBOX
        || item.CtlID != PROFILE_LIST_ID as u32
        || item.itemID == u32::MAX
    {
        return LRESULT(0);
    }

    let Some((profile, copy, accessible_text, visuals, rtl, list)) = (|| {
        let state = &*state;
        let profile = state.controller.profile_at(item.itemID as usize)?.clone();
        let copy = profile_manager_row_text(&profile, state.language);
        let accessible_text = profile_manager_accessible_row_text(&profile, state.language);
        Some((
            profile,
            copy,
            accessible_text,
            state.resources.profile_row_snapshot(),
            state.language == Language::Arabic,
            state.list,
        ))
    })() else {
        return LRESULT(0);
    };
    if item.hwndItem != list {
        return LRESULT(0);
    }

    if draw_profile_row(&item, &profile, &copy, &accessible_text, visuals, rtl).is_ok() {
        LRESULT(1)
    } else {
        LRESULT(0)
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
        WM_DPICHANGED => {
            if lparam.0 != 0 {
                let suggested = *(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            let requested_dpi = ((wparam.0 >> 16) & 0xffff) as u32;
            rebuild_dialog_visuals_for_dpi(
                hwnd,
                std::ptr::addr_of_mut!((*state).resources),
                requested_dpi,
            );
            let committed_dpi = (*state).resources.dpi;
            let work_area = dialog_window_work_area(hwnd);
            let requested_width =
                requested_client_width_for_work_area(work_area, committed_dpi, (*state).language);
            let _ = relayout_add_dialog(hwnd, state, requested_width, work_area);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            rebuild_dialog_visuals(hwnd, std::ptr::addr_of_mut!((*state).resources));
            let committed_dpi = (*state).resources.dpi;
            let work_area = dialog_window_work_area(hwnd);
            let requested_width =
                requested_client_width_for_work_area(work_area, committed_dpi, (*state).language);
            let _ = relayout_add_dialog(hwnd, state, requested_width, work_area);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            if erase_dialog_background(hwnd, &(*state).resources, wparam) {
                LRESULT(1)
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            dialog_control_color(&(*state).resources, message, wparam)
        }
        WM_NOTIFY => {
            let (label, font, palette, rtl) = {
                let state = &*state;
                (
                    profile_dialog_button_labels(state.language)[4],
                    state.resources.body_font,
                    state.resources.palette,
                    state.language == Language::Arabic,
                )
            };
            draw_primary_button_notification(lparam, IDOK.0, label, font, palette, rtl)
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
        set_enabled(
            hwnd,
            manager_control_id(control),
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
        Foundation::{COLORREF, HWND, RECT},
        Graphics::Gdi::{
            BACKGROUND_MODE, CLR_INVALID, DRAW_TEXT_FORMAT, DT_END_ELLIPSIS, DT_RIGHT,
            DT_RTLREADING, HBRUSH, HFONT, HGDIOBJ,
        },
        UI::Controls::{CDIS_DEFAULT, CDIS_FOCUS, TTF_IDISHWND, TTF_RTLREADING, TTF_SUBCLASS},
        UI::WindowsAndMessaging::{
            IDCANCEL, IDOK, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MESSAGEBOX_RESULT,
            MESSAGEBOX_STYLE,
        },
    };

    use crate::{
        windows::{
            design::{DialogColor, DialogPalette, DialogTheme},
            ProfileUsageStatus,
        },
        Language, LocalizationKey, UsageProfileId,
    };

    use super::{
        add_control_tooltip_flags, add_profile_prompt_result, bound_dialog_outer_rect,
        composite_dialog_color, confirm_profile_delete_with_presenter,
        confirm_profile_login_with_presenter, consume_centered_message_box_request,
        dialog_child_setpos_rect, erase_dialog_background_with, fill_primary_button_rect,
        fit_manager_layout_to_client_height, handle_add_profile_prompt_result_with_presenter,
        handle_manager_rename_result_with_presenter, paint_primary_button,
        paint_profile_row_with_fallback, primary_button_paint_state, profile_row_content_padding,
        profile_row_first_line_layout, profile_row_surface_color, profile_row_visual_state,
        show_add_dialog_warning_with_presenter, show_safe_error_with_presenter,
        update_dialog_visual_resources, AddDialogState, AddProfilePromptCommand,
        CenteredMessageBoxHookBackend, CenteredMessageBoxHookGuard, DialogFontFace,
        DialogResourceBackend, DialogVisualResources, DialogVisualUpdateOutcome, DialogWindowSize,
        DialogWorkArea, PrimaryButtonBrushBackend, PrimaryButtonCue, PrimaryButtonPaintBackend,
        PrimaryButtonPaintStage, ProfileDialogController, ProfileMessagePresenter,
        ProfileMessageRoute, ProfileRowFillStage, ProfileRowPaintBackend, ProfileRowPaintResources,
        ProfileRowSurfaceRole, ProfileRowTextStage, UsageProfileView,
    };

    fn mirrored_parent_physical_rect(
        rect: crate::windows::design::LogicalRect,
        width: i32,
    ) -> crate::windows::design::LogicalRect {
        crate::windows::design::LogicalRect::new(
            width - rect.right,
            rect.top,
            width - rect.left,
            rect.bottom,
        )
    }

    #[test]
    fn rtl_child_setpos_coordinates_produce_the_pure_layout_physical_rectangles_once() {
        let layout = crate::windows::design::profile_manager_layout(
            crate::windows::design::DialogLayoutInput::new(620, 144, true, [160, 120, 140, 100]),
        );

        for desired in [
            layout.add_control,
            layout.name_edit,
            layout.action_buttons[0],
            layout.action_buttons[1],
            layout.action_buttons[2],
            layout.action_buttons[3],
        ] {
            let setpos = dialog_child_setpos_rect(desired, layout.client.width(), true);
            assert_eq!(
                mirrored_parent_physical_rect(setpos, layout.client.width()),
                desired,
            );
        }
    }

    #[test]
    fn constrained_manager_height_reduces_only_the_list_to_whole_rows() {
        for (dpi, expected_rows) in [(144, 2), (192, 1)] {
            let layout = crate::windows::design::profile_manager_layout(
                crate::windows::design::DialogLayoutInput::new(
                    620,
                    dpi,
                    false,
                    [160, 120, 140, 100],
                ),
            );
            let row_height = crate::windows::design::scale_logical(56, dpi);
            let fixed_height = layout.client.height() - layout.list.height();
            let maximum_height = fixed_height + row_height * expected_rows;

            let fitted = fit_manager_layout_to_client_height(layout, maximum_height, dpi);

            assert_eq!(fitted.client.height(), maximum_height);
            assert_eq!(fitted.list.height(), row_height * expected_rows);
            assert_eq!(fitted.add_control.height(), layout.add_control.height());
            assert_eq!(fitted.name_edit.height(), layout.name_edit.height());
            assert!(fitted
                .action_buttons
                .iter()
                .zip(layout.action_buttons)
                .all(|(actual, original)| actual.height() == original.height()));
            assert!(fitted.list.bottom <= fitted.add_control.top);
            assert!(fitted
                .action_buttons
                .iter()
                .all(|rect| rect.bottom <= fitted.client.bottom));
        }
    }

    #[test]
    fn destination_work_area_bounds_and_repositions_the_outer_window_on_both_axes() {
        let work_area = DialogWorkArea::new(-1280, 0, 0, 720);
        let suggested = RECT {
            left: -900,
            top: 300,
            right: 500,
            bottom: 1200,
        };

        assert_eq!(
            bound_dialog_outer_rect(suggested, DialogWindowSize::new(1400, 900), work_area,),
            RECT {
                left: -1280,
                top: 0,
                right: 0,
                bottom: 720,
            }
        );

        assert_eq!(
            bound_dialog_outer_rect(suggested, DialogWindowSize::new(900, 600), work_area,),
            RECT {
                left: -900,
                top: 120,
                right: 0,
                bottom: 720,
            }
        );
    }

    #[test]
    fn primary_button_default_state_has_a_distinct_default_border_cue() {
        let focused = primary_button_paint_state(CDIS_FOCUS, true).unwrap();
        let defaulted = primary_button_paint_state(CDIS_DEFAULT, true).unwrap();

        assert_eq!(focused.cue, PrimaryButtonCue::Focus);
        assert_eq!(defaulted.cue, PrimaryButtonCue::DefaultBorder);
        assert_ne!(focused.cue, defaulted.cue);
    }

    #[test]
    fn add_control_tooltip_uses_rtl_reading_only_for_arabic() {
        assert_eq!(
            add_control_tooltip_flags(Language::English),
            TTF_IDISHWND | TTF_SUBCLASS
        );
        assert_eq!(
            add_control_tooltip_flags(Language::Arabic),
            TTF_IDISHWND | TTF_SUBCLASS | TTF_RTLREADING
        );
    }

    struct RecordingPrimaryButtonBrushBackend {
        color_results: VecDeque<COLORREF>,
        set_colors: Vec<COLORREF>,
        fill_calls: usize,
        fail_fill: bool,
    }

    impl PrimaryButtonBrushBackend for RecordingPrimaryButtonBrushBackend {
        fn set_brush_color(&mut self, color: COLORREF) -> COLORREF {
            self.set_colors.push(color);
            self.color_results.pop_front().unwrap()
        }

        fn fill_rect(&mut self, _rect: &RECT) -> io::Result<()> {
            self.fill_calls += 1;
            if self.fail_fill {
                Err(io::Error::other("injected FillRect failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn concrete_primary_fill_rejects_brush_color_apply_failure_before_fill() {
        let mut backend = RecordingPrimaryButtonBrushBackend {
            color_results: VecDeque::from([COLORREF(CLR_INVALID)]),
            set_colors: Vec::new(),
            fill_calls: 0,
            fail_fill: false,
        };
        let rect = RECT {
            left: 0,
            top: 0,
            right: 80,
            bottom: 32,
        };

        assert!(fill_primary_button_rect(&mut backend, &rect, 0x0012_3456).is_err());
        assert_eq!(backend.set_colors, [COLORREF(0x0012_3456)]);
        assert_eq!(backend.fill_calls, 0);
    }

    #[test]
    fn concrete_primary_fill_propagates_restore_failure_and_restores_after_fill_failure() {
        let rect = RECT {
            left: 0,
            top: 0,
            right: 80,
            bottom: 32,
        };
        let previous = COLORREF(0x0065_4321);
        let requested = 0x0012_3456;
        let mut restore_failure = RecordingPrimaryButtonBrushBackend {
            color_results: VecDeque::from([previous, COLORREF(CLR_INVALID)]),
            set_colors: Vec::new(),
            fill_calls: 0,
            fail_fill: false,
        };

        assert!(fill_primary_button_rect(&mut restore_failure, &rect, requested).is_err());
        assert_eq!(restore_failure.set_colors, [COLORREF(requested), previous]);
        assert_eq!(restore_failure.fill_calls, 1);

        let mut fill_failure = RecordingPrimaryButtonBrushBackend {
            color_results: VecDeque::from([previous, COLORREF(requested)]),
            set_colors: Vec::new(),
            fill_calls: 0,
            fail_fill: true,
        };
        assert!(fill_primary_button_rect(&mut fill_failure, &rect, requested).is_err());
        assert_eq!(fill_failure.set_colors, [COLORREF(requested), previous]);
        assert_eq!(fill_failure.fill_calls, 1);
    }

    #[derive(Default)]
    struct FailingPrimaryPaintBackend {
        failure: Option<InjectedPrimaryPaintFailure>,
        acquired: usize,
        restore_attempts: usize,
        text_colors: Vec<COLORREF>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum InjectedPrimaryPaintFailure {
        DefaultBorder,
        Border,
        Surface,
        SelectFont,
        SetBackground,
        SetTextColor,
        Text,
        Focus,
        RestoreTextColor,
        RestoreBackground,
        RestoreFont,
    }

    impl PrimaryButtonPaintBackend for FailingPrimaryPaintBackend {
        fn fill_rect(
            &mut self,
            stage: PrimaryButtonPaintStage,
            _rect: &RECT,
            _color: u32,
        ) -> io::Result<()> {
            self.complete(match stage {
                PrimaryButtonPaintStage::DefaultBorder => {
                    InjectedPrimaryPaintFailure::DefaultBorder
                }
                PrimaryButtonPaintStage::Border => InjectedPrimaryPaintFailure::Border,
                PrimaryButtonPaintStage::Surface => InjectedPrimaryPaintFailure::Surface,
            })
        }

        fn select_font(&mut self, _font: HFONT) -> io::Result<HGDIOBJ> {
            self.complete(InjectedPrimaryPaintFailure::SelectFont)?;
            self.acquired += 1;
            Ok(HGDIOBJ(11_usize as _))
        }

        fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE> {
            self.complete(InjectedPrimaryPaintFailure::SetBackground)?;
            self.acquired += 1;
            Ok(BACKGROUND_MODE(1))
        }

        fn set_text_color(&mut self, _color: COLORREF) -> io::Result<COLORREF> {
            self.complete(InjectedPrimaryPaintFailure::SetTextColor)?;
            self.text_colors.push(_color);
            self.acquired += 1;
            Ok(COLORREF(0x0011_2233))
        }

        fn draw_text(&mut self, _text: &str, _rect: &mut RECT, _rtl: bool) -> io::Result<()> {
            self.complete(InjectedPrimaryPaintFailure::Text)
        }

        fn draw_focus(&mut self, _rect: &RECT) -> io::Result<()> {
            self.complete(InjectedPrimaryPaintFailure::Focus)
        }

        fn restore_font(&mut self, _font: HGDIOBJ) -> io::Result<()> {
            self.restore_attempts += 1;
            self.complete(InjectedPrimaryPaintFailure::RestoreFont)
        }

        fn restore_background(&mut self, _mode: BACKGROUND_MODE) -> io::Result<()> {
            self.restore_attempts += 1;
            self.complete(InjectedPrimaryPaintFailure::RestoreBackground)
        }

        fn restore_text_color(&mut self, _color: COLORREF) -> io::Result<()> {
            self.restore_attempts += 1;
            self.complete(InjectedPrimaryPaintFailure::RestoreTextColor)
        }
    }

    impl FailingPrimaryPaintBackend {
        fn complete(&self, stage: InjectedPrimaryPaintFailure) -> io::Result<()> {
            if self.failure == Some(stage) {
                Err(io::Error::other("injected primary paint failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn primary_button_paint_failure_at_every_required_stage_restores_and_uses_native_fallback() {
        let state = primary_button_paint_state(CDIS_DEFAULT | CDIS_FOCUS, true).unwrap();
        let rect = RECT {
            left: 0,
            top: 0,
            right: 120,
            bottom: 40,
        };
        for failure in [
            InjectedPrimaryPaintFailure::DefaultBorder,
            InjectedPrimaryPaintFailure::Border,
            InjectedPrimaryPaintFailure::Surface,
            InjectedPrimaryPaintFailure::SelectFont,
            InjectedPrimaryPaintFailure::SetBackground,
            InjectedPrimaryPaintFailure::SetTextColor,
            InjectedPrimaryPaintFailure::Text,
            InjectedPrimaryPaintFailure::Focus,
            InjectedPrimaryPaintFailure::RestoreTextColor,
            InjectedPrimaryPaintFailure::RestoreBackground,
            InjectedPrimaryPaintFailure::RestoreFont,
        ] {
            let mut backend = FailingPrimaryPaintBackend {
                failure: Some(failure),
                ..Default::default()
            };

            assert!(!paint_primary_button(
                &mut backend,
                rect,
                "Add",
                font_handle(1),
                DialogPalette::for_theme(DialogTheme::Light),
                false,
                state,
            ));
            assert_eq!(backend.restore_attempts, backend.acquired, "{failure:?}");
        }
    }

    #[test]
    fn primary_button_paint_uses_the_semantic_contrasting_text_token() {
        let palette = DialogPalette::for_theme(DialogTheme::Light);
        let state = primary_button_paint_state(Default::default(), true).unwrap();
        let mut backend = FailingPrimaryPaintBackend::default();

        assert!(paint_primary_button(
            &mut backend,
            RECT {
                left: 0,
                top: 0,
                right: 120,
                bottom: 40,
            },
            "Login",
            font_handle(1),
            palette,
            false,
            state,
        ));
        assert_eq!(
            backend.text_colors,
            [COLORREF(palette.primary_text.colorref)]
        );
    }

    #[test]
    fn profile_row_content_padding_matches_shared_outer_padding() {
        for dpi in [96, 120, 144, 168, 192] {
            assert_eq!(
                profile_row_content_padding(dpi),
                crate::windows::design::scale_logical(crate::windows::design::OUTER_PADDING, dpi,)
            );
        }
    }

    #[test]
    fn long_profile_names_reserve_a_non_overlapping_trailing_marker_region_at_every_dpi() {
        for dpi in [96, 120, 144, 168, 192] {
            let width = crate::windows::design::scale_logical(190, dpi);
            let marker_width = crate::windows::design::scale_logical(112, dpi);
            let line = crate::windows::design::LogicalRect::new(
                0,
                0,
                width,
                crate::windows::design::scale_logical(19, dpi),
            );

            for rtl in [false, true] {
                let layout = profile_row_first_line_layout(line, marker_width, dpi, rtl);
                let markers = layout
                    .markers
                    .expect("role markers must have a reserved region");
                assert_eq!(markers.width(), marker_width);
                assert!(!layout.name.intersects(markers));
                if rtl {
                    assert_eq!(markers.left, line.left);
                    assert_eq!(layout.name.right, line.right);
                } else {
                    assert_eq!(markers.right, line.right);
                    assert_eq!(layout.name.left, line.left);
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum InjectedProfileRowPaintFailure {
        Apply,
        Fill,
        Text,
        Focus,
    }

    #[derive(Clone, Debug, PartialEq)]
    enum ProfileRowPaintEvent {
        Apply,
        Fill(ProfileRowFillStage),
        RestoreFill,
        FallbackFill,
        DefaultFont,
        SelectFont(usize),
        SetBackground,
        SetText(COLORREF),
        Measure(String),
        Draw(ProfileRowTextStage, String, RECT, u32),
        Focus,
        RestoreText,
        RestoreBackground,
        RestoreFont,
    }

    struct RecordingProfileRowPaintBackend {
        failure: Option<InjectedProfileRowPaintFailure>,
        fail_fallback: bool,
        marker_width: i32,
        fallback_started: bool,
        events: Vec<ProfileRowPaintEvent>,
    }

    impl RecordingProfileRowPaintBackend {
        fn new(failure: Option<InjectedProfileRowPaintFailure>, marker_width: i32) -> Self {
            Self {
                failure,
                fail_fallback: false,
                marker_width,
                fallback_started: false,
                events: Vec::new(),
            }
        }

        fn custom_fails(&self, failure: InjectedProfileRowPaintFailure) -> bool {
            !self.fallback_started && self.failure == Some(failure)
        }
    }

    impl ProfileRowPaintBackend for RecordingProfileRowPaintBackend {
        fn apply_fill_color(&mut self, _color: COLORREF) -> io::Result<COLORREF> {
            self.events.push(ProfileRowPaintEvent::Apply);
            if self.custom_fails(InjectedProfileRowPaintFailure::Apply) {
                Err(io::Error::other("injected row brush apply failure"))
            } else {
                Ok(COLORREF(0x0011_2233))
            }
        }

        fn fill_custom_rect(&mut self, stage: ProfileRowFillStage, _rect: &RECT) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::Fill(stage));
            if self.custom_fails(InjectedProfileRowPaintFailure::Fill) {
                Err(io::Error::other("injected row FillRect failure"))
            } else {
                Ok(())
            }
        }

        fn restore_fill_color(&mut self, _color: COLORREF) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::RestoreFill);
            Ok(())
        }

        fn fill_system_rect(&mut self, _rect: &RECT, _selected: bool) -> io::Result<COLORREF> {
            self.fallback_started = true;
            self.events.push(ProfileRowPaintEvent::FallbackFill);
            if self.fail_fallback {
                Err(io::Error::other("injected system brush failure"))
            } else {
                Ok(COLORREF(0x00aa_bbcc))
            }
        }

        fn default_gui_font(&mut self) -> io::Result<HFONT> {
            self.events.push(ProfileRowPaintEvent::DefaultFont);
            Ok(font_handle(99))
        }

        fn select_font(&mut self, font: HFONT) -> io::Result<HGDIOBJ> {
            self.events
                .push(ProfileRowPaintEvent::SelectFont(font.0 as usize));
            Ok(HGDIOBJ(77_usize as _))
        }

        fn set_transparent_background(&mut self) -> io::Result<BACKGROUND_MODE> {
            self.events.push(ProfileRowPaintEvent::SetBackground);
            Ok(BACKGROUND_MODE(1))
        }

        fn set_text_color(&mut self, color: COLORREF) -> io::Result<COLORREF> {
            self.events.push(ProfileRowPaintEvent::SetText(color));
            Ok(COLORREF(0x0033_2211))
        }

        fn measure_text_width(&mut self, text: &str) -> io::Result<i32> {
            self.events
                .push(ProfileRowPaintEvent::Measure(text.to_string()));
            Ok(self.marker_width)
        }

        fn draw_text(
            &mut self,
            stage: ProfileRowTextStage,
            text: &str,
            rect: &mut RECT,
            format: DRAW_TEXT_FORMAT,
        ) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::Draw(
                stage,
                text.to_string(),
                *rect,
                format.0,
            ));
            if stage == ProfileRowTextStage::Name
                && self.custom_fails(InjectedProfileRowPaintFailure::Text)
            {
                Err(io::Error::other("injected row DrawText failure"))
            } else {
                Ok(())
            }
        }

        fn draw_focus(&mut self, _rect: &RECT) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::Focus);
            if self.custom_fails(InjectedProfileRowPaintFailure::Focus) {
                Err(io::Error::other("injected row focus failure"))
            } else {
                Ok(())
            }
        }

        fn restore_font(&mut self, _font: HGDIOBJ) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::RestoreFont);
            Ok(())
        }

        fn restore_background(&mut self, _mode: BACKGROUND_MODE) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::RestoreBackground);
            Ok(())
        }

        fn restore_text_color(&mut self, _color: COLORREF) -> io::Result<()> {
            self.events.push(ProfileRowPaintEvent::RestoreText);
            Ok(())
        }
    }

    fn row_paint_fixture() -> (UsageProfileView, super::ProfileManagerRowText, String) {
        let profile = UsageProfileView {
            id: UsageProfileId::System,
            label: "W".repeat(super::PROFILE_LABEL_MAX_UTF16_UNITS),
            summary: "72% remaining".to_string(),
            selected: true,
            login_required: false,
            used_percent: Some(28),
            usage_status: Some(ProfileUsageStatus::Healthy),
            managed: false,
        };
        let copy = super::profile_manager_row_text(&profile, Language::English);
        let accessible = super::profile_manager_accessible_row_text(&profile, Language::English);
        (profile, copy, accessible)
    }

    #[test]
    fn profile_row_custom_apply_fill_text_and_focus_failures_restore_before_stock_fallback() {
        let (profile, copy, accessible) = row_paint_fixture();
        for failure in [
            InjectedProfileRowPaintFailure::Apply,
            InjectedProfileRowPaintFailure::Fill,
            InjectedProfileRowPaintFailure::Text,
            InjectedProfileRowPaintFailure::Focus,
        ] {
            let mut backend = RecordingProfileRowPaintBackend::new(Some(failure), 84);

            assert!(paint_profile_row_with_fallback(
                &mut backend,
                RECT {
                    left: 0,
                    top: 0,
                    right: 220,
                    bottom: 56,
                },
                &profile,
                &copy,
                &accessible,
                ProfileRowPaintResources {
                    dpi: 96,
                    palette: DialogPalette::for_theme(DialogTheme::Light),
                    body_font: font_handle(1),
                },
                false,
                true,
                true,
            ));

            let fallback = backend
                .events
                .iter()
                .position(|event| *event == ProfileRowPaintEvent::FallbackFill)
                .expect("custom failure must enter stock fallback");
            let before_fallback = &backend.events[..fallback];
            assert_eq!(
                backend.events.get(fallback + 1),
                Some(&ProfileRowPaintEvent::DefaultFont)
            );
            assert_eq!(
                backend.events.get(fallback + 2),
                Some(&ProfileRowPaintEvent::SelectFont(99))
            );
            if matches!(
                failure,
                InjectedProfileRowPaintFailure::Text | InjectedProfileRowPaintFailure::Focus
            ) {
                assert!(before_fallback.ends_with(&[
                    ProfileRowPaintEvent::RestoreText,
                    ProfileRowPaintEvent::RestoreBackground,
                    ProfileRowPaintEvent::RestoreFont,
                ]));
            }
            if failure == InjectedProfileRowPaintFailure::Fill {
                assert!(before_fallback.ends_with(&[ProfileRowPaintEvent::RestoreFill]));
            }
            assert!(backend.events.iter().any(|event| matches!(
                event,
                ProfileRowPaintEvent::Draw(ProfileRowTextStage::Fallback, text, _, _)
                    if text == &accessible
            )));
        }
    }

    #[test]
    fn profile_row_is_not_handled_when_custom_and_stock_fallback_both_fail() {
        let (profile, copy, accessible) = row_paint_fixture();
        let mut backend =
            RecordingProfileRowPaintBackend::new(Some(InjectedProfileRowPaintFailure::Apply), 84);
        backend.fail_fallback = true;

        assert!(!paint_profile_row_with_fallback(
            &mut backend,
            RECT {
                left: 0,
                top: 0,
                right: 220,
                bottom: 56,
            },
            &profile,
            &copy,
            &accessible,
            ProfileRowPaintResources {
                dpi: 96,
                palette: DialogPalette::for_theme(DialogTheme::Light),
                body_font: font_handle(1),
            },
            false,
            false,
            false,
        ));
    }

    #[test]
    fn max_length_ltr_and_arabic_rows_draw_markers_separately_without_ellipsis() {
        let (profile, copy, accessible) = row_paint_fixture();
        for dpi in [96, 120, 144, 168, 192] {
            for rtl in [false, true] {
                let marker_width = crate::windows::design::scale_logical(92, dpi);
                let mut backend = RecordingProfileRowPaintBackend::new(None, marker_width);
                let row_width = crate::windows::design::scale_logical(190, dpi);
                assert!(paint_profile_row_with_fallback(
                    &mut backend,
                    RECT {
                        left: 0,
                        top: 0,
                        right: row_width,
                        bottom: crate::windows::design::scale_logical(56, dpi),
                    },
                    &profile,
                    &copy,
                    &accessible,
                    ProfileRowPaintResources {
                        dpi,
                        palette: DialogPalette::for_theme(DialogTheme::Light),
                        body_font: font_handle(1),
                    },
                    rtl,
                    false,
                    false,
                ));

                let draws = backend
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        ProfileRowPaintEvent::Draw(stage, _, rect, format) => {
                            Some((*stage, *rect, *format))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let (_, name, name_format) = draws
                    .iter()
                    .find(|(stage, _, _)| *stage == ProfileRowTextStage::Name)
                    .copied()
                    .unwrap();
                let (_, markers, marker_format) = draws
                    .iter()
                    .find(|(stage, _, _)| *stage == ProfileRowTextStage::Markers)
                    .copied()
                    .unwrap();
                assert!(name.right <= markers.left || markers.right <= name.left);
                assert_ne!(name_format & DT_END_ELLIPSIS.0, 0);
                assert_eq!(marker_format & DT_END_ELLIPSIS.0, 0);
                if rtl {
                    assert_eq!(markers.left, crate::windows::design::scale_logical(16, dpi));
                    assert_ne!(marker_format & (DT_RTLREADING | DT_RIGHT).0, 0);
                } else {
                    assert!(markers.right > name.right);
                    assert_eq!(marker_format & (DT_RTLREADING | DT_RIGHT).0, 0);
                }
            }
        }
    }

    #[test]
    fn profile_row_visual_state_uses_native_selection_and_focus() {
        let profile = UsageProfileView {
            id: UsageProfileId::Managed(1),
            label: "Work".to_string(),
            summary: "Ready".to_string(),
            selected: false,
            login_required: false,
            used_percent: Some(42),
            usage_status: Some(ProfileUsageStatus::Healthy),
            managed: true,
        };

        let neutral = profile_row_visual_state(&profile, false, false);
        assert_eq!(neutral.surface, ProfileRowSurfaceRole::Neutral);
        assert!(!neutral.focused);

        let selected = profile_row_visual_state(&profile, true, true);
        assert_eq!(selected.surface, ProfileRowSurfaceRole::Selected);
        assert!(selected.focused);
    }

    #[test]
    fn profile_row_visual_state_preserves_typed_status_and_clamps_percent() {
        for (percent, status, expected_percent) in [
            (24, ProfileUsageStatus::Healthy, 24),
            (81, ProfileUsageStatus::Warning, 81),
            (255, ProfileUsageStatus::Critical, 100),
        ] {
            let profile = UsageProfileView {
                id: UsageProfileId::Managed(1),
                label: "Work".to_string(),
                summary: "Ready".to_string(),
                selected: false,
                login_required: false,
                used_percent: Some(percent),
                usage_status: Some(status),
                managed: true,
            };

            assert_eq!(
                profile_row_visual_state(&profile, false, false).progress,
                Some((expected_percent, status))
            );
        }
    }

    #[test]
    fn profile_row_visual_state_requires_complete_usage_and_exposes_text_marker_flags() {
        let profile = UsageProfileView {
            id: UsageProfileId::System,
            label: "Main".to_string(),
            summary: "Login required".to_string(),
            selected: true,
            login_required: true,
            used_percent: None,
            usage_status: None,
            managed: false,
        };

        let visual = profile_row_visual_state(&profile, false, false);
        assert_eq!(visual.progress, None);
        assert!(visual.system_marker);
        assert!(visual.current_marker);

        let incomplete_usage = UsageProfileView {
            login_required: false,
            used_percent: Some(0),
            ..profile
        };
        assert_eq!(
            profile_row_visual_state(&incomplete_usage, false, false).progress,
            None
        );
    }

    #[test]
    fn profile_row_translucent_track_composites_over_row_surface() {
        assert_eq!(
            composite_dialog_color(
                DialogColor::translucent(0x0000_0000, 36),
                DialogColor::opaque(0x00ff_ffff)
            ),
            0x00db_dbdb
        );
    }

    #[test]
    fn profile_row_selected_surface_is_a_subtle_green_tint_on_both_themes() {
        let light = DialogPalette::for_theme(DialogTheme::Light);
        assert_eq!(
            profile_row_surface_color(light, ProfileRowSurfaceRole::Neutral),
            0x00ff_ffff
        );
        assert_eq!(
            profile_row_surface_color(light, ProfileRowSurfaceRole::Selected),
            0x00f4_fbf1
        );

        let dark = DialogPalette::for_theme(DialogTheme::Dark);
        assert_eq!(
            profile_row_surface_color(dark, ProfileRowSurfaceRole::Neutral),
            0x0026_2626
        );
        assert_eq!(
            profile_row_surface_color(dark, ProfileRowSurfaceRole::Selected),
            0x002c_3329
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ResourceEvent {
        Created(usize),
        Applied {
            body_font: usize,
            heading_font: usize,
        },
        Deleted(usize),
    }

    #[derive(Default)]
    struct ResourceCalls {
        font_faces: Vec<DialogFontFace>,
        deleted: Vec<usize>,
        events: Vec<ResourceEvent>,
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
            let handle = self.fonts.pop_front().unwrap_or_default();
            self.calls
                .borrow_mut()
                .events
                .push(ResourceEvent::Created(handle));
            font_handle(handle)
        }

        fn stock_font(&mut self) -> HFONT {
            font_handle(self.stock_font)
        }

        fn create_brush(&mut self, _colorref: u32) -> HBRUSH {
            let handle = self.brushes.pop_front().unwrap_or_default();
            self.calls
                .borrow_mut()
                .events
                .push(ResourceEvent::Created(handle));
            brush_handle(handle)
        }

        fn delete_object(&mut self, object: HGDIOBJ) {
            let handle = object.0 as usize;
            let mut calls = self.calls.borrow_mut();
            calls.deleted.push(handle);
            calls.events.push(ResourceEvent::Deleted(handle));
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
    fn initial_null_brushes_are_unowned_and_leave_background_erase_to_windows() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([11, 12]),
            brushes: VecDeque::from([0, 0]),
            stock_font: 99,
        };
        let resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));

        assert!(resources.background_brush.0.is_null());
        assert!(resources.surface_brush.0.is_null());
        assert!(!resources.owns_background_brush);
        assert!(!resources.owns_surface_brush);
        assert!(!erase_dialog_background_with(&resources, |_| {
            panic!("a null brush must not reach FillRect")
        }));

        drop(resources);
        assert_eq!(calls.borrow().deleted, [11, 12]);
        assert!(!calls.borrow().deleted.contains(&0));
    }

    #[test]
    fn incomplete_rebuild_brushes_are_released_without_discarding_valid_active_resources() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([11, 12, 31, 32]),
            brushes: VecDeque::from([21, 22, 41, 0]),
            stock_font: 99,
        };
        let mut resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));
        calls.borrow_mut().events.clear();

        unsafe {
            update_dialog_visual_resources(
                std::ptr::addr_of_mut!(resources),
                144,
                DialogTheme::Light,
                |visual| {
                    calls.borrow_mut().events.push(ResourceEvent::Applied {
                        body_font: visual.body_font.0 as usize,
                        heading_font: visual.heading_font.0 as usize,
                    });
                },
            );
        }

        assert_eq!(resources.dpi, 96);
        assert_eq!(resources.body_font.0 as usize, 11);
        assert_eq!(resources.heading_font.0 as usize, 12);
        assert_eq!(resources.background_brush.0 as usize, 21);
        assert_eq!(resources.surface_brush.0 as usize, 22);
        assert_eq!(
            calls.borrow().events,
            [
                ResourceEvent::Created(31),
                ResourceEvent::Created(32),
                ResourceEvent::Created(41),
                ResourceEvent::Created(0),
                ResourceEvent::Deleted(31),
                ResourceEvent::Deleted(32),
                ResourceEvent::Deleted(41),
                ResourceEvent::Applied {
                    body_font: 11,
                    heading_font: 12,
                },
            ]
        );
        assert!(!calls.borrow().deleted.contains(&0));

        drop(resources);
        let mut deleted = calls.borrow().deleted.clone();
        deleted.sort_unstable();
        assert_eq!(deleted, [11, 12, 21, 22, 31, 32, 41]);
    }

    #[test]
    fn background_erase_is_handled_only_when_fill_succeeds() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls,
            fonts: VecDeque::from([11, 12]),
            brushes: VecDeque::from([21, 22]),
            stock_font: 99,
        };
        let resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));

        assert!(!erase_dialog_background_with(&resources, |_| {
            Err(io::Error::other("injected FillRect failure"))
        }));
        assert!(erase_dialog_background_with(&resources, |_| Ok(())));
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

        unsafe {
            update_dialog_visual_resources(
                std::ptr::addr_of_mut!(resources),
                144,
                DialogTheme::Light,
                |_| {},
            );
        }
        drop(resources);

        let mut deleted = calls.borrow().deleted.clone();
        deleted.sort_unstable();
        assert_eq!(deleted, [11, 21, 22, 31, 32, 41, 42]);
        assert!(!deleted.contains(&99));
    }

    #[test]
    fn rebuild_applies_staged_fonts_before_deleting_old_handles() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([11, 12, 31, 32]),
            brushes: VecDeque::from([21, 22, 41, 42]),
            stock_font: 99,
        };
        let mut resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));
        calls.borrow_mut().events.clear();

        let outcome = unsafe {
            update_dialog_visual_resources(
                std::ptr::addr_of_mut!(resources),
                144,
                DialogTheme::Light,
                |visual| {
                    calls.borrow_mut().events.push(ResourceEvent::Applied {
                        body_font: visual.body_font.0 as usize,
                        heading_font: visual.heading_font.0 as usize,
                    });
                },
            )
        };

        assert_eq!(outcome, DialogVisualUpdateOutcome::Applied);
        assert_eq!(
            calls.borrow().events,
            [
                ResourceEvent::Created(31),
                ResourceEvent::Created(32),
                ResourceEvent::Created(41),
                ResourceEvent::Created(42),
                ResourceEvent::Applied {
                    body_font: 31,
                    heading_font: 32,
                },
                ResourceEvent::Deleted(11),
                ResourceEvent::Deleted(12),
                ResourceEvent::Deleted(21),
                ResourceEvent::Deleted(22),
            ]
        );
    }

    #[test]
    fn reentrant_visual_update_is_coalesced_to_the_latest_safe_state() {
        let calls = Rc::new(RefCell::new(ResourceCalls::default()));
        let backend = RecordingResourceBackend {
            calls: Rc::clone(&calls),
            fonts: VecDeque::from([11, 12, 31, 32, 51, 52]),
            brushes: VecDeque::from([21, 22, 41, 42, 61, 62]),
            stock_font: 99,
        };
        let mut resources =
            DialogVisualResources::new_with_backend(96, DialogTheme::Dark, Box::new(backend));
        let resources_pointer = std::ptr::addr_of_mut!(resources);
        let applied = Rc::new(RefCell::new(Vec::new()));
        let nested_outcomes = Rc::new(RefCell::new(Vec::new()));

        let outcome = unsafe {
            update_dialog_visual_resources(resources_pointer, 144, DialogTheme::Light, |visual| {
                applied
                    .borrow_mut()
                    .push((visual.body_font.0 as usize, visual.heading_font.0 as usize));
                if applied.borrow().len() == 1 {
                    nested_outcomes
                        .borrow_mut()
                        .push(update_dialog_visual_resources(
                            resources_pointer,
                            192,
                            DialogTheme::Dark,
                            |_| panic!("coalesced update must not apply inside the nested call"),
                        ));
                }
            })
        };

        assert_eq!(outcome, DialogVisualUpdateOutcome::Applied);
        assert_eq!(
            nested_outcomes.borrow().as_slice(),
            [DialogVisualUpdateOutcome::Coalesced]
        );
        assert_eq!(applied.borrow().as_slice(), [(31, 32), (51, 52)]);
        assert_eq!(resources.dpi, 192);
        assert_eq!(
            resources.palette,
            DialogPalette::for_theme(DialogTheme::Dark)
        );
        assert_eq!(resources.body_font.0 as usize, 51);
        assert_eq!(resources.heading_font.0 as usize, 52);
        assert_eq!(resources.profile_row_height(), 112);
        assert!(!resources.update_in_progress);
        assert!(resources.pending_update.is_none());
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
            name_label: HWND::default(),
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
            name_label: HWND::default(),
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
