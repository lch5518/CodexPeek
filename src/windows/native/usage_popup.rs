//! 작업 표시줄 사용량 상세 팝업의 Win32 렌더링입니다.

use std::io;

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, POINT, RECT, SIZE},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC,
            DeleteObject, DrawTextW, FillRect, GetDC, GetMonitorInfoW, MonitorFromWindow,
            ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER,
            BLENDFUNCTION, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS,
            DRAW_TEXT_FORMAT, DT_CALCRECT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT,
            DT_RTLREADING, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FF_SWISS, FW_NORMAL,
            FW_SEMIBOLD, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, OUT_DEFAULT_PRECIS,
            PROOF_QUALITY, TRANSPARENT,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, LoadCursorW,
                RegisterClassW, ShowWindow, UpdateLayeredWindow, CS_DROPSHADOW, IDC_ARROW,
                SW_SHOWNA, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
};

use crate::windows::{
    popup::{
        place_popup, popup_palette, PopupPalette, UsagePopupPresentation, POPUP_WIDTH_LOGICAL,
    },
    widget::{logical_to_physical, Rect},
};

const USAGE_POPUP_CLASS: PCWSTR = w!("CodexUsageMonitor.UsagePopup.v1");
const DAILY_USAGE_CHART_HEIGHT_LOGICAL: i32 = 53;
const DAILY_USAGE_DATE_GAP_LOGICAL: i32 = 4;
const DAILY_USAGE_DATE_HEIGHT_LOGICAL: i32 = 16;
const DAILY_USAGE_DATE_WIDTH_LOGICAL: i32 = 44;
const WRAPPED_TEXT_FALLBACK_HEIGHT_LOGICAL: i32 = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextStyle {
    Meta,
    Label,
    Body,
    Headline,
    Title,
    Error,
}

impl TextStyle {
    fn size(self) -> i32 {
        match self {
            Self::Meta => 12,
            Self::Label => 14,
            Self::Body | Self::Error => 13,
            Self::Headline => 28,
            Self::Title => 18,
        }
    }

    fn weight(self) -> i32 {
        if matches!(self, Self::Label | Self::Headline | Self::Title) {
            FW_SEMIBOLD.0 as i32
        } else {
            FW_NORMAL.0 as i32
        }
    }
}

struct TextBlockLayout {
    text: String,
    rect: Rect,
    style: TextStyle,
}

struct UsageMeterLayout {
    rect: Rect,
    fraction: f64,
    risk: crate::windows::ProfileUsageStatus,
}

struct PopupLayout {
    width: i32,
    height: i32,
    blocks: Vec<TextBlockLayout>,
    rules: Vec<Rect>,
    meters: Vec<UsageMeterLayout>,
    insight: Option<Rect>,
    daily_usage: Option<Rect>,
}

/// 창 이름과 숫자를 같은 행에 두고 폭이 좁으면 한 열로 쌓습니다.
///
/// 입력·출력은 물리 픽셀입니다. RTL에서는 값의 위치만 반전하며 두 사용량 창의 순서는 유지합니다.
fn popup_columns(width: i32, dpi: u32, rtl: bool) -> (Rect, Rect, bool) {
    let scale = |value| logical_to_physical(value, dpi);
    let padding = scale(20);
    let full = Rect::new(padding, 0, (width - padding).max(padding + 1), 0);
    if width < scale(320) {
        return (full, full, false);
    }
    let number_width = scale(108);
    let gap = scale(12);
    if rtl {
        (
            Rect::new(full.left + number_width + gap, 0, full.right, 0),
            Rect::new(full.left, 0, full.left + number_width, 0),
            true,
        )
    } else {
        (
            Rect::new(full.left, 0, full.right - number_width - gap, 0),
            Rect::new(full.right - number_width, 0, full.right, 0),
            true,
        )
    }
}

