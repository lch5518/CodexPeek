//! 사용량 상세 팝업의 표현 모델과 DPI 독립 레이아웃 계산입니다.

use super::{widget::Rect, WidgetViewModel};
use crate::Language;

pub(crate) const POPUP_WIDTH_LOGICAL: i32 = 360;

/// owner-draw 메뉴 항목의 시각적 역할입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuItemKind {
    Command,
    Submenu,
    Info,
}

/// 메뉴 항목 역할에 맞는 논리 픽셀 높이를 반환합니다.
pub(crate) const fn menu_item_height(kind: MenuItemKind) -> i32 {
    match kind {
        MenuItemKind::Command | MenuItemKind::Submenu => 32,
        MenuItemKind::Info => 40,
    }
}

/// UTF-16 레이블 길이를 바탕으로 메뉴 항목의 논리 픽셀 너비를 제한해 계산합니다.
pub(crate) fn menu_item_width(text_units: usize) -> i32 {
    let estimated = text_units.saturating_mul(7).saturating_add(72);
    i32::try_from(estimated).unwrap_or(i32::MAX).clamp(220, 520)
}

/// 상세 팝업과 owner-draw 메뉴가 공유하는 색상 팔레트입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PopupPalette {
    pub(crate) background: u32,
    pub(crate) surface: u32,
    pub(crate) text: u32,
    pub(crate) secondary_text: u32,
    pub(crate) accent: u32,
    pub(crate) separator: u32,
    pub(crate) selection: u32,
}

/// Windows 시스템 테마에 맞는 Fluent Compact 색상 팔레트를 반환합니다.
pub(crate) const fn popup_palette(light: bool) -> PopupPalette {
    if light {
        PopupPalette {
            background: 0x00f9_f9f9,
            surface: 0x00ed_f7ed,
            text: 0x001c_1c1c,
            secondary_text: 0x0060_6060,
            accent: 0x0074_c748,
            separator: 0x00df_dfdf,
            selection: 0x00ee_f5ee,
        }
    } else {
        PopupPalette {
            background: 0x0020_2020,
            surface: 0x002b_352b,
            text: 0x00f2_f2f2,
            secondary_text: 0x00b8_b8b8,
            accent: 0x0074_c748,
            separator: 0x0040_4040,
            selection: 0x0032_3a32,
        }
    }
}

/// 사용량 상세 팝업에 표시할 창별 예측 문구입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PopupForecastLine {
    pub(crate) label: String,
    pub(crate) detail: String,
}

/// 네이티브 렌더러가 문자열을 해석하지 않고 소비하는 사용량 상세 표현입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsagePopupPresentation {
    pub(crate) profile_label: String,
    pub(crate) profile_note: String,
    pub(crate) usage_label: Option<String>,
    pub(crate) current_label: String,
    pub(crate) current_percent: Option<u8>,
    pub(crate) remaining_label: String,
    pub(crate) remaining_percent: Option<u8>,
    pub(crate) reset_label: String,
    pub(crate) reset_text: Option<String>,
    pub(crate) status_label: String,
    pub(crate) status: String,
    pub(crate) pace_summary: String,
    pub(crate) pace_detail: Option<String>,
    pub(crate) forecasts: Vec<PopupForecastLine>,
}

