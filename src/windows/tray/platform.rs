use std::{
    io,
    pin::Pin,
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread,
};

use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::{
            CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetStockObject,
            MonitorFromPoint, RoundRect, SelectObject, SetBkMode, SetTextColor,
            CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DT_END_ELLIPSIS, DT_LEFT,
            DT_NOPREFIX, DT_RIGHT, DT_RTLREADING, DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_NORMAL,
            HBRUSH, HGDIOBJ, MONITOR_DEFAULTTONEAREST, NULL_PEN, OUT_DEFAULT_PRECIS, PROOF_QUALITY,
            TRANSPARENT,
        },
        UI::{
            Controls::{
                DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_SELECTED, ODT_MENU,
            },
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Shell::{
                Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
            },
            WindowsAndMessaging::{
                AppendMenuW, CreateIcon, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
                InsertMenuItemW, PostMessageW, SetForegroundWindow, SetMenuInfo, TrackPopupMenu,
                HICON, HMENU, MENUINFO, MENUITEMINFOW, MFS_CHECKED, MFS_DISABLED, MFS_GRAYED,
                MFT_OWNERDRAW, MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR,
                MF_STRING, MIIM_DATA, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU,
                MIM_BACKGROUND, MIM_STYLE, MNS_CHECKORBMP, TPM_NONOTIFY, TPM_RETURNCMD,
                TPM_RIGHTBUTTON, WM_APP, WM_NULL,
            },
        },
    },
};

use super::{
    super::{
        popup::{menu_item_height, menu_item_width, popup_palette, MenuItemKind, PopupPalette},
        theme,
        widget::logical_to_physical,
        UiAction, UiSettings,
    },
    TrayMenuEntry,
};
use crate::diagnostics::{DiagnosticLogger, SafeDiagnostic};

pub(crate) const TRAY_CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

struct MenuItemVisual {
    text: Vec<u16>,
    kind: MenuItemKind,
    checked: bool,
    disabled: bool,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
    glyph: &'static str,
}

struct MenuRenderState {
    items: Vec<Pin<Box<MenuItemVisual>>>,
    background: HBRUSH,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
}

impl MenuRenderState {
    unsafe fn new(light: bool, dpi: u32, rtl: bool) -> Self {
        let palette = popup_palette(light);
        Self {
            items: Vec::new(),
            background: CreateSolidBrush(COLORREF(palette.background)),
            palette,
            dpi,
            rtl,
        }
    }
}

impl Drop for MenuRenderState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.background.0));
        }
    }
}

/// 셸 명령을 하나의 워커에서 직렬화하고 대기 명령을 최신 값으로 합치는 실행기입니다.
struct CoalescingWorker<C> {
    pending: Arc<Mutex<C>>,
    trigger: SyncSender<()>,
    shutdown: Arc<Mutex<Option<SyncSender<()>>>>,
}

impl<C: Clone + Send + 'static> CoalescingWorker<C> {
    /// 느릴 수 있는 최신 명령 하나를 전용 워커에서 실행합니다.
    ///
    /// `submit`은 진행 중인 명령을 기다리지 않습니다. 대기 중인 여러 명령은 마지막 값으로 합쳐지며,
    /// 외부 호출이 정지한 경우에도 워커 스레드를 추가로 만들지 않습니다.
    fn spawn<H>(
        initial: C,
        handler_factory: impl FnOnce() -> H + Send + 'static,
    ) -> io::Result<Self>
    where
        H: FnMut(C) + 'static,
    {
        let pending = Arc::new(Mutex::new(initial));
        let worker_pending = Arc::clone(&pending);
        let shutdown = Arc::new(Mutex::new(None::<SyncSender<()>>));
        let worker_shutdown = Arc::clone(&shutdown);
        let (trigger, commands) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("tray-shell".to_string())
            .spawn(move || {
                let mut handler = Some(handler_factory());
                while commands.recv().is_ok() {
                    if let Some(completion) = worker_shutdown
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take()
                    {
                        drop(handler.take());
                        let _ = completion.send(());
                        break;
                    }
                    let command = worker_pending
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .clone();
                    handler.as_mut().expect("handler exists before shutdown")(command);
                }
            })?;
        Ok(Self {
            pending,
            trigger,
            shutdown,
        })
    }

    fn submit(&self, command: C) {
        *self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = command;
        let _ = self.trigger.try_send(());
    }

    /// 현재 셸 호출이 끝나고 워커가 보유 리소스를 해제했을 때 완료 신호를 반환합니다.
    fn begin_shutdown(&self) -> mpsc::Receiver<()> {
        let (completion, receiver) = mpsc::sync_channel(1);
        *self
            .shutdown
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(completion);
        let _ = self.trigger.try_send(());
        receiver
    }
}