/// 표시 비율·초기화·소비 속도를 순서대로 측정해 단일 열 상세 화면을 만듭니다.
///
/// `compact`는 작업영역이 부족할 때 보조 설명과 그래프만 생략합니다. 숫자·단위·초기화·상태와
/// 예측은 보존하며, 반환 높이가 여전히 너무 크면 호출자가 네이티브 툴팁으로 폴백합니다.
fn allowance_layout(
    presentation: &UsagePopupPresentation,
    width: i32,
    dpi: u32,
    rtl: bool,
    compact: bool,
    mut measure: impl FnMut(&str, i32, TextStyle) -> i32,
) -> PopupLayout {
    let scale = |value| logical_to_physical(value, dpi);
    let padding = scale(20);
    let full = Rect::new(padding, 0, width - padding, 0);
    let (label, number, paired) = popup_columns(width, dpi, rtl);
    let mut blocks = Vec::new();
    let mut rules = Vec::new();
    let mut meters = Vec::new();
    let mut add = |text: &str, area: Rect, top: &mut i32, style: TextStyle| {
        if text.is_empty() {
            return;
        }
        let height = measure(text, area.width().max(1), style).max(scale(style.size() + 6));
        blocks.push(TextBlockLayout {
            text: text.to_owned(),
            rect: Rect::new(area.left, *top, area.right, top.saturating_add(height)),
            style,
        });
        *top = top.saturating_add(height);
    };
    let mut top = padding;
    add(
        &presentation.profile_label,
        full,
        &mut top,
        TextStyle::Title,
    );
    top += scale(4);
    add(&presentation.lead_label, full, &mut top, TextStyle::Meta);
    if let Some(status) = &presentation.status {
        top += scale(8);
        let style = if presentation.data_state == crate::windows::WidgetDataState::Error {
            TextStyle::Error
        } else {
            TextStyle::Body
        };
        add(status, full, &mut top, style);
    }
    top += scale(16);
    if presentation.rows.is_empty() {
        add(
            &presentation.lead_percent,
            full,
            &mut top,
            TextStyle::Headline,
        );
    }
    for row in &presentation.rows {
        let mut label_bottom = top + scale(6);
        add(&row.label, label, &mut label_bottom, TextStyle::Label);
        let mut number_bottom = if paired { top } else { label_bottom };
        add(
            &row.percent_text,
            number,
            &mut number_bottom,
            TextStyle::Headline,
        );
        top = label_bottom.max(number_bottom) + scale(6);
        let track = Rect::new(full.left, top, full.right, top + scale(4));
        meters.push(UsageMeterLayout {
            rect: track,
            fraction: if row.display_percent.is_finite() {
                row.display_percent.clamp(0.0, 100.0) / 100.0
            } else {
                0.0
            },
            risk: crate::windows::ProfileUsageStatus::from_used_percent(row.used_percent),
        });
        top = track.bottom + scale(8);
        if !row.reset_text.is_empty() {
            add(
                &format!("{}: {}", presentation.reset_label, row.reset_text),
                full,
                &mut top,
                TextStyle::Meta,
            );
        }
        top += scale(16);
    }
    if let Some(credits) = &presentation.reset_credits_text {
        add(credits, full, &mut top, TextStyle::Meta);
        top += scale(12);
    }
    let insight = if !presentation.rows.is_empty() {
        let insight_top = top;
        let inset = scale(12);
        let inner = Rect::new(full.left + inset, 0, full.right - inset, 0);
        top += inset;
        add(
            &presentation.pace_summary,
            inner,
            &mut top,
            TextStyle::Label,
        );
        if !compact {
            if let Some(detail) = &presentation.pace_detail {
                top += scale(4);
                add(detail, inner, &mut top, TextStyle::Meta);
            }
        }
        if !presentation.forecasts.is_empty() {
            top += scale(12);
            add(
                &presentation.forecast_label,
                inner,
                &mut top,
                TextStyle::Meta,
            );
            for forecast in &presentation.forecasts {
                add(&forecast.detail, inner, &mut top, TextStyle::Body);
            }
        }
        top += inset;
        Some(Rect::new(full.left, insight_top, full.right, top))
    } else {
        None
    };
    let daily_usage = if !compact && !presentation.daily_token_usage.is_empty() {
        top += scale(16);
        add(
            &presentation.daily_usage_label,
            full,
            &mut top,
            TextStyle::Meta,
        );
        top += scale(8);
        let chart = Rect::new(
            full.left,
            top,
            full.right,
            top + daily_usage_chart_height(dpi),
        );
        top = chart.bottom + scale(DAILY_USAGE_DATE_GAP_LOGICAL + DAILY_USAGE_DATE_HEIGHT_LOGICAL);
        Some(chart)
    } else {
        None
    };
    top += scale(14);
    rules.push(Rect::new(full.left, top, full.right, top + scale(1)));
    top += scale(8);
    add(&presentation.last_success, full, &mut top, TextStyle::Meta);
    PopupLayout {
        width,
        height: top + padding,
        blocks,
        rules,
        meters,
        insight,
        daily_usage,
    }
}

fn daily_usage_chart_height(dpi: u32) -> i32 {
    logical_to_physical(DAILY_USAGE_CHART_HEIGHT_LOGICAL, dpi)
}

fn daily_date_indices(count: usize) -> Vec<usize> {
    match count {
        0 => Vec::new(),
        1..=3 => (0..count).collect(),
        count => vec![0, count / 2, count - 1],
    }
}

fn daily_date_label(start_date: &str) -> String {
    let date = start_date.split('T').next().unwrap_or(start_date);
    let mut parts = date.split('-');
    let day_prefix = parts
        .clone()
        .nth(2)
        .map(|day| day.chars().take(2).collect::<String>());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_year), Some(month), Some(_day))
            if month.len() == 2
                && day_prefix.as_deref().is_some_and(|day| {
                    day.len() == 2 && day.chars().all(|c| c.is_ascii_digit())
                }) =>
        {
            format!("{month}/{}", day_prefix.unwrap_or_default())
        }
        _ => date.to_owned(),
    }
}

