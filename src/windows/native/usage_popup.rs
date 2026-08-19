//! 작업 표시줄 사용량 상세 팝업의 Win32 렌더링입니다.

use std::io;

use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, POINT, RECT, SIZE},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC,
            DeleteObject, DrawTextW, Ellipse, FillRect, GetDC, GetMonitorInfoW, GetStockObject,
            MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
            BITMAPINFOHEADER, BLENDFUNCTION, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
            DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
            DT_NOPREFIX, DT_RIGHT, DT_RTLREADING, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK,
            FF_SWISS, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
            NULL_PEN, OUT_DEFAULT_PRECIS, PROOF_QUALITY, TRANSPARENT,
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
const PACE_SUMMARY_TOP_LOGICAL: i32 = 106;
const PACE_DETAIL_GAP_LOGICAL: i32 = 4;
const FORECAST_GAP_LOGICAL: i32 = 8;
const DAILY_USAGE_GAP_LOGICAL: i32 = 12;
const DAILY_USAGE_HEIGHT_LOGICAL: i32 = 92;
const DAILY_USAGE_CHART_HEIGHT_LOGICAL: i32 = 53;
const DAILY_USAGE_TITLE_HEIGHT_LOGICAL: i32 = 18;
const DAILY_USAGE_CHART_GAP_LOGICAL: i32 = 4;
const DAILY_USAGE_DATE_GAP_LOGICAL: i32 = 3;
const DAILY_USAGE_DATE_HEIGHT_LOGICAL: i32 = 14;
const DAILY_USAGE_MIN_BAR_HEIGHT_PX: i32 = 4;
const WRAPPED_TEXT_FALLBACK_HEIGHT_LOGICAL: i32 = 80;

#[derive(Clone, Copy)]
struct TextBlockLayout {
    top: i32,
    height: i32,
}

#[derive(Clone, Copy)]
struct ForecastRowLayout {
    top: i32,
    detail_height: i32,
}

#[derive(Clone, Copy)]
struct DailyUsageLayout {
    top: i32,
    height: i32,
}

struct PopupLayout {
    width: i32,
    height: i32,
    pace_summary: TextBlockLayout,
    pace_detail: Option<TextBlockLayout>,
    daily_usage: Option<DailyUsageLayout>,
    forecast_rows: Vec<ForecastRowLayout>,
}

fn forecast_row_height(detail_height: i32, dpi: u32) -> i32 {
    detail_height
        .max(0)
        .saturating_add(logical_to_physical(24, dpi))
}

fn popup_height_for_content(content_bottom: i32, dpi: u32) -> i32 {
    content_bottom
        .max(0)
        .saturating_add(logical_to_physical(20, dpi))
}

fn pace_detail_top(summary_height: i32, dpi: u32) -> i32 {
    logical_to_physical(PACE_SUMMARY_TOP_LOGICAL, dpi)
        .saturating_add(summary_height.max(0))
        .saturating_add(logical_to_physical(PACE_DETAIL_GAP_LOGICAL, dpi))
}

fn daily_usage_top(content_bottom: i32, dpi: u32) -> i32 {
    content_bottom.saturating_add(logical_to_physical(DAILY_USAGE_GAP_LOGICAL, dpi))
}