/// 트레이 워커에 전달하는 최신 아이콘 표시 명령입니다.
#[derive(Clone)]
struct TrayUpdate {
    percent: Option<f64>,
    tip: String,
    restore: bool,
}

/// Explorer 셸 호출과 UI 메시지 처리를 분리하는 비동기 트레이 아이콘입니다.
pub(crate) struct AsyncTrayIcon {
    worker: CoalescingWorker<TrayUpdate>,
}

impl AsyncTrayIcon {
    /// Explorer 셸 호출을 전용 워커에서 실행하는 트레이 아이콘을 만듭니다.
    ///
    /// `owner`는 트레이 콜백을 받을 UI 창입니다. 생성·갱신·복구·삭제 셸 호출은 UI 스레드를
    /// 차단하지 않으며, Explorer가 응답하지 않으면 마지막 명령 하나만 대기합니다.
    pub(crate) fn new(owner: HWND, percent: Option<f64>, tip: &str) -> io::Result<Self> {
        let initial = TrayUpdate {
            percent,
            tip: tip.to_string(),
            restore: true,
        };
        let owner_value = owner.0 as usize;
        let worker = CoalescingWorker::spawn(initial.clone(), move || {
            let owner = HWND(owner_value as *mut _);
            let mut tray: Option<TrayIcon> = None;
            move |update: TrayUpdate| unsafe {
                let result = match tray.as_mut() {
                    Some(tray) if update.restore => tray.restore(update.percent, &update.tip),
                    Some(tray) => tray
                        .update(update.percent, &update.tip)
                        .or_else(|_| tray.restore(update.percent, &update.tip)),
                    None => TrayIcon::new(owner, update.percent, &update.tip).map(|created| {
                        tray = Some(created);
                    }),
                };
                let _ = result;
            }
        })?;
        worker.submit(initial);
        Ok(Self { worker })
    }

    /// 최신 상태로 트레이 아이콘 갱신을 예약하고 즉시 반환합니다.
    pub(crate) fn update(&self, percent: Option<f64>, tip: &str) {
        self.submit(percent, tip, false);
    }

    /// Explorer 재시작 후 트레이 아이콘 복구를 예약하고 즉시 반환합니다.
    pub(crate) fn restore(&self, percent: Option<f64>, tip: &str) {
        self.submit(percent, tip, true);
    }

    /// 아이콘 삭제를 워커에서 완료한 뒤 수신기로 알립니다.
    ///
    /// 반환된 수신기는 `TrayIcon`의 drop이 끝난 후에만 값을 받습니다. 호출자는 owner 창을
    /// 파괴하기 전에 이 신호를 확인해야 합니다.
    pub(crate) fn begin_shutdown(&self) -> mpsc::Receiver<()> {
        self.worker.begin_shutdown()
    }

    fn submit(&self, percent: Option<f64>, tip: &str, restore: bool) {
        self.worker.submit(TrayUpdate {
            percent,
            tip: tip.to_string(),
            restore,
        });
    }
}

/// 알림 영역 아이콘과 동적 미터 아이콘의 소유자입니다.
pub(crate) struct TrayIcon {
    owner: HWND,
    icon: HICON,
    added: bool,
}