fn daily_date_text_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let direction = if rtl {
        DT_RTLREADING
    } else {
        DRAW_TEXT_FORMAT(0)
    };
    DT_CENTER | DT_SINGLELINE | DT_VCENTER | direction | DT_NOPREFIX
}

fn daily_usage_bar_height(value: u64, maximum: u64, chart_height: i32) -> i32 {
    if value == 0 || maximum == 0 || chart_height <= 0 {
        return 0;
    }
    let ratio = value.min(maximum) as f64 / maximum as f64;
    ((f64::from(chart_height) * ratio).round() as i32).max(1)
}

/// 날짜 인덱스에 대응하는 물리 픽셀 열을 계산해 막대와 날짜가 같은 위치를 사용하게 합니다.
///
/// `area`를 `count`개로 나누고 `rtl`이면 순서를 뒤집습니다. 잘못된 인덱스나 면적이 없는
/// 열은 `None`을 반환하며, 나눗셈 나머지를 분산해 마지막 열까지 영역 안에 배치합니다.
fn daily_usage_slot(area: Rect, index: usize, count: usize, rtl: bool) -> Option<Rect> {
    if index >= count || area.width() <= 0 || area.height() <= 0 {
        return None;
    }
    let count = i64::from(i32::try_from(count).ok()?);
    let index = i64::try_from(index).ok()?;
    let visual_index = if rtl { count - 1 - index } else { index };
    let width = i64::from(area.width());
    let left = area.left + (width * visual_index / count) as i32;
    let right = area.left + (width * (visual_index + 1) / count) as i32;
    (left < right).then_some(Rect::new(left, area.top, right, area.bottom))
}

fn daily_date_bounds(area: Rect, slot: Rect, count: usize, dpi: u32) -> Rect {
    let width = logical_to_physical(DAILY_USAGE_DATE_WIDTH_LOGICAL, dpi)
        .min(area.width() / count.clamp(1, 3) as i32);
    let center = slot.left + slot.width() / 2;
    let left = (center - width / 2).clamp(area.left, area.right - width);
    Rect::new(left, area.top, left + width, area.bottom)
}

fn daily_chart_bounds(area: Rect, dpi: u32) -> Rect {
    // 양 끝 날짜가 막대 중앙에 놓이도록 레이블 너비의 절반을 여백으로 남깁니다.
    let inset =
        logical_to_physical(DAILY_USAGE_DATE_WIDTH_LOGICAL, dpi).min(area.width().max(0)) / 2;
    Rect::new(area.left + inset, area.top, area.right - inset, area.bottom)
}

fn popup_render_size(requested: (i32, i32), bounds: Rect) -> Option<(i32, i32)> {
    (requested.0 > 0
        && requested.1 > 0
        && requested.0 <= bounds.width()
        && requested.1 <= bounds.height())
    .then_some(requested)
}

/// 상세 팝업 전용 창 클래스를 현재 프로세스에 등록합니다.
///
/// `instance`는 앱 모듈 인스턴스여야 합니다. 등록 실패 시 운영체제 오류를 반환하며 다른 UI
/// 클래스나 전역 상태는 변경하지 않습니다.
pub(super) unsafe fn register_class(instance: HINSTANCE) -> io::Result<()> {
    let class = WNDCLASSW {
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(popup_proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW).map_err(win_error)?,
        lpszClassName: USAGE_POPUP_CLASS,
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// 위젯에 고정된 비활성 상세 팝업을 만들고 렌더링합니다.
///
/// `owner`와 `widget`은 호출 동안 유효한 창이어야 합니다. 반환된 창은 호출자가
/// `destroy`로 닫아야 하며, 함수는 포커스를 이동하거나 UI 스레드 밖 I/O를 수행하지 않습니다.
pub(super) unsafe fn show(
    instance: HINSTANCE,
    owner: HWND,
    widget: HWND,
    presentation: &UsagePopupPresentation,
    light: bool,
    rtl: bool,
) -> io::Result<HWND> {
    let dpi = GetDpiForWindow(widget).max(96);
    let mut anchor = RECT::default();
    GetWindowRect(widget, &mut anchor).map_err(win_error)?;
    let monitor = MonitorFromWindow(widget, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
        return Err(io::Error::last_os_error());
    }
    let margin = logical_to_physical(12, dpi);
    let available_width = (monitor_info.rcWork.right - monitor_info.rcWork.left)
        .saturating_sub(margin.saturating_mul(2))
        .max(0);
    let width = logical_to_physical(POPUP_WIDTH_LOGICAL, dpi).min(available_width);
    if width < logical_to_physical(200, dpi) {
        return Err(io::Error::other("popup work area is too narrow"));
    }
    let max_height = logical_to_physical(560, dpi).min(
        (monitor_info.rcWork.bottom - monitor_info.rcWork.top)
            .saturating_sub(margin.saturating_mul(2)),
    );
    let measure = |text: &str, width, style: TextStyle| {
        measure_wrapped_text_height(text, width, style.size(), style.weight(), dpi, rtl)
    };
    let mut layout = allowance_layout(presentation, width, dpi, rtl, false, measure);
    if layout.height > max_height {
        layout = allowance_layout(presentation, width, dpi, rtl, true, measure);
    }
    if layout.height > max_height {
        return Err(io::Error::other("popup content exceeds readable work area"));
    }
    let bounds = place_popup(
        Rect::new(anchor.left, anchor.top, anchor.right, anchor.bottom),
        Rect::new(
            monitor_info.rcWork.left,
            monitor_info.rcWork.top,
            monitor_info.rcWork.right,
            monitor_info.rcWork.bottom,
        ),
        (layout.width, layout.height),
        margin,
        logical_to_physical(8, dpi),
    );
    let render_size =
        popup_render_size((layout.width, layout.height), bounds).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "popup content does not fit the monitor work area",
            )
        })?;
    let title: Vec<u16> = presentation
        .profile_label
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
        USAGE_POPUP_CLASS,
        PCWSTR(title.as_ptr()),
        WS_POPUP,
        bounds.left,
        bounds.top,
        bounds.width(),
        bounds.height(),
        Some(owner),
        None,
        Some(instance),
        None,
    )
    .map_err(win_error)?;
    if let Err(error) = render(
        hwnd,
        presentation,
        popup_palette(light),
        dpi,
        rtl,
        &layout,
        render_size,
    ) {
        let _ = DestroyWindow(hwnd);
        return Err(error);
    }
    let _ = ShowWindow(hwnd, SW_SHOWNA);
    Ok(hwnd)
}