fn daily_usage_height(dpi: u32) -> i32 {
    logical_to_physical(DAILY_USAGE_HEIGHT_LOGICAL, dpi)
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
    let ratio = if maximum > 0 {
        (value.min(maximum) as f64 / maximum as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((f64::from(chart_height.max(0)) * ratio).round() as i32)
        .max(DAILY_USAGE_MIN_BAR_HEIGHT_PX.min(chart_height.max(0)))
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
    let margin = logical_to_physical(8, dpi);
    let available_width = (monitor_info.rcWork.right - monitor_info.rcWork.left)
        .saturating_sub(margin.saturating_mul(2))
        .max(0);
    let width = logical_to_physical(POPUP_WIDTH_LOGICAL, dpi).min(available_width);
    let padding = logical_to_physical(20, dpi);
    let text_width = width
        .saturating_sub(padding.saturating_mul(2))
        .saturating_sub(logical_to_physical(36, dpi))
        .saturating_sub(logical_to_physical(12, dpi))
        .max(1);
    let pace_summary = TextBlockLayout {
        top: logical_to_physical(PACE_SUMMARY_TOP_LOGICAL, dpi),
        height: measure_wrapped_text_height(
            &presentation.pace_summary,
            text_width,
            13,
            FW_SEMIBOLD.0 as i32,
            dpi,
            rtl,
        ),
    };
    let pace_detail_top = pace_detail_top(pace_summary.height, dpi);
    let pace_detail = presentation
        .pace_detail
        .as_deref()
        .map(|detail| TextBlockLayout {
            top: pace_detail_top,
            height: measure_wrapped_text_height(
                detail,
                text_width,
                11,
                FW_NORMAL.0 as i32,
                dpi,
                rtl,
            ),
        });
    let mut content_bottom = pace_detail
        .map(|detail| detail.top.saturating_add(detail.height))
        .unwrap_or(pace_detail_top);
    let daily_usage = (!presentation.daily_token_usage.is_empty()).then(|| {
        let top = daily_usage_top(content_bottom, dpi);
        DailyUsageLayout {
            top,
            height: daily_usage_height(dpi),
        }
    });
    if let Some(daily_usage) = daily_usage {
        content_bottom = daily_usage.top.saturating_add(daily_usage.height);
    }
    content_bottom = content_bottom.saturating_add(logical_to_physical(FORECAST_GAP_LOGICAL, dpi));
    let mut forecast_rows = Vec::with_capacity(presentation.forecasts.len());
    for forecast in &presentation.forecasts {
        let detail_height = measure_wrapped_text_height(
            &forecast.detail,
            text_width,
            11,
            FW_NORMAL.0 as i32,
            dpi,
            rtl,
        );
        forecast_rows.push(ForecastRowLayout {
            top: content_bottom,
            detail_height,
        });
        content_bottom = content_bottom.saturating_add(forecast_row_height(detail_height, dpi));
    }
    let layout = PopupLayout {
        width,
        height: popup_height_for_content(content_bottom, dpi),
        pace_summary,
        pace_detail,
        daily_usage,
        forecast_rows,
    };
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
    let memory_dc = CreateCompatibleDC(Some(screen_dc));
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
        logical_to_physical(14, dpi),
        palette.background,
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
    let padding = logical_to_physical(20, dpi);
    let icon = logical_to_physical(36, dpi);
    let gap = logical_to_physical(12, dpi);
    let text_left = padding + icon + gap;
    let text_right = width - padding;
    let text_rect = |top: i32, bottom: i32| {
        if rtl {
            Rect::new(padding, top, width - text_left, bottom)
        } else {
            Rect::new(text_left, top, text_right, bottom)
        }
    };
    let icon_rect = |top: i32| {
        let left = if rtl { width - padding - icon } else { padding };
        Rect::new(left, top, left + icon, top + icon)
    };
    let section = |logical: i32| logical_to_physical(logical, dpi);

    draw_icon(dc, icon_rect(section(12)), "\u{E77B}", palette, dpi);
    draw_text(
        dc,
        &presentation.profile_label,
        text_rect(section(10), section(34)),
        popup_font(dc, 14, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    let reset = presentation
        .reset_text
        .as_deref()
        .map(|value| format!("{}: {value}", presentation.reset_label))
        .unwrap_or_else(|| format!("{}: --", presentation.reset_label));
    draw_text(
        dc,
        &reset,
        text_rect(section(34), section(56)),
        popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
        palette.secondary_text,
        rtl,
        true,
    );
    separator(dc, section(68), width, padding, palette.separator);

    draw_icon(dc, icon_rect(section(80)), "\u{E95E}", palette, dpi);
    draw_text(
        dc,
        &presentation.forecast_label,
        text_rect(section(78), section(104)),
        popup_font(dc, 14, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    draw_formatted_text(
        dc,
        &presentation.pace_summary,
        text_rect(
            layout.pace_summary.top,
            layout
                .pace_summary
                .top
                .saturating_add(layout.pace_summary.height),
        ),
        popup_font(dc, 13, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        wrapped_text_format(rtl),
    );
    if let (Some(detail), Some(detail_layout)) =
        (presentation.pace_detail.as_deref(), layout.pace_detail)
    {
        draw_formatted_text(
            dc,
            detail,
            text_rect(
                detail_layout.top,
                detail_layout.top.saturating_add(detail_layout.height),
            ),
            popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
            palette.secondary_text,
            wrapped_text_format(rtl),
        );
    }

    if let Some(daily_layout) = layout.daily_usage {
        let daily_rect = text_rect(
            daily_layout.top,
            daily_layout.top.saturating_add(daily_layout.height),
        );
        draw_text(
            dc,
            &presentation.daily_usage_label,
            Rect::new(
                daily_rect.left,
                daily_rect.top,
                daily_rect.right,
                daily_rect
                    .top
                    .saturating_add(section(DAILY_USAGE_TITLE_HEIGHT_LOGICAL)),
            ),
            popup_font(dc, 11, FW_SEMIBOLD.0 as i32, dpi),
            palette.text,
            rtl,
            true,
        );
        let chart_top = daily_rect.top.saturating_add(section(
            DAILY_USAGE_TITLE_HEIGHT_LOGICAL + DAILY_USAGE_CHART_GAP_LOGICAL,
        ));
        let chart_bottom = chart_top
            .saturating_add(daily_usage_chart_height(dpi))
            .min(daily_rect.bottom);
        paint_daily_usage(
            dc,
            &presentation.daily_token_usage,
            Rect::new(daily_rect.left, chart_top, daily_rect.right, chart_bottom),
            palette,
            dpi,
            rtl,
        );
        let date_top =
            chart_bottom.saturating_add(logical_to_physical(DAILY_USAGE_DATE_GAP_LOGICAL, dpi));
        paint_daily_dates(
            dc,
            &presentation.daily_token_usage,
            Rect::new(
                daily_rect.left,
                date_top,
                daily_rect.right,
                date_top
                    .saturating_add(logical_to_physical(DAILY_USAGE_DATE_HEIGHT_LOGICAL, dpi))
                    .min(daily_rect.bottom),
            ),
            palette,
            dpi,
            rtl,
        );
    }

    for (forecast, row) in presentation.forecasts.iter().zip(&layout.forecast_rows) {
        draw_text(
            dc,
            &forecast.label,
            text_rect(row.top, row.top.saturating_add(section(18))),
            popup_font(dc, 12, FW_SEMIBOLD.0 as i32, dpi),
            palette.text,
            rtl,
            true,
        );
        let detail_top = row.top.saturating_add(section(20));
        draw_formatted_text(
            dc,
            &forecast.detail,
            text_rect(detail_top, detail_top.saturating_add(row.detail_height)),
            popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
            palette.secondary_text,
            wrapped_text_format(rtl),
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
    if daily_usage.is_empty() || chart.width() <= 0 || chart.height() <= 0 {
        return;
    }
    let maximum = daily_usage
        .iter()
        .map(|daily| daily.tokens)
        .max()
        .unwrap_or(1)
        .max(1);
    let gap = logical_to_physical(4, dpi).max(1);
    let count = i32::try_from(daily_usage.len()).unwrap_or(i32::MAX).max(1);
    let total_gap = gap.saturating_mul(count.saturating_sub(1));
    let bar_width = chart.width().saturating_sub(total_gap).max(count) / count;
    for (index, daily) in daily_usage.iter().enumerate() {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        let visual_index = if rtl { count - 1 - index } else { index };
        let left = chart
            .left
            .saturating_add(visual_index.saturating_mul(bar_width.saturating_add(gap)));
        let height = daily_usage_bar_height(daily.tokens, maximum, chart.height());
        let bar = Rect::new(
            left,
            chart.bottom.saturating_sub(height),
            left.saturating_add(bar_width),
            chart.bottom,
        );
        fill(dc, bar, palette.accent);
    }
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
    let slot_count = i32::try_from(indices.len()).unwrap_or(i32::MAX).max(1);
    for (slot, index) in indices.into_iter().enumerate() {
        let slot = i32::try_from(slot).unwrap_or(i32::MAX);
        let visual_slot = if rtl {
            slot_count.saturating_sub(1).saturating_sub(slot)
        } else {
            slot
        };
        let left = area.left.saturating_add(
            area.width()
                .saturating_mul(visual_slot)
                .checked_div(slot_count)
                .unwrap_or(0),
        );
        let right = area.left.saturating_add(
            area.width()
                .saturating_mul(visual_slot.saturating_add(1))
                .checked_div(slot_count)
                .unwrap_or(area.width()),
        );
        let label = daily_date_label(&daily_usage[index].start_date);
        draw_formatted_text(
            dc,
            &label,
            Rect::new(left, area.top, right, area.bottom),
            popup_font(dc, 9, FW_NORMAL.0 as i32, dpi),
            palette.secondary_text,
            daily_date_text_format(rtl),
        );
    }
}

unsafe fn draw_icon(dc: HDC, rect: Rect, glyph: &str, palette: PopupPalette, dpi: u32) {
    let brush = CreateSolidBrush(COLORREF(palette.surface));
    let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
    let _ = Ellipse(dc, rect.left, rect.top, rect.right, rect.bottom);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    draw_formatted_text(
        dc,
        glyph,
        rect,
        icon_font(16, dpi),
        palette.accent,
        icon_text_format(),
    );
}

fn icon_text_format() -> DRAW_TEXT_FORMAT {
    DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX
}

fn wrapped_text_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    alignment | DT_WORDBREAK | DT_NOPREFIX
}

unsafe fn separator(dc: HDC, y: i32, width: i32, padding: i32, color: u32) {
    fill(dc, Rect::new(padding, y, width - padding, y + 1), color);
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

unsafe fn icon_font(size: i32, dpi: u32) -> HGDIOBJ {
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
            w!("Segoe MDL2 Assets"),
        )
        .0,
    )
}

unsafe fn draw_text(
    dc: HDC,
    value: &str,
    rect: Rect,
    font: HGDIOBJ,
    color: u32,
    rtl: bool,
    single_line: bool,
) {
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    let line_mode = if single_line {
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS
    } else {
        DT_WORDBREAK | DT_END_ELLIPSIS
    };
    draw_formatted_text(
        dc,
        value,
        rect,
        font,
        color,
        alignment | line_mode | DT_NOPREFIX,
    );
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
        f64::from(radius) - 0.5
    } else if x >= width - radius {
        f64::from(width - radius) + 0.5
    } else {
        px
    };
    let center_y = if y < radius {
        f64::from(radius) - 0.5
    } else if y >= height - radius {
        f64::from(height - radius) + 0.5
    } else {
        py
    };
    let distance = ((px - center_x).powi(2) + (py - center_y).powi(2)).sqrt();
    ((f64::from(radius) + 0.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn apply_surface_alpha(pixels: &mut [u32], width: i32, height: i32, radius: i32, background: u32) {
    for y in 0..height {
        for x in 0..width {
            let index = y as usize * width as usize + x as usize;
            let pixel = pixels[index];
            let coverage = rounded_surface_alpha(x, y, width, height, radius);
            if coverage == 0 {
                pixels[index] = 0;
                continue;
            }
            let rgb = pixel & 0x00ff_ffff;
            let base_alpha = if rgb == background { 248_u16 } else { 255_u16 };
            let alpha = ((base_alpha * u16::from(coverage)) / 255) as u8;
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
        daily_date_indices, daily_date_label, daily_date_text_format, daily_usage_bar_height,
        daily_usage_chart_height, daily_usage_height, daily_usage_top, forecast_row_height,
        icon_text_format, pace_detail_top, popup_height_for_content, popup_render_size,
        rounded_surface_alpha, wrapped_text_format,
    };
    use crate::windows::widget::Rect;

    #[test]
    fn rounded_surface_keeps_center_opaque_and_corners_transparent() {
        assert_eq!(rounded_surface_alpha(50, 50, 100, 100, 14), 255);
        assert_eq!(rounded_surface_alpha(0, 0, 100, 100, 14), 0);
        assert!(rounded_surface_alpha(4, 4, 100, 100, 14) > 0);
    }

    #[test]
    fn icon_glyph_is_centered_inside_its_surface() {
        assert_eq!(
            icon_text_format(),
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX
        );
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
    fn measured_content_sets_forecast_rows_and_popup_height() {
        assert_eq!(forecast_row_height(18, 96), 42);
        assert_eq!(forecast_row_height(44, 96), 68);
        assert_eq!(popup_height_for_content(220, 96), 240);
    }

    #[test]
    fn wrapped_pace_summary_pushes_the_optional_detail_down() {
        assert_eq!(pace_detail_top(18, 96), 128);
        assert_eq!(pace_detail_top(44, 96), 154);
    }

    #[test]
    fn daily_usage_layout_reserves_a_fixed_dpi_scaled_chart() {
        assert_eq!(daily_usage_top(200, 96), 212);
        assert_eq!(daily_usage_height(96), 92);
        assert_eq!(daily_usage_height(144), 138);
        assert_eq!(daily_usage_chart_height(96), 53);
    }

    #[test]
    fn daily_usage_bar_height_scales_to_the_largest_day_and_keeps_zero_visible() {
        assert_eq!(daily_usage_bar_height(50, 100, 80), 40);
        assert_eq!(daily_usage_bar_height(0, 0, 80), 4);
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
