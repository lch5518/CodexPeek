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
            FF_SWISS, FW_MEDIUM, FW_NORMAL, FW_SEMIBOLD, HDC, HGDIOBJ, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, NULL_PEN, OUT_DEFAULT_PRECIS, PROOF_QUALITY, TRANSPARENT,
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
const PACE_DETAIL_TOP_LOGICAL: i32 = 208;
const FORECAST_TOP_WITHOUT_DETAIL_LOGICAL: i32 = 244;
const FORECAST_GAP_LOGICAL: i32 = 8;
const FORECAST_SECTION_HEIGHT_LOGICAL: i32 = 60;
const PACE_DETAIL_FALLBACK_HEIGHT_LOGICAL: i32 = 80;

#[derive(Clone, Copy)]
struct PopupLayout {
    width: i32,
    height: i32,
    pace_detail_height: Option<i32>,
    forecast_top: i32,
}

fn forecast_top_for_detail(detail_height: Option<i32>, dpi: u32) -> i32 {
    let minimum = logical_to_physical(FORECAST_TOP_WITHOUT_DETAIL_LOGICAL, dpi);
    detail_height
        .map(|height| {
            logical_to_physical(PACE_DETAIL_TOP_LOGICAL, dpi)
                .saturating_add(height.max(0))
                .saturating_add(logical_to_physical(FORECAST_GAP_LOGICAL, dpi))
        })
        .unwrap_or(minimum)
        .max(minimum)
}