/// 표시 중인 상세 팝업을 닫습니다. 이미 닫힌 기본 핸들은 무시합니다.
pub(super) unsafe fn destroy(hwnd: HWND) {
    if hwnd != HWND::default() {
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, message, wparam, lparam)
}

/// 줄바꿈 문구를 현재 DPI와 글꼴로 측정해 필요한 실제 높이를 반환합니다.
///
/// `value`는 표시할 문구이고 `text_width`는 물리 픽셀 단위의 최대 너비입니다. 빈 문구는 0을
/// 반환합니다. 화면 DC 또는 글꼴 측정이 실패하면 문구가 잘리지 않도록 보수적인 4줄 높이를
/// 사용하며, 생성한 GDI 객체와 DC는 호출 안에서 정리합니다.
unsafe fn measure_wrapped_text_height(
    value: &str,
    text_width: i32,
    font_size: i32,
    font_weight: i32,
    dpi: u32,
    rtl: bool,
) -> i32 {
    if value.is_empty() {
        return 0;
    }
    let fallback = logical_to_physical(WRAPPED_TEXT_FALLBACK_HEIGHT_LOGICAL, dpi);
    let dc = GetDC(None);
    if dc == HDC::default() {
        return fallback;
    }

    let font = popup_font(dc, font_size, font_weight, dpi);
    let old_font = SelectObject(dc, font);
    let mut text: Vec<u16> = value.encode_utf16().collect();
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: text_width.max(1),
        bottom: 0,
    };
    let measured = DrawTextW(
        dc,
        &mut text,
        &mut rect,
        wrapped_text_format(rtl) | DT_CALCRECT,
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(font);
    let _ = ReleaseDC(None, dc);

    if measured > 0 {
        measured
    } else {
        fallback
    }
}