impl TrayIcon {
    pub(crate) unsafe fn new(owner: HWND, percent: Option<f64>, tip: &str) -> io::Result<Self> {
        let icon = meter_icon(percent)?;
        let mut tray = Self {
            owner,
            icon,
            added: false,
        };
        tray.add(tip)?;
        Ok(tray)
    }

    pub(crate) unsafe fn restore(&mut self, percent: Option<f64>, tip: &str) -> io::Result<()> {
        self.added = false;
        self.replace_icon(percent)?;
        self.add(tip)
    }

    pub(crate) unsafe fn update(&mut self, percent: Option<f64>, tip: &str) -> io::Result<()> {
        self.replace_icon(percent)?;
        let data = notify_data(self.owner, self.icon, tip);
        if Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(crate) unsafe fn show_menu(
        owner: HWND,
        settings: &UiSettings,
        reset_credits_text: Option<&str>,
        fluent_style: bool,
    ) -> Option<UiAction> {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok()?;
        let model = super::tray_menu_model(settings);
        let mut menu = CreatePopupMenu().ok()?;
        let mut render_state = fluent_style.then(|| {
            MenuRenderState::new(
                theme::system_uses_light_theme(),
                menu_dpi(point),
                matches!(settings.resolved_language, crate::Language::Arabic),
            )
        });
        let fluent_ready = render_state.as_mut().is_some_and(|render| {
            populate_owner_draw_menu(menu, &model.entries, reset_credits_text, render).is_some()
        });
        if !fluent_ready {
            if fluent_style {
                let _ = DiagnosticLogger::new().record_safe(SafeDiagnostic::PopupRender {
                    surface: "tray_menu",
                    stage: "build",
                    error_code: None,
                });
            }
            let _ = DestroyMenu(menu);
            render_state = None;
            menu = CreatePopupMenu().ok()?;
            populate_native_menu(menu, &model.entries, reset_credits_text)?;
        }
        let result = {
            let _ = SetForegroundWindow(owner);
            let command = TrackPopupMenu(
                menu,
                TPM_NONOTIFY | TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                owner,
                None,
            );
            (command.0 > 0)
                .then(|| model.action(command.0 as u16))
                .flatten()
        };
        let _ = PostMessageW(Some(owner), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
        drop(render_state);
        result
    }

    /// `WM_MEASUREITEM`의 owner-draw 메뉴 항목 크기를 DPI에 맞게 제공합니다.
    ///
    /// `lparam`은 Windows가 전달한 `MEASUREITEMSTRUCT` 포인터여야 합니다. 이 앱이 만든 메뉴가
    /// 아니면 `false`를 반환하며 구조체를 변경하지 않습니다.
    pub(crate) unsafe fn measure_menu_item(lparam: LPARAM) -> bool {
        if lparam.0 == 0 {
            return false;
        }
        let item = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
        if item.CtlType != ODT_MENU || item.itemData == 0 {
            return false;
        }
        let visual = &*(item.itemData as *const MenuItemVisual);
        item.itemHeight = logical_to_physical(menu_item_height(visual.kind), visual.dpi) as u32;
        item.itemWidth = logical_to_physical(
            menu_item_width(visual.text.len().saturating_sub(1)),
            visual.dpi,
        ) as u32;
        true
    }

    /// `WM_DRAWITEM`의 owner-draw 메뉴 항목을 Fluent Compact 팔레트로 그립니다.
    ///
    /// `lparam`은 Windows가 전달한 `DRAWITEMSTRUCT` 포인터여야 합니다. 항목 데이터가 이 앱의
    /// 렌더 모델이 아니면 `false`를 반환해 기본 처리가 계속되도록 합니다.
    pub(crate) unsafe fn draw_menu_item(lparam: LPARAM) -> bool {
        if lparam.0 == 0 {
            return false;
        }
        let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
        if item.CtlType != ODT_MENU || item.itemData == 0 {
            return false;
        }
        draw_owner_item(item, &*(item.itemData as *const MenuItemVisual));
        true
    }

    unsafe fn add(&mut self, tip: &str) -> io::Result<()> {
        let mut data = notify_data(self.owner, self.icon, tip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            return Err(io::Error::last_os_error());
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
        self.added = true;
        Ok(())
    }

    unsafe fn replace_icon(&mut self, percent: Option<f64>) -> io::Result<()> {
        let next = meter_icon(percent)?;
        let previous = std::mem::replace(&mut self.icon, next);
        let _ = DestroyIcon(previous);
        Ok(())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            if self.added {
                let data = notify_data(self.owner, self.icon, "");
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            let _ = DestroyIcon(self.icon);
        }
    }
}

fn notify_data(owner: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: owner,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK,
        hIcon: icon,
        ..Default::default()
    };
    for (target, source) in data.szTip.iter_mut().take(127).zip(tip.encode_utf16()) {
        *target = source;
    }
    data
}

unsafe fn meter_icon(percent: Option<f64>) -> io::Result<HICON> {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 16;
    const BYTES_PER_ROW: usize = 2;
    let mut xor = [0_u8; HEIGHT * BYTES_PER_ROW];
    let and = [0_u8; HEIGHT * BYTES_PER_ROW];
    let percent = percent.filter(|value| value.is_finite()).unwrap_or(0.0);
    let fill = ((percent.clamp(0.0, 100.0) / 100.0) * 12.0).round() as usize;
    for y in 1..15 {
        for x in 2..14 {
            let border = x == 2 || x == 13 || y == 1 || y == 14;
            let filled = y >= 14_usize.saturating_sub(fill);
            if border || filled {
                xor[y * BYTES_PER_ROW + x / 8] |= 1 << (7 - x % 8);
            }
        }
    }
    CreateIcon(
        None,
        WIDTH as i32,
        HEIGHT as i32,
        1,
        1,
        and.as_ptr(),
        xor.as_ptr(),
    )
    .map_err(|_| io::Error::last_os_error())
}

/// 검증된 순수 메뉴 트리를 지정한 Win32 팝업 메뉴에 재귀적으로 추가합니다.
///
/// `menu`는 호출자가 소유한 유효한 팝업 메뉴여야 합니다. 성공적으로 연결된 하위 메뉴의
/// 소유권은 `menu`로 이전되며, 연결 전에 실패한 하위 메뉴는 이 함수가 정리합니다.
unsafe fn populate_native_menu(
    menu: HMENU,
    entries: &[TrayMenuEntry],
    reset_credits_text: Option<&str>,
) -> Option<()> {
    if let Some(text) = reset_credits_text {
        add_info_banner(menu, text)?;
        separator(menu)?;
    }
    append_entries(menu, entries)
}

unsafe fn populate_owner_draw_menu(
    menu: HMENU,
    entries: &[TrayMenuEntry],
    reset_credits_text: Option<&str>,
    render: &mut MenuRenderState,
) -> Option<()> {
    configure_owner_draw_menu(menu, render)?;
    if let Some(text) = reset_credits_text {
        add_owner_item(menu, 0, text, MenuItemKind::Info, false, true, None, render)?;
        separator(menu)?;
    }
    append_owner_entries(menu, entries, render)
}

unsafe fn configure_owner_draw_menu(menu: HMENU, render: &MenuRenderState) -> Option<()> {
    let info = MENUINFO {
        cbSize: std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND | MIM_STYLE,
        dwStyle: MNS_CHECKORBMP,
        hbrBack: render.background,
        ..Default::default()
    };
    SetMenuInfo(menu, &info).ok().map(|_| ())
}

unsafe fn append_owner_entries(
    menu: HMENU,
    entries: &[TrayMenuEntry],
    render: &mut MenuRenderState,
) -> Option<()> {
    for entry in entries {
        match entry {
            TrayMenuEntry::Command(command) => add_owner_item(
                menu,
                command.id,
                &command.label,
                MenuItemKind::Command,
                command.checked,
                false,
                None,
                render,
            )?,
            TrayMenuEntry::Submenu(submenu) => {
                let child = CreatePopupMenu().ok()?;
                if configure_owner_draw_menu(child, render).is_none()
                    || append_owner_entries(child, &submenu.entries, render).is_none()
                    || add_owner_item(
                        menu,
                        0,
                        &submenu.label,
                        MenuItemKind::Submenu,
                        false,
                        false,
                        Some(child),
                        render,
                    )
                    .is_none()
                {
                    let _ = DestroyMenu(child);
                    return None;
                }
            }
            TrayMenuEntry::Separator => separator(menu)?,
        }
    }
    Some(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn add_owner_item(
    menu: HMENU,
    id: u16,
    text: &str,
    kind: MenuItemKind,
    checked: bool,
    disabled: bool,
    submenu: Option<HMENU>,
    render: &mut MenuRenderState,
) -> Option<()> {
    let glyph = match kind {
        MenuItemKind::Info => "\u{E946}",
        MenuItemKind::Submenu => "\u{E712}",
        MenuItemKind::Command => command_glyph(id, checked),
    };
    let mut visual = Box::pin(MenuItemVisual {
        text: text.encode_utf16().chain(Some(0)).collect(),
        kind,
        checked,
        disabled,
        palette: render.palette,
        dpi: render.dpi,
        rtl: render.rtl,
        glyph,
    });
    let state = if disabled {
        MFS_DISABLED | MFS_GRAYED
    } else if checked {
        MFS_CHECKED
    } else {
        Default::default()
    };
    let mut mask = MIIM_FTYPE | MIIM_ID | MIIM_STATE | MIIM_DATA | MIIM_STRING;
    if submenu.is_some() {
        mask |= MIIM_SUBMENU;
    }
    let info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: mask,
        fType: MFT_OWNERDRAW,
        fState: state,
        wID: u32::from(id),
        hSubMenu: submenu.unwrap_or_default(),
        dwItemData: (visual.as_ref().get_ref() as *const MenuItemVisual) as usize,
        dwTypeData: PWSTR(visual.as_mut().get_mut().text.as_mut_ptr()),
        cch: visual.text.len().saturating_sub(1) as u32,
        ..Default::default()
    };
    InsertMenuItemW(menu, u32::MAX, true, &info).ok()?;
    render.items.push(visual);
    Some(())
}

const fn command_glyph(id: u16, checked: bool) -> &'static str {
    if checked {
        return "\u{E73E}";
    }
    match id {
        crate::windows::MENU_REFRESH | crate::windows::MENU_AUTH_REFRESH => "\u{E72C}",
        crate::windows::MENU_MANAGE_USAGE_PROFILES | crate::windows::MENU_LOGIN => "\u{E77B}",
        crate::windows::MENU_DIAGNOSTICS => "\u{E9D9}",
        crate::windows::MENU_UPDATE_CHECK => "\u{E895}",
        crate::windows::MENU_EXIT => "\u{E8BB}",
        _ => "\u{E8B8}",
    }
}

unsafe fn draw_owner_item(item: &DRAWITEMSTRUCT, visual: &MenuItemVisual) {
    let rect = item.rcItem;
    let selected = has_item_state(item, ODS_SELECTED)
        && !has_item_state(item, ODS_DISABLED)
        && !has_item_state(item, ODS_GRAYED);
    fill_menu_rect(item.hDC, rect, visual.palette.background);
    if selected {
        let inset = logical_to_physical(4, visual.dpi);
        let radius = logical_to_physical(7, visual.dpi);
        let brush = CreateSolidBrush(COLORREF(visual.palette.selection));
        let old_brush = SelectObject(item.hDC, HGDIOBJ(brush.0));
        let old_pen = SelectObject(item.hDC, GetStockObject(NULL_PEN));
        let _ = RoundRect(
            item.hDC,
            rect.left + inset,
            rect.top + logical_to_physical(2, visual.dpi),
            rect.right - inset,
            rect.bottom - logical_to_physical(2, visual.dpi),
            radius,
            radius,
        );
        SelectObject(item.hDC, old_pen);
        SelectObject(item.hDC, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
    let icon_inset = logical_to_physical(12, visual.dpi);
    let text_inset = logical_to_physical(44, visual.dpi);
    let trailing = logical_to_physical(28, visual.dpi);
    let text_rect = if visual.rtl {
        RECT {
            left: rect.left + trailing,
            top: rect.top,
            right: rect.right - text_inset,
            bottom: rect.bottom,
        }
    } else {
        RECT {
            left: rect.left + text_inset,
            top: rect.top,
            right: rect.right - trailing,
            bottom: rect.bottom,
        }
    };
    let icon_rect = if visual.rtl {
        RECT {
            left: rect.right - icon_inset - logical_to_physical(20, visual.dpi),
            right: rect.right - icon_inset,
            top: rect.top,
            bottom: rect.bottom,
        }
    } else {
        RECT {
            left: rect.left + icon_inset,
            right: rect.left + icon_inset + logical_to_physical(20, visual.dpi),
            top: rect.top,
            bottom: rect.bottom,
        }
    };
    let disabled =
        visual.disabled || has_item_state(item, ODS_DISABLED) || has_item_state(item, ODS_GRAYED);
    draw_menu_glyph(
        item.hDC,
        visual.glyph,
        icon_rect,
        if visual.checked {
            visual.palette.accent
        } else if disabled {
            visual.palette.secondary_text
        } else {
            visual.palette.text
        },
        visual.dpi,
        false,
    );
    let mut label = visual.text[..visual.text.len().saturating_sub(1)].to_vec();
    draw_menu_label(
        item.hDC,
        &mut label,
        text_rect,
        if disabled {
            visual.palette.secondary_text
        } else {
            visual.palette.text
        },
        visual.dpi,
        visual.rtl,
    );
    if matches!(visual.kind, MenuItemKind::Submenu) {
        let arrow = if visual.rtl { "\u{E76B}" } else { "\u{E76C}" };
        let arrow_rect = if visual.rtl {
            RECT {
                left: rect.left + icon_inset,
                right: rect.left + icon_inset + trailing,
                top: rect.top,
                bottom: rect.bottom,
            }
        } else {
            RECT {
                left: rect.right - trailing,
                right: rect.right - icon_inset,
                top: rect.top,
                bottom: rect.bottom,
            }
        };
        draw_menu_glyph(
            item.hDC,
            arrow,
            arrow_rect,
            visual.palette.secondary_text,
            visual.dpi,
            visual.rtl,
        );
    }
}

fn has_item_state(item: &DRAWITEMSTRUCT, state: windows::Win32::UI::Controls::ODS_FLAGS) -> bool {
    item.itemState.0 & state.0 != 0
}

unsafe fn fill_menu_rect(dc: windows::Win32::Graphics::Gdi::HDC, rect: RECT, color: u32) {
    let brush = CreateSolidBrush(COLORREF(color));
    FillRect(dc, &rect, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn draw_menu_label(
    dc: windows::Win32::Graphics::Gdi::HDC,
    text: &mut [u16],
    mut rect: RECT,
    color: u32,
    dpi: u32,
    rtl: bool,
) {
    let font = menu_font(12, dpi, w!("Segoe UI Variable"));
    let old_font = SelectObject(dc, HGDIOBJ(font.0));
    let _ = SetBkMode(dc, TRANSPARENT);
    let _ = SetTextColor(dc, COLORREF(color));
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    let _ = DrawTextW(
        dc,
        text,
        &mut rect,
        alignment | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(font.0));
}

unsafe fn draw_menu_glyph(
    dc: windows::Win32::Graphics::Gdi::HDC,
    glyph: &str,
    mut rect: RECT,
    color: u32,
    dpi: u32,
    rtl: bool,
) {
    let font = menu_font(14, dpi, w!("Segoe MDL2 Assets"));
    let old_font = SelectObject(dc, HGDIOBJ(font.0));
    let _ = SetBkMode(dc, TRANSPARENT);
    let _ = SetTextColor(dc, COLORREF(color));
    let mut glyph: Vec<u16> = glyph.encode_utf16().collect();
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    let _ = DrawTextW(
        dc,
        &mut glyph,
        &mut rect,
        alignment | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(HGDIOBJ(font.0));
}

unsafe fn menu_font(size: i32, dpi: u32, face: PCWSTR) -> windows::Win32::Graphics::Gdi::HFONT {
    CreateFontW(
        -logical_to_physical(size, dpi),
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
        face,
    )
}

unsafe fn menu_dpi(point: POINT) -> u32 {
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut dpi_x = 96_u32;
    let mut dpi_y = 96_u32;
    let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    dpi_x.max(96)
}

unsafe fn append_entries(menu: HMENU, entries: &[TrayMenuEntry]) -> Option<()> {
    for entry in entries {
        match entry {
            TrayMenuEntry::Command(command) => {
                add(menu, command.id, &command.label, command.checked)?;
            }
            TrayMenuEntry::Submenu(submenu) => {
                add_submenu(menu, &submenu.label, &submenu.entries)?;
            }
            TrayMenuEntry::Separator => separator(menu)?,
        }
    }
    Some(())
}

unsafe fn add_submenu(parent: HMENU, text: &str, entries: &[TrayMenuEntry]) -> Option<()> {
    let submenu = CreatePopupMenu().ok()?;
    if append_entries(submenu, entries).is_none() {
        let _ = DestroyMenu(submenu);
        return None;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    if AppendMenuW(
        parent,
        MF_STRING | MF_POPUP,
        submenu.0 as usize,
        PCWSTR(wide.as_ptr()),
    )
    .is_err()
    {
        let _ = DestroyMenu(submenu);
        return None;
    }
    Some(())
}

unsafe fn add(menu: HMENU, id: u16, text: &str, checked: bool) -> Option<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let flags = MF_STRING
        | if checked {
            MF_CHECKED
        } else {
            Default::default()
        };
    AppendMenuW(menu, flags, usize::from(id), PCWSTR(wide.as_ptr()))
        .ok()
        .map(|_| ())
}

unsafe fn separator(menu: HMENU) -> Option<()> {
    AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
        .ok()
        .map(|_| ())
}

/// 클릭할 수 없는 정보 배너 항목을 메뉴에 추가합니다.
///
/// 명령 식별자로 0을 사용하므로 선택되어도 어떤 동작도 발생하지 않습니다.
unsafe fn add_info_banner(menu: HMENU, text: &str) -> Option<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    AppendMenuW(
        menu,
        MF_STRING | MF_DISABLED | MF_GRAYED,
        0,
        PCWSTR(wide.as_ptr()),
    )
    .ok()
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    use super::CoalescingWorker;

    #[test]
    fn tray_worker_submission_never_waits_for_a_blocked_shell_call() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (handled_tx, handled_rx) = mpsc::channel();
        let worker = CoalescingWorker::spawn(0_u32, move || {
            move |value| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                handled_tx.send(value).unwrap();
            }
        })
        .unwrap();

        let delayed_release = release_tx.clone();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            delayed_release.send(()).unwrap();
        });
        let started = Instant::now();
        worker.submit(1);
        assert!(started.elapsed() < Duration::from_millis(100));
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release.join().unwrap();
        assert_eq!(handled_rx.recv_timeout(Duration::from_secs(1)), Ok(1));

        worker.submit(2);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(handled_rx.recv_timeout(Duration::from_secs(1)), Ok(2));
    }

    #[test]
    fn tray_worker_shutdown_acknowledges_only_after_the_active_shell_call_returns() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = CoalescingWorker::spawn(0_u32, move || {
            move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }
        })
        .unwrap();

        worker.submit(1);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let shutdown = worker.begin_shutdown();
        assert!(shutdown.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).unwrap();
        shutdown.recv_timeout(Duration::from_secs(1)).unwrap();
    }
}