fn popup_height_for_forecasts(forecast_top: i32, forecast_count: usize, dpi: u32) -> i32 {
    let section_count = i32::try_from(forecast_count).unwrap_or(i32::MAX);
    forecast_top.saturating_add(
        logical_to_physical(FORECAST_SECTION_HEIGHT_LOGICAL, dpi).saturating_mul(section_count),
    )
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
    let width = logical_to_physical(POPUP_WIDTH_LOGICAL, dpi);
    let pace_detail_height = measure_pace_detail_height(presentation, width, dpi, rtl);
    let forecast_top = forecast_top_for_detail(pace_detail_height, dpi);
    let height = popup_height_for_forecasts(forecast_top, presentation.forecasts.len(), dpi);
    let layout = PopupLayout {
        width,
        height,
        pace_detail_height,
        forecast_top,
    };
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
    let bounds = place_popup(
        Rect::new(anchor.left, anchor.top, anchor.right, anchor.bottom),
        Rect::new(
            monitor_info.rcWork.left,
            monitor_info.rcWork.top,
            monitor_info.rcWork.right,
            monitor_info.rcWork.bottom,
        ),
        (layout.width, layout.height),
        logical_to_physical(8, dpi),
        logical_to_physical(8, dpi),
    );
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
    if let Err(error) = render(hwnd, presentation, popup_palette(light), dpi, rtl, layout) {
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

/// 소비 속도 상세 문구를 현재 DPI와 글꼴로 측정해 필요한 실제 높이를 반환합니다.
///
/// `presentation`에 상세 문구가 없으면 `None`을 반환합니다. 화면 DC 또는 글꼴 측정이 실패하면
/// 문구가 잘리지 않도록 보수적인 4줄 높이를 사용하며, 생성한 GDI 객체와 DC는 호출 안에서 정리합니다.
unsafe fn measure_pace_detail_height(
    presentation: &UsagePopupPresentation,
    popup_width: i32,
    dpi: u32,
    rtl: bool,
) -> Option<i32> {
    let detail = presentation.pace_detail.as_deref()?;
    let padding = logical_to_physical(20, dpi);
    let icon = logical_to_physical(36, dpi);
    let gap = logical_to_physical(12, dpi);
    let text_width = popup_width
        .saturating_sub(padding.saturating_mul(2))
        .saturating_sub(icon)
        .saturating_sub(gap)
        .max(1);
    let fallback = logical_to_physical(PACE_DETAIL_FALLBACK_HEIGHT_LOGICAL, dpi);
    let dc = GetDC(None);
    if dc == HDC::default() {
        return Some(fallback);
    }

    let font = popup_font(dc, 11, FW_NORMAL.0 as i32, dpi);
    let old_font = SelectObject(dc, font);
    let mut text: Vec<u16> = detail.encode_utf16().collect();
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: text_width,
        bottom: 0,
    };
    let measured = DrawTextW(dc, &mut text, &mut rect, pace_detail_measure_format(rtl));
    SelectObject(dc, old_font);
    let _ = DeleteObject(font);
    let _ = ReleaseDC(None, dc);

    Some(if measured > 0 {
        measured.saturating_add(logical_to_physical(2, dpi))
    } else {
        fallback
    })
}

unsafe fn render(
    hwnd: HWND,
    presentation: &UsagePopupPresentation,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
    layout: PopupLayout,
) -> io::Result<()> {
    let pixel_count = usize::try_from(layout.width)
        .ok()
        .and_then(|width| {
            usize::try_from(layout.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid popup bitmap size"))?;
    let screen_dc = GetDC(None);
    let memory_dc = CreateCompatibleDC(Some(screen_dc));
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: layout.width,
            biHeight: -layout.height,
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
    paint_content(memory_dc, presentation, palette, dpi, rtl, layout);
    apply_surface_alpha(
        std::slice::from_raw_parts_mut(bits.cast::<u32>(), pixel_count),
        layout.width,
        layout.height,
        logical_to_physical(14, dpi),
        palette.background,
    );
    let source = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: layout.width,
        cy: layout.height,
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
    layout: PopupLayout,
) {
    let width = layout.width;
    let height = layout.height;
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

    draw_icon(dc, icon_rect(section(14)), "\u{E77B}", palette, dpi);
    draw_text(
        dc,
        &presentation.profile_label,
        text_rect(section(12), section(34)),
        popup_font(dc, 14, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    draw_text(
        dc,
        &presentation.profile_note,
        text_rect(section(34), section(56)),
        popup_font(dc, 12, FW_NORMAL.0 as i32, dpi),
        palette.secondary_text,
        rtl,
        true,
    );
    separator(dc, section(64), width, padding, palette.separator);

    draw_icon(dc, icon_rect(section(74)), "\u{E9D2}", palette, dpi);
    draw_text(
        dc,
        presentation.usage_label.as_deref().unwrap_or("Codex usage"),
        text_rect(section(70), section(92)),
        popup_font(dc, 14, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    draw_metric(
        dc,
        Rect::new(text_left, section(94), text_right, section(130)),
        &presentation.metric_label,
        presentation.metric_percent,
        palette,
        dpi,
        rtl,
    );
    let track = Rect::new(text_left, section(134), text_right, section(138));
    fill(dc, track, palette.separator);
    if let Some(percent) = presentation.metric_percent {
        fill(
            dc,
            Rect::new(
                track.left,
                track.top,
                track.left + (track.width() * i32::from(percent)) / 100,
                track.bottom,
            ),
            palette.accent,
        );
    }
    let reset = presentation
        .reset_text
        .as_deref()
        .map(|value| format!("{}: {value}", presentation.reset_label))
        .unwrap_or_else(|| format!("{}: --", presentation.reset_label));
    draw_text(
        dc,
        &reset,
        text_rect(section(141), section(159)),
        popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
        palette.secondary_text,
        rtl,
        true,
    );
    draw_text(
        dc,
        &format!("{}: {}", presentation.status_label, presentation.status),
        text_rect(section(159), section(179)),
        popup_font(dc, 11, FW_MEDIUM.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    separator(dc, section(180), width, padding, palette.separator);

    draw_icon(dc, icon_rect(section(190)), "\u{E9D9}", palette, dpi);
    draw_text(
        dc,
        &presentation.pace_summary,
        text_rect(section(186), section(208)),
        popup_font(dc, 13, FW_SEMIBOLD.0 as i32, dpi),
        palette.text,
        rtl,
        true,
    );
    if let (Some(detail), Some(detail_height)) = (
        presentation.pace_detail.as_deref(),
        layout.pace_detail_height,
    ) {
        let detail_top = section(PACE_DETAIL_TOP_LOGICAL);
        draw_formatted_text(
            dc,
            detail,
            text_rect(detail_top, detail_top.saturating_add(detail_height)),
            popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
            palette.secondary_text,
            pace_detail_text_format(rtl),
        );
    }

    let mut top = layout.forecast_top;
    for forecast in &presentation.forecasts {
        separator(dc, top, width, padding, palette.separator);
        draw_icon(
            dc,
            icon_rect(top.saturating_add(section(12))),
            "\u{E95E}",
            palette,
            dpi,
        );
        draw_text(
            dc,
            &forecast.label,
            text_rect(
                top.saturating_add(section(8)),
                top.saturating_add(section(30)),
            ),
            popup_font(dc, 12, FW_SEMIBOLD.0 as i32, dpi),
            palette.text,
            rtl,
            true,
        );
        draw_text(
            dc,
            &forecast.detail,
            text_rect(
                top.saturating_add(section(28)),
                top.saturating_add(section(56)),
            ),
            popup_font(dc, 11, FW_NORMAL.0 as i32, dpi),
            palette.secondary_text,
            rtl,
            false,
        );
        top = top.saturating_add(section(FORECAST_SECTION_HEIGHT_LOGICAL));
    }
}

unsafe fn draw_metric(
    dc: HDC,
    rect: Rect,
    label: &str,
    percent: Option<u8>,
    palette: PopupPalette,
    dpi: u32,
    rtl: bool,
) {
    draw_text(
        dc,
        label,
        Rect::new(
            rect.left,
            rect.top,
            rect.right,
            rect.top + logical_to_physical(16, dpi),
        ),
        popup_font(dc, 10, FW_NORMAL.0 as i32, dpi),
        palette.secondary_text,
        rtl,
        true,
    );
    draw_text(
        dc,
        &percent.map_or_else(|| "--".to_owned(), |value| format!("{value}%")),
        Rect::new(
            rect.left,
            rect.top + logical_to_physical(15, dpi),
            rect.right,
            rect.bottom,
        ),
        popup_font(dc, 15, FW_SEMIBOLD.0 as i32, dpi),
        palette.accent,
        rtl,
        true,
    );
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

fn pace_detail_text_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    alignment | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX
}

fn pace_detail_measure_format(rtl: bool) -> DRAW_TEXT_FORMAT {
    let alignment = if rtl {
        DT_RIGHT | DT_RTLREADING
    } else {
        DT_LEFT
    };
    alignment | DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT
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
        DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_RTLREADING, DT_SINGLELINE,
        DT_VCENTER, DT_WORDBREAK,
    };

    use super::{
        forecast_top_for_detail, icon_text_format, pace_detail_text_format,
        popup_height_for_forecasts, rounded_surface_alpha,
    };

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
    fn pace_detail_uses_wrapping_in_ltr_and_rtl() {
        assert_eq!(
            pace_detail_text_format(false),
            DT_LEFT | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX
        );
        assert_eq!(
            pace_detail_text_format(true),
            DT_RIGHT | DT_RTLREADING | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX
        );
    }

    #[test]
    fn measured_pace_detail_pushes_following_content_below_text() {
        assert_eq!(forecast_top_for_detail(None, 96), 244);
        assert_eq!(forecast_top_for_detail(Some(56), 96), 272);
        assert_eq!(popup_height_for_forecasts(272, 1, 96), 332);
    }
}