/// 현재 위젯 복사본을 상세 팝업 전용 구조로 변환합니다.
///
/// `view`는 민감 정보가 제거된 UI 복사본이어야 하며, `language`는 레이블 선택에 사용됩니다.
/// 반환값은 Win32 렌더러가 추가 문자열 파싱 없이 사용할 수 있습니다.
pub(crate) fn usage_popup_presentation(
    view: &WidgetViewModel,
    language: Language,
) -> UsagePopupPresentation {
    let row = view.secondary.as_ref().or(view.primary.as_ref());
    let used_percent = row.map(|row| row.used_percent.clamp(0.0, 100.0).round() as u8);
    let forecast = |label: crate::LocalizationKey,
                    row: Option<&super::UsageRowView>|
     -> Option<PopupForecastLine> {
        row.and_then(|row| row.forecast.line())
            .map(|detail| PopupForecastLine {
                label: crate::localized_text(label, language).to_owned(),
                detail: detail.to_owned(),
            })
    };

    UsagePopupPresentation {
        profile_label: view.usage_profile_label.clone(),
        profile_note: crate::localized_text(
            crate::LocalizationKey::UsageProfileCliUnchanged,
            language,
        )
        .to_owned(),
        usage_label: row.map(|_| {
            crate::domain::window_kind_label(crate::WindowKind::Secondary, language).to_owned()
        }),
        current_label: crate::app::current_usage_label(language).to_owned(),
        current_percent: used_percent,
        remaining_label: crate::app::remaining_usage_label(language).to_owned(),
        remaining_percent: used_percent.map(|percent| 100_u8.saturating_sub(percent)),
        reset_label: crate::app::reset_at_label(language).to_owned(),
        reset_text: row
            .and_then(|row| (!row.reset_text.is_empty()).then(|| row.reset_text.clone())),
        status_label: crate::app::status_label(language).to_owned(),
        status: view.status.clone(),
        pace_summary: view.consumption_pace.summary.clone(),
        pace_detail: view.consumption_pace.detail.clone(),
        forecasts: [
            forecast(
                crate::LocalizationKey::PrimaryWindowLabel,
                view.primary.as_ref(),
            ),
            forecast(
                crate::LocalizationKey::SecondaryWindowLabel,
                view.secondary.as_ref(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

/// 앵커와 현재 모니터 작업 영역을 기준으로 팝업의 물리 픽셀 경계를 계산합니다.
///
/// `size`는 `(너비, 높이)`, `margin`과 `gap`은 물리 픽셀 단위입니다. 가능하면 앵커 위에
/// 배치하고 공간이 부족하면 아래에 배치하며, 반환 경계는 항상 작업 영역 안에 제한됩니다.
pub(crate) fn place_popup(
    anchor: Rect,
    work_area: Rect,
    size: (i32, i32),
    margin: i32,
    gap: i32,
) -> Rect {
    let margin = margin.max(0);
    let gap = gap.max(0);
    let usable = Rect::new(
        work_area.left.saturating_add(margin),
        work_area.top.saturating_add(margin),
        work_area.right.saturating_sub(margin),
        work_area.bottom.saturating_sub(margin),
    );
    let width = size.0.max(0).min(usable.width().max(0));
    let height = size.1.max(0).min(usable.height().max(0));
    let max_left = usable.right.saturating_sub(width);
    let left = anchor.left.clamp(usable.left, max_left.max(usable.left));
    let above = anchor.top.saturating_sub(gap).saturating_sub(height);
    let below = anchor.bottom.saturating_add(gap);
    let max_top = usable.bottom.saturating_sub(height);
    let top =
        if above >= usable.top { above } else { below }.clamp(usable.top, max_top.max(usable.top));
    Rect::new(left, top, left + width, top + height)
}

/// 접근성 환경에서 커스텀 팝업을 사용할 수 있는지 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupRenderMode {
    Custom,
    Native,
}

/// 표준 툴팁 표시 요청을 커스텀 화면으로 대체할지 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TooltipShowDecision {
    ShowCustomAndParkNative,
    ShowNative,
}

/// 접근성 정책과 커스텀 창 생성 결과를 조합해 표준 툴팁 억제 여부를 결정합니다.
pub(crate) const fn tooltip_show_decision(
    mode: PopupRenderMode,
    custom_render_succeeded: bool,
) -> TooltipShowDecision {
    if matches!(mode, PopupRenderMode::Custom) && custom_render_succeeded {
        TooltipShowDecision::ShowCustomAndParkNative
    } else {
        TooltipShowDecision::ShowNative
    }
}

/// 고대비 또는 스크린리더 사용 시 Windows 기본 UI로 안전하게 폴백합니다.
pub(crate) const fn popup_render_mode(high_contrast: bool, screen_reader: bool) -> PopupRenderMode {
    if high_contrast || screen_reader {
        PopupRenderMode::Native
    } else {
        PopupRenderMode::Custom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows::{
        ConsumptionPaceState, ConsumptionPaceView, ForecastView, UsageRowView, WidgetDataState,
    };
    use crate::UsageLevel;

    fn row(
        label: &str,
        used_percent: f64,
        reset_text: &str,
        forecast: ForecastView,
    ) -> UsageRowView {
        UsageRowView {
            label: label.to_owned(),
            used_percent,
            display_percent: used_percent,
            percent_text: format!("{used_percent:.0}%"),
            reset_text: reset_text.to_owned(),
            level: UsageLevel::Stable,
            forecast,
        }
    }

    fn ready_view() -> WidgetViewModel {
        WidgetViewModel {
            usage_profile_label: "Work".to_owned(),
            primary: Some(row(
                "5h",
                12.0,
                "2026-08-11 15:00",
                ForecastView::Collecting {
                    line: "Collecting primary samples".to_owned(),
                },
            )),
            secondary: Some(row(
                "7d",
                34.4,
                "2026-08-18 10:23",
                ForecastView::ForecastAvailable {
                    line: "About 52% will remain".to_owned(),
                },
            )),
            status: "Healthy · Polling".to_owned(),
            last_success: "just now".to_owned(),
            is_stale: false,
            taskbar_label: "7d".to_owned(),
            taskbar_tooltip: "legacy fallback".to_owned(),
            reset_credits_text: None,
            data_state: WidgetDataState::Ready,
            consumption_pace: ConsumptionPaceView {
                state: ConsumptionPaceState::Comfortable,
                summary: "Usage pace: Comfortable".to_owned(),
                detail: Some("Used 2% over the last 2 hours".to_owned()),
            },
        }
    }

    #[test]
    fn presentation_prefers_weekly_row_and_keeps_both_percentages() {
        let presentation = usage_popup_presentation(&ready_view(), Language::English);

        assert_eq!(presentation.profile_label, "Work");
        assert_eq!(presentation.usage_label.as_deref(), Some("Weekly"));
        assert_eq!(presentation.current_label, "Current usage");
        assert_eq!(presentation.current_percent, Some(34));
        assert_eq!(presentation.remaining_label, "Remaining");
        assert_eq!(presentation.remaining_percent, Some(66));
        assert_eq!(presentation.reset_text.as_deref(), Some("2026-08-18 10:23"));
        assert_eq!(
            presentation.forecasts,
            vec![
                PopupForecastLine {
                    label: "Primary window".to_owned(),
                    detail: "Collecting primary samples".to_owned(),
                },
                PopupForecastLine {
                    label: "Secondary window".to_owned(),
                    detail: "About 52% will remain".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn weekly_heading_uses_the_localized_semantic_label_in_every_language() {
        for &language in Language::ALL {
            let presentation = usage_popup_presentation(&ready_view(), language);
            let expected = crate::UsageWindow::new(crate::WindowKind::Secondary, 0.0, None, None)
                .expect("valid semantic weekly window")
                .period_label(language);

            assert_eq!(
                presentation.usage_label.as_deref(),
                Some(expected.as_str()),
                "language={language:?}"
            );
        }
    }

    #[test]
    fn presentation_uses_empty_metrics_when_no_usage_row_exists() {
        let mut view = ready_view();
        view.primary = None;
        view.secondary = None;
        view.data_state = WidgetDataState::Loading;

        let presentation = usage_popup_presentation(&view, Language::English);

        assert_eq!(presentation.usage_label, None);
        assert_eq!(presentation.current_percent, None);
        assert_eq!(presentation.remaining_percent, None);
        assert_eq!(presentation.reset_text, None);
        assert!(presentation.forecasts.is_empty());
    }

    #[test]
    fn popup_prefers_above_anchor_and_stays_inside_work_area() {
        let bounds = place_popup(
            Rect::new(1_800, 1_000, 1_900, 1_040),
            Rect::new(0, 0, 1_920, 1_080),
            (360, 500),
            8,
            8,
        );

        assert_eq!(bounds, Rect::new(1_552, 492, 1_912, 992));
    }

    #[test]
    fn popup_uses_below_anchor_when_above_has_insufficient_space() {
        let bounds = place_popup(
            Rect::new(20, 10, 120, 50),
            Rect::new(0, 0, 1_280, 720),
            (360, 500),
            8,
            8,
        );

        assert_eq!(bounds, Rect::new(20, 58, 380, 558));
        assert!(bounds.is_inside(Rect::new(8, 8, 1_272, 712)));
    }

    #[test]
    fn accessibility_modes_use_native_fallback() {
        assert_eq!(popup_render_mode(false, false), PopupRenderMode::Custom);
        assert_eq!(popup_render_mode(true, false), PopupRenderMode::Native);
        assert_eq!(popup_render_mode(false, true), PopupRenderMode::Native);
    }

    #[test]
    fn native_tooltip_is_parked_only_after_custom_render_succeeds() {
        assert_eq!(
            tooltip_show_decision(PopupRenderMode::Custom, true),
            TooltipShowDecision::ShowCustomAndParkNative
        );
        assert_eq!(
            tooltip_show_decision(PopupRenderMode::Custom, false),
            TooltipShowDecision::ShowNative
        );
        assert_eq!(
            tooltip_show_decision(PopupRenderMode::Native, true),
            TooltipShowDecision::ShowNative
        );
    }

    #[test]
    fn light_and_dark_palettes_keep_one_accent_and_distinct_surfaces() {
        let light = popup_palette(true);
        let dark = popup_palette(false);

        assert_eq!(light.accent, dark.accent);
        assert_ne!(light.background, dark.background);
        assert_ne!(light.text, dark.text);
        assert_ne!(light.selection, dark.selection);
    }

    #[test]
    fn owner_draw_menu_uses_compact_rows_and_a_taller_info_banner() {
        assert_eq!(menu_item_height(MenuItemKind::Command), 32);
        assert_eq!(menu_item_height(MenuItemKind::Submenu), 32);
        assert_eq!(menu_item_height(MenuItemKind::Info), 40);
    }

    #[test]
    fn owner_draw_menu_width_has_readable_minimum_and_screen_safe_ceiling() {
        assert_eq!(menu_item_width(5), 220);
        assert_eq!(menu_item_width(30), 282);
        assert_eq!(menu_item_width(200), 520);
    }
}