unsafe fn render(
    hwnd: HWND,
    presentation: &UsagePopupPresentation,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
    layout: &PopupLayout,
    render_size: (i32, i32),
) -> io::Result<()> {
    let pixel_count = usize::try_from(render_size.0)
        .ok()
        .and_then(|width| {
            usize::try_from(render_size.1)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid popup bitmap size"))?;
    let screen_dc = GetDC(None);
    if screen_dc.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    let memory_dc = CreateCompatibleDC(Some(screen_dc));
    if memory_dc.is_invalid() {
        let _ = ReleaseDC(None, screen_dc);
        return Err(io::Error::last_os_error());
    }
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: render_size.0,
            biHeight: -render_size.1,
            biPlanes: 1,
            biBitCount: 32,
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
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err(win_error(error));
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
    paint_content(
        memory_dc,
        presentation,
        palette,
        dpi,
        rtl,
        layout,
        render_size,
    );
    apply_surface_alpha(
        std::slice::from_raw_parts_mut(bits.cast::<u32>(), pixel_count),
        render_size.0,
        render_size.1,
        logical_to_physical(8, dpi),
    );
    let source = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: render_size.0,
        cy: render_size.1,
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

unsafe fn paint_content(
    dc: HDC,
    presentation: &UsagePopupPresentation,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
    layout: &PopupLayout,
    render_size: (i32, i32),
) {
    let width = render_size.0;
    let height = render_size.1;
    fill(dc, Rect::new(0, 0, width, height), palette.background);
    let border = logical_to_physical(1, dpi).max(1);
    for rect in [
        Rect::new(0, 0, width, border),
        Rect::new(0, height - border, width, height),
        Rect::new(0, 0, border, height),
        Rect::new(width - border, 0, width, height),
    ] {
        fill(dc, rect, palette.border);
    }
    if let Some(insight) = layout.insight {
        fill(dc, insight, palette.surface);
    }
    for meter in &layout.meters {
        fill(dc, meter.rect, palette.separator);
        let fill_width = (f64::from(meter.rect.width()) * meter.fraction).round() as i32;
        let color = match meter.risk {
            crate::windows::ProfileUsageStatus::Healthy => 0x0074_c748,
            crate::windows::ProfileUsageStatus::Warning => 0x0023_a6f5,
            crate::windows::ProfileUsageStatus::Critical => 0x005c_5cff,
        };
        let rect = if rtl {
            Rect::new(
                meter.rect.right - fill_width,
                meter.rect.top,
                meter.rect.right,
                meter.rect.bottom,
            )
        } else {
            Rect::new(
                meter.rect.left,
                meter.rect.top,
                meter.rect.left + fill_width,
                meter.rect.bottom,
            )
        };
        fill(dc, rect, color);
    }
    for rule in &layout.rules {
        fill(dc, *rule, palette.separator);
    }
    for block in &layout.blocks {
        let color = match block.style {
            TextStyle::Meta => palette.secondary_text,
            TextStyle::Error => palette.danger,
            _ => palette.text,
        };
        draw_formatted_text(
            dc,
            &block.text,
            block.rect,
            popup_font(dc, block.style.size(), block.style.weight(), dpi),
            color,
            if block.style == TextStyle::Headline {
                wrapped_text_format(!rtl) & !DT_RTLREADING
            } else {
                wrapped_text_format(rtl)
            },
        );
    }
    if let Some(chart) = layout.daily_usage {
        paint_daily_usage(
            dc,
            &presentation.daily_token_usage,
            chart,
            palette,
            dpi,
            rtl,
        );
        let date_top = chart.bottom + logical_to_physical(DAILY_USAGE_DATE_GAP_LOGICAL, dpi);
        paint_daily_dates(
            dc,
            &presentation.daily_token_usage,
            Rect::new(
                chart.left,
                date_top,
                chart.right,
                date_top + logical_to_physical(DAILY_USAGE_DATE_HEIGHT_LOGICAL, dpi),
            ),
            palette,
            dpi,
            rtl,
        );
    }
}

unsafe fn paint_daily_usage(
    dc: HDC,
    daily_usage: &[crate::DailyTokenUsage],
    chart: Rect,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
) {
    let chart = daily_chart_bounds(chart, dpi);
    if daily_usage.is_empty() || chart.width() <= 0 || chart.height() <= 0 {
        return;
    }
    let maximum = daily_usage
        .iter()
        .map(|daily| daily.tokens)
        .max()
        .unwrap_or(0);
    let baseline_height = logical_to_physical(1, dpi).max(1);
    fill(
        dc,
        Rect::new(
            chart.left,
            chart.bottom.saturating_sub(baseline_height),
            chart.right,
            chart.bottom,
        ),
        palette.separator,
    );
    let gap = logical_to_physical(4, dpi).max(1);
    for (index, daily) in daily_usage.iter().enumerate() {
        let Some(slot) = daily_usage_slot(chart, index, daily_usage.len(), rtl) else {
            continue;
        };
        let gap = gap.min(slot.width() - 1);
        let height = daily_usage_bar_height(daily.tokens, maximum, chart.height());
        if height == 0 {
            continue;
        }
        let bar = Rect::new(
            slot.left + gap / 2,
            chart.bottom.saturating_sub(height),
            slot.right - (gap - gap / 2),
            chart.bottom,
        );
        fill(dc, bar, palette.secondary_text);
    }
}

unsafe fn paint_daily_dates(
    dc: HDC,
    daily_usage: &[crate::DailyTokenUsage],
    area: Rect,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
) {
    let indices = daily_date_indices(daily_usage.len());
    if indices.is_empty() || area.width() <= 0 || area.height() <= 0 {
        return;
    }
    for index in indices {
        let Some(slot) =
            daily_usage_slot(daily_chart_bounds(area, dpi), index, daily_usage.len(), rtl)
        else {
            continue;
        };
        let label = daily_date_label(&daily_usage[index].start_date);
        draw_formatted_text(
            dc,
            &label,
            daily_date_bounds(area, slot, daily_usage.len(), dpi),
            number_font(11, dpi),
            palette.secondary_text,
            daily_date_text_format(rtl),
        );
    }
}

fn wrapped_text_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    alignment | DT_WORDBREAK | DT_NOPREFIX
}

unsafe fn fill(dc: HDC, rect: Rect, color: u32) {
    let brush = CreateSolidBrush(COLORREF(color));
    FillRect(
        dc,
        &RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        },
        brush,
    );
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn popup_font(_dc: HDC, size: i32, weight: i32, dpi: u32) -> HGDIOBJ {
    HGDIOBJ(
        CreateFontW(
            -logical_to_physical(size, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            PROOF_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_SWISS.0),
            w!("Segoe UI Variable"),
        )
        .0,
    )
}

unsafe fn number_font(size: i32, dpi: u32) -> HGDIOBJ {
    HGDIOBJ(
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
            w!("Segoe UI"),
        )
        .0,
    )
}

unsafe fn draw_formatted_text(
    dc: HDC,
    value: &str,
    rect: Rect,
    font: HGDIOBJ,
    color: u32,
    format: DRAW_TEXT_FORMAT,
) {
    let old_font = SelectObject(dc, font);
    let _ = SetBkMode(dc, TRANSPARENT);
    let _ = SetTextColor(dc, COLORREF(color));
    let mut value: Vec<u16> = value.encode_utf16().collect();
    let mut rect = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    let _ = DrawTextW(dc, &mut value, &mut rect, format);
    SelectObject(dc, old_font);
    let _ = DeleteObject(font);
}

fn rounded_surface_alpha(x: i32, y: i32, width: i32, height: i32, radius: i32) -> u8 {
    let radius = radius.max(1);
    let px = f64::from(x) + 0.5;
    let py = f64::from(y) + 0.5;
    let center_x = if x < radius {
        f64::from(radius)
    } else if x >= width - radius {
        f64::from(width - radius)
    } else {
        px
    };
    let center_y = if y < radius {
        f64::from(radius)
    } else if y >= height - radius {
        f64::from(height - radius)
    } else {
        py
    };
    let distance = ((px - center_x).powi(2) + (py - center_y).powi(2)).sqrt();
    ((f64::from(radius) + 0.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn apply_surface_alpha(pixels: &mut [u32], width: i32, height: i32, radius: i32) {
    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            let pixel = pixels[index];
            let coverage = rounded_surface_alpha(x, y, width, height, radius);
            if coverage == 0 {
                pixels[index] = 0;
                continue;
            }
            let alpha = coverage;
            let blue = (pixel & 0xff) * u32::from(alpha) / 255;
            let green = ((pixel >> 8) & 0xff) * u32::from(alpha) / 255;
            let red = ((pixel >> 16) & 0xff) * u32::from(alpha) / 255;
            pixels[index] = (u32::from(alpha) << 24) | (red << 16) | (green << 8) | blue;
        }
    }
}

fn win_error(_: windows::core::Error) -> io::Error {
    io::Error::last_os_error()
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::Gdi::{
        DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_RTLREADING, DT_SINGLELINE, DT_VCENTER,
        DT_WORDBREAK,
    };

    use super::{
        allowance_layout, apply_surface_alpha, daily_chart_bounds, daily_date_bounds,
        daily_date_indices, daily_date_label, daily_date_text_format, daily_usage_bar_height,
        daily_usage_slot, popup_columns, popup_render_size, rounded_surface_alpha,
        wrapped_text_format, TextStyle,
    };
    use crate::windows::widget::Rect;

    #[test]
    fn rounded_surface_keeps_center_opaque_and_corners_transparent() {
        assert_eq!(rounded_surface_alpha(50, 50, 100, 100, 14), 255);
        assert_eq!(rounded_surface_alpha(0, 0, 100, 100, 14), 0);
        assert!(rounded_surface_alpha(4, 4, 100, 100, 14) > 0);
    }

    #[test]
    fn long_detail_text_wraps_without_ellipsis_in_ltr_and_rtl() {
        assert_eq!(
            wrapped_text_format(false),
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX
        );
        assert_eq!(
            wrapped_text_format(true),
            DT_RIGHT | DT_RTLREADING | DT_WORDBREAK | DT_NOPREFIX
        );
    }

    #[test]
    fn allowance_columns_stack_on_narrow_screens_and_mirror_at_every_dpi() {
        for dpi in [96, 120, 144, 168, 192] {
            let scale = |value| crate::windows::widget::logical_to_physical(value, dpi);
            let (main, side, columns) = popup_columns(scale(440), dpi, false);
            assert!(columns);
            assert!(main.right < side.left);
            let (rtl_main, rtl_side, _) = popup_columns(scale(440), dpi, true);
            assert_eq!(main.width(), rtl_main.width());
            assert_eq!(side.width(), rtl_side.width());
            assert!(rtl_side.right < rtl_main.left);
            let (main, side, columns) = popup_columns(scale(280), dpi, false);
            assert!(!columns);
            assert_eq!(main, side);
        }
    }

    #[test]
    fn allowance_surface_is_opaque_even_on_colored_dark_paper() {
        let mut pixels = vec![0x00181a1b; 100 * 100];
        apply_surface_alpha(&mut pixels, 100, 100, 2);
        assert_eq!(pixels[50 * 100 + 50], 0xff181a1b);
        assert!(pixels[0] >> 24 < 255);
    }

    #[test]
    fn allowance_measured_layout_and_render_keep_content_inside_paper() {
        use crate::windows::popup::{popup_palette, tests::ready_view, usage_popup_presentation};
        for (language, rtl) in [
            (crate::Language::Korean, false),
            (crate::Language::English, false),
            (crate::Language::Arabic, true),
        ] {
            let mut view = ready_view();
            if language == crate::Language::Korean {
                view.usage_profile_label = "개인 프로필".into();
                view.consumption_pace.summary = "사용 속도 여유 · 최근 관측 기준".into();
                view.consumption_pace.detail =
                    Some("최근 2시간 동안 2% 사용 · 시간당 약 1%".into());
                view.secondary.as_mut().unwrap().forecast =
                    crate::windows::ForecastView::Collecting {
                        line: "예측에 필요한 표본을 수집하고 있습니다".into(),
                    };
                view.last_success = "마지막 성공 120초 전".into();
            }
            view.daily_token_usage = (1..=14)
                .map(|day| {
                    crate::DailyTokenUsage::new(
                        format!("2026-09-{day:02}"),
                        (day * 7919 % 25000) as u64,
                    )
                })
                .collect();
            let presentation = usage_popup_presentation(&view, language);
            for dpi in [96, 120, 144, 168, 192] {
                for logical_width in [280, 320, 400] {
                    let width = crate::windows::widget::logical_to_physical(logical_width, dpi);
                    for compact in [false, true] {
                        // SAFETY: 측정 함수는 자체 DC·글꼴을 소유하며 외부 계정이나 창을 사용하지 않습니다.
                        let layout = allowance_layout(
                            &presentation,
                            width,
                            dpi,
                            rtl,
                            compact,
                            |text, width, style: TextStyle| unsafe {
                                super::measure_wrapped_text_height(
                                    text,
                                    width,
                                    style.size(),
                                    style.weight(),
                                    dpi,
                                    rtl,
                                )
                            },
                        );
                        let paper = Rect::new(0, 0, width, layout.height);
                        for (index, block) in layout.blocks.iter().enumerate() {
                            assert!(block.rect.is_inside(paper));
                            assert!(layout.blocks[index + 1..]
                                .iter()
                                .all(|other| !block.rect.intersects(other.rect)));
                            assert!(layout
                                .rules
                                .iter()
                                .all(|rule| !block.rect.intersects(*rule)));
                        }
                        assert!(layout
                            .blocks
                            .iter()
                            .any(|block| block.text == presentation.lead_percent));
                        if dpi == 96 && logical_width == 400 && !compact {
                            for light in [true, false] {
                                crate::windows::test_render::surface(
                                    &format!("popup-{language:?}-{light}"),
                                    width,
                                    layout.height,
                                    |dc| unsafe {
                                        super::paint_content(
                                            dc,
                                            &presentation,
                                            popup_palette(light),
                                            dpi,
                                            rtl,
                                            &layout,
                                            (width, layout.height),
                                        );
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn daily_usage_bar_height_preserves_zero_and_scales_positive_values() {
        assert_eq!(daily_usage_bar_height(50, 100, 80), 40);
        assert_eq!(daily_usage_bar_height(0, 0, 80), 0);
        assert_eq!(daily_usage_bar_height(0, 100, 80), 0);
        assert_eq!(daily_usage_bar_height(1, u64::MAX, 80), 1);
        assert_eq!(daily_usage_bar_height(u64::MAX, u64::MAX, 80), 80);
        assert_eq!(daily_usage_bar_height(200, 100, 80), 80);
        assert_eq!(daily_usage_bar_height(50, 0, 80), 0);
        assert_eq!(daily_usage_bar_height(50, 100, 0), 0);
        assert_eq!(daily_usage_bar_height(50, 100, -1), 0);
    }

    #[test]
    fn allowance_keeps_each_reset_beside_its_window_and_risk_independent_of_display_mode() {
        use crate::windows::popup::{tests::ready_view, usage_popup_presentation};
        use crate::windows::ProfileUsageStatus;
        for remaining in [true, false] {
            let mut view = ready_view();
            view.show_remaining_percent = remaining;
            let row = view.secondary.as_mut().unwrap();
            row.used_percent = 95.0;
            row.display_percent = if remaining { 5.0 } else { 95.0 };
            row.percent_text = if remaining { "5%" } else { "95%" }.into();
            let presentation = usage_popup_presentation(&view, crate::Language::English);
            let layout = allowance_layout(&presentation, 400, 96, false, false, |_, _, style| {
                style.size() + 6
            });
            assert_eq!(layout.meters.len(), 2);
            assert_eq!(layout.meters[1].risk, ProfileUsageStatus::Critical);
            assert_eq!(
                layout.meters[1].fraction,
                if remaining { 0.05 } else { 0.95 }
            );
            for (index, row) in presentation.rows.iter().enumerate() {
                let value = layout
                    .blocks
                    .iter()
                    .find(|block| block.text == row.percent_text)
                    .unwrap();
                let reset = layout
                    .blocks
                    .iter()
                    .find(|block| block.text.ends_with(&row.reset_text))
                    .unwrap();
                assert!(value.rect.bottom <= layout.meters[index].rect.top);
                assert!(layout.meters[index].rect.bottom <= reset.rect.top);
                if let Some(next) = presentation.rows.get(index + 1) {
                    let next_value = layout
                        .blocks
                        .iter()
                        .find(|block| block.text == next.percent_text)
                        .unwrap();
                    assert!(reset.rect.bottom < next_value.rect.top);
                }
            }
        }
    }

    #[test]
    fn loading_and_error_surfaces_preserve_status_without_inventing_usage() {
        use crate::windows::popup::{tests::ready_view, usage_popup_presentation};
        use crate::windows::WidgetDataState;
        for state in [WidgetDataState::Loading, WidgetDataState::Error] {
            let mut view = ready_view();
            view.primary = None;
            view.secondary = None;
            view.daily_token_usage.clear();
            view.data_state = state;
            view.status = if state == WidgetDataState::Loading {
                "불러오는 중"
            } else {
                "사용량을 조회하지 못했습니다"
            }
            .into();
            let presentation = usage_popup_presentation(&view, crate::Language::Korean);
            let layout = allowance_layout(&presentation, 400, 96, false, false, |_, _, style| {
                style.size() + 6
            });
            assert!(layout.meters.is_empty());
            assert!(layout.insight.is_none());
            assert!(layout.daily_usage.is_none());
            let status = layout
                .blocks
                .iter()
                .find(|block| block.text == view.status)
                .unwrap();
            assert_eq!(
                status.style == TextStyle::Error,
                state == WidgetDataState::Error
            );
            for light in [true, false] {
                crate::windows::test_render::surface(
                    &format!("popup-{state:?}-{light}"),
                    400,
                    layout.height,
                    |dc| unsafe {
                        super::paint_content(
                            dc,
                            &presentation,
                            crate::windows::popup::popup_palette(light),
                            96,
                            false,
                            &layout,
                            (400, layout.height),
                        );
                    },
                );
            }
        }
    }

    #[test]
    fn daily_chart_columns_and_dates_stay_aligned_across_dpi_and_direction() {
        for dpi in [96, 120, 144, 192] {
            let width = crate::windows::widget::logical_to_physical(272, dpi);
            let area = Rect::new(20, 0, 20 + width, 28);
            let chart = daily_chart_bounds(area, dpi);
            for count in [1, 2, 3, 5, 14] {
                for rtl in [false, true] {
                    let mut labels = Vec::new();
                    for index in 0..count {
                        let slot = daily_usage_slot(chart, index, count, rtl).unwrap();
                        let mirrored =
                            daily_usage_slot(chart, count - 1 - index, count, !rtl).unwrap();
                        assert_eq!(slot, mirrored);
                        assert!(slot.is_inside(area));
                        if daily_date_indices(count).contains(&index) {
                            let label = daily_date_bounds(area, slot, count, dpi);
                            assert!(label.is_inside(area));
                            let center = slot.left + slot.width() / 2;
                            assert!(label.left <= center && center <= label.right);
                            assert_eq!(label.left + label.width() / 2, center);
                            assert!(labels.iter().all(|other| !label.intersects(*other)));
                            labels.push(label);
                        }
                    }
                }
            }
        }
        let tiny = Rect::new(0, 0, 5, 10);
        for rtl in [false, true] {
            let slots: Vec<_> = (0..14)
                .filter_map(|index| daily_usage_slot(tiny, index, 14, rtl))
                .collect();
            assert_eq!(slots.len(), 5);
            assert!(slots.iter().all(|slot| slot.is_inside(tiny)));
        }
        assert_eq!(daily_usage_slot(tiny, 0, 0, false), None);
        assert_eq!(daily_usage_slot(tiny, 14, 14, false), None);
        assert_eq!(daily_usage_slot(Rect::new(0, 0, 0, 10), 0, 1, false), None);
    }

    #[test]
    fn daily_date_labels_choose_at_most_three_evenly_spaced_days() {
        assert_eq!(daily_date_indices(0), Vec::<usize>::new());
        assert_eq!(daily_date_indices(1), vec![0]);
        assert_eq!(daily_date_indices(2), vec![0, 1]);
        assert_eq!(daily_date_indices(5), vec![0, 2, 4]);
    }

    #[test]
    fn daily_date_label_keeps_month_and_day_compact() {
        assert_eq!(daily_date_label("2026-08-19"), "08/19");
        assert_eq!(daily_date_label("invalid"), "invalid");
    }

    #[test]
    fn daily_date_text_is_centered_and_transparent() {
        assert_eq!(
            daily_date_text_format(false),
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX
        );
        assert_eq!(
            daily_date_text_format(true),
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_RTLREADING | DT_NOPREFIX
        );
    }

    #[test]
    fn oversized_content_uses_the_native_fallback_instead_of_clipping() {
        assert_eq!(
            popup_render_size((360, 800), Rect::new(0, 0, 360, 480)),
            None
        );
        assert_eq!(
            popup_render_size((360, 480), Rect::new(0, 0, 360, 480)),
            Some((360, 480))
        );
    }
}
