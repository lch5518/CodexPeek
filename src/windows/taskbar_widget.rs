//! 작업 표시줄 전용 사용량 창 표현과 DPI 레이아웃입니다.

use super::{
    widget::logical_to_physical, ConsumptionPaceState, UsageRowView, WidgetDataState,
    WidgetViewModel,
};
use crate::windows::widget::Rect;

/// 작업 표시줄에 여유 공간이 있을 때 사용하는 위젯의 기본 논리 너비입니다.
pub const TASKBAR_WIDTH_LOGICAL: i32 = 208;

/// 작업 표시줄 아이콘과 겹치지 않으면서 내용을 유지할 수 있는 최소 논리 너비입니다.
pub const TASKBAR_MIN_WIDTH_LOGICAL: i32 = 88;

/// 연결 상태에 따라 프로필 헤더와 기존 축약 본문을 분리한 위젯 표면 레이아웃입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetSurfaceLayout {
    /// 분리된 플로팅 위젯에서 선택 프로필 이름을 표시할 영역입니다.
    pub profile_header: Option<Rect>,
    /// 기존 축약 사용량 본문이 차지하는 영역입니다.
    pub content: Rect,
}

/// 위젯의 작업표시줄 연결 상태에 맞는 표면 레이아웃을 계산합니다.
///
/// `attached_to_taskbar`가 참이면 기존 본문 영역을 한 픽셀도 변경하지 않습니다. 거짓이면
/// 상단에 선택 프로필 헤더를 예약하며, 입력과 반환 좌표는 물리 픽셀입니다.
pub fn widget_surface_layout(
    width: i32,
    height: i32,
    dpi: u32,
    attached_to_taskbar: bool,
) -> WidgetSurfaceLayout {
    let width = width.max(0);
    let height = height.max(0);
    if attached_to_taskbar {
        return WidgetSurfaceLayout {
            profile_header: None,
            content: Rect::new(0, 0, width, height),
        };
    }

    let header_bottom = logical_to_physical(18, dpi).min(height);
    let horizontal_inset = logical_to_physical(8, dpi).min(width / 2);
    let header_top = logical_to_physical(2, dpi).min(header_bottom);
    WidgetSurfaceLayout {
        profile_header: Some(Rect::new(
            horizontal_inset,
            header_top,
            width - horizontal_inset,
            header_bottom,
        )),
        content: Rect::new(0, header_bottom, width, height),
    }
}

/// 표면 레이아웃이 프로필 헤더를 제공할 때 표시할 선택 프로필 이름을 반환합니다.
///
/// 작업표시줄에 연결된 축약 본문이나 빈 라벨에는 `None`을 반환합니다. 반환 문자열은
/// `WidgetViewModel`이 소유하며 경로나 계정 식별 정보를 새로 만들지 않습니다.
pub fn profile_header_text(view: &WidgetViewModel, layout: WidgetSurfaceLayout) -> Option<&str> {
    layout
        .profile_header
        .and((!view.usage_profile_label.is_empty()).then_some(view.usage_profile_label.as_str()))
}

/// hover 밝기를 약 150ms 동안 현재 값에서 목표 값으로 이동시킵니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoverTransition {
    value: u8,
    target: u8,
}

/// 저장된 UTF-16 툴팁과 새 문자열이 달라 네이티브 갱신이 필요한지 판단합니다.
///
/// 동일한 문자열에는 `false`를 반환해, 표시 중인 Windows 툴팁에 불필요한 갱신 메시지를
/// 보내지 않도록 합니다. 입력 버퍼의 소유권이나 문자열 종료 방식은 변경하지 않습니다.
pub(crate) fn tooltip_text_needs_update(previous: &[u16], next: &[u16]) -> bool {
    previous != next
}

impl HoverTransition {
    /// 마우스 진입 여부에 맞춰 새 목표를 설정하며 현재 값은 유지합니다.
    pub fn set_hovered(&mut self, hovered: bool) {
        self.target = if hovered { u8::MAX } else { 0 };
    }

    /// 한 프레임 진행하고 추가 프레임이 필요한지 반환합니다.
    pub fn tick(&mut self) -> bool {
        if self.value == self.target {
            return false;
        }
        if self.value < self.target {
            self.value = self.value.saturating_add(26).min(self.target);
        } else {
            self.value = self.value.saturating_sub(26).max(self.target);
        }
        self.value != self.target
    }

    /// 현재 hover 밝기 값을 반환합니다.
    pub const fn value(self) -> u8 {
        self.value
    }
}

/// 작업 표시줄에서 사용하는 위험 표현입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarRisk {
    /// 사용량이 70% 미만입니다.
    Healthy,
    /// 사용량이 70% 이상 90% 미만입니다.
    Warning,
    /// 사용량이 90% 이상입니다.
    Critical,
    /// 첫 사용량을 불러오는 중입니다.
    Loading,
    /// 최근 조회가 실패했습니다.
    Error,
}

impl TaskbarRisk {
    /// 사용한 비율을 작업 표시줄 전용 위험 단계로 변환합니다.
    pub fn from_percent(percent: f64) -> Self {
        if percent >= 90.0 {
            Self::Critical
        } else if percent >= 70.0 {
            Self::Warning
        } else {
            Self::Healthy
        }
    }
}

/// 작업 표시줄 좌측 상단 상태점이 표현하는 소비 속도 또는 조회 상태입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarIndicator {
    /// 초기화까지 여유 있는 소비 속도입니다.
    Comfortable,
    /// 초기화까지 보통인 소비 속도입니다.
    Normal,
    /// 초기화 전 소진 위험이 있는 빠른 소비 속도입니다.
    Fast,
    /// 측정 중이거나 판단할 수 없는 중립 상태입니다.
    Neutral,
    /// 최근 조회가 실패한 오류 상태입니다.
    Error,
}

/// 상태점과 진행 막대가 서로 다른 의미를 유지하도록 분리한 시각 상태입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarVisualState {
    /// 좌측 상단 상태점 또는 오류 표시에 사용할 상태입니다.
    pub indicator: TaskbarIndicator,
    /// 현재 사용률 진행 막대에 사용할 위험 단계입니다.
    pub progress_risk: TaskbarRisk,
}

/// 위젯 표시 모델을 상태점과 현재 사용률 진행 막대의 시각 상태로 변환합니다.
///
/// 조회 오류와 로딩은 상태점보다 우선하며, 진행 막대는 조회 상태와 무관하게 보조 사용량 창을
/// 우선한 현재 사용률을 사용합니다. 표시 가능한 행이 없으면 로딩 색을 반환합니다.
pub fn taskbar_visual_state(view: &WidgetViewModel) -> TaskbarVisualState {
    let progress_risk = select_weekly_row(view.primary.as_ref(), view.secondary.as_ref())
        .map(|row| TaskbarRisk::from_percent(row.used_percent))
        .unwrap_or(TaskbarRisk::Loading);
    let indicator = match view.data_state {
        WidgetDataState::Error => TaskbarIndicator::Error,
        WidgetDataState::Loading => TaskbarIndicator::Neutral,
        WidgetDataState::Ready => match view.consumption_pace.state {
            ConsumptionPaceState::Comfortable => TaskbarIndicator::Comfortable,
            ConsumptionPaceState::Normal => TaskbarIndicator::Normal,
            ConsumptionPaceState::Fast | ConsumptionPaceState::Exhausted => TaskbarIndicator::Fast,
            ConsumptionPaceState::Measuring
            | ConsumptionPaceState::InsufficientActivity
            | ConsumptionPaceState::Unavailable
            | ConsumptionPaceState::Disabled => TaskbarIndicator::Neutral,
        },
    };
    TaskbarVisualState {
        indicator,
        progress_risk,
    }
}

/// 보조 사용량 창을 주간 값으로 우선 선택하고 없으면 유일한 기본 창을 반환합니다.
pub fn select_weekly_row<'a>(
    primary: Option<&'a UsageRowView>,
    secondary: Option<&'a UsageRowView>,
) -> Option<&'a UsageRowView> {
    secondary.or(primary)
}

/// 작업 표시줄 본문에 표시할 두 사용량 창을 5시간 우선 순서로 반환합니다.
///
/// `primary`가 있으면 첫 번째 행에, `secondary`가 있으면 두 번째 행에 배치합니다. 기본 창이
/// 없는 계정은 보조 창을 첫 번째 행으로 올려 빈 행을 만들지 않습니다.
pub fn taskbar_rows<'a>(
    primary: Option<&'a UsageRowView>,
    secondary: Option<&'a UsageRowView>,
) -> [Option<&'a UsageRowView>; 2] {
    match primary {
        Some(primary) => [Some(primary), secondary],
        None => [secondary, None],
    }
}

/// 진행 막대 너비와 표시 비율을 사용해 실제 채움 너비를 계산합니다.
///
/// 표시 비율은 0~100%로 제한되며, 잘못된 음수나 초과 값이 레이아웃 밖으로 그려지지 않게 합니다.
pub(crate) fn progress_fill_width(width: i32, display_percent: f64) -> i32 {
    (f64::from(width) * display_percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

/// 사용 가능한 너비에 맞춘 작업 표시줄 위젯 표현 단계입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarLayoutMode {
    /// 상태점과 두 사용량 창의 기간 라벨·사용률을 표시합니다.
    Full,
    /// 기간 라벨을 생략하고 상태점과 두 사용량 창의 사용률을 표시합니다.
    Compact,
    /// 첫 번째 사용량 창의 사용률만 중앙에 표시합니다.
    Minimal,
}

/// 작업 표시줄 글라스 위젯의 고정 영역입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarLayout {
    /// 현재 너비에 맞춘 표현 단계입니다.
    pub mode: TaskbarLayoutMode,
    /// 전체 클라이언트 영역입니다.
    pub window: Rect,
    /// 상태 점 영역입니다.
    pub dot: Option<Rect>,
    /// 첫 번째 사용량 창(보통 5시간)의 레이블 영역입니다.
    pub label: Option<Rect>,
    /// 두 번째 사용량 창(보통 주간)의 레이블 영역입니다.
    pub secondary_label: Option<Rect>,
    /// 첫 번째 사용량 창의 오른쪽 고정 퍼센트 영역입니다.
    pub percent: Rect,
    /// 두 번째 사용량 창의 오른쪽 고정 퍼센트 영역입니다.
    pub secondary_percent: Option<Rect>,
    /// 첫 번째 사용량 창의 진행 막대 영역입니다.
    pub progress: Rect,
    /// 두 번째 사용량 창의 진행 막대 영역입니다.
    pub secondary_progress: Option<Rect>,
}

impl TaskbarLayout {
    /// 실제 클라이언트 크기와 DPI에 맞춰 고정 영역을 계산합니다.
    pub fn for_size(width: i32, height: i32, dpi: u32) -> Self {
        Self::for_rows(width, height, dpi, true)
    }

    /// 실제 클라이언트 크기와 표시할 행 수에 맞춰 고정 영역을 계산합니다.
    pub(crate) fn for_rows(width: i32, height: i32, dpi: u32, include_secondary: bool) -> Self {
        let scale = |value| logical_to_physical(value, dpi);
        let mode = if width >= scale(140) {
            TaskbarLayoutMode::Full
        } else if width >= scale(100) {
            TaskbarLayoutMode::Compact
        } else {
            TaskbarLayoutMode::Minimal
        };
        let inset = scale(11).min((width / 4).max(1));
        let row_count = if mode == TaskbarLayoutMode::Minimal || !include_secondary {
            1
        } else {
            2
        };
        let top_padding = scale(2).max(1).min(height.max(1));
        let bottom_padding = scale(3)
            .max(1)
            .min(height.saturating_sub(top_padding).max(1));
        let row_gap = if row_count == 2 { scale(2).max(1) } else { 0 };
        let available_height = height
            .saturating_sub(top_padding)
            .saturating_sub(bottom_padding)
            .saturating_sub(row_gap)
            .max(row_count);
        let primary_row_height = (available_height / row_count).max(1);
        let secondary_row_height = (available_height - primary_row_height).max(1);
        let primary_top = top_padding;
        let primary_bottom = primary_top + primary_row_height;
        let secondary_top = primary_bottom + row_gap;
        let secondary_bottom = secondary_top + secondary_row_height;
        let progress_height = scale(2).max(1).min(
            (primary_row_height
                .min(secondary_row_height)
                .saturating_sub(2))
            .max(1),
        );
        let primary_progress_top = primary_bottom.saturating_sub(progress_height);
        let secondary_progress_top = secondary_bottom.saturating_sub(progress_height);
        let primary_text_bottom = primary_progress_top
            .saturating_sub(1)
            .max(primary_top + 1)
            .min(primary_progress_top);
        let secondary_text_bottom = secondary_progress_top
            .saturating_sub(1)
            .max(secondary_top + 1)
            .min(secondary_progress_top);
        let dot_size = scale(6).min(primary_row_height.max(1));
        let dot_top = primary_top + primary_row_height.saturating_sub(dot_size) / 2;
        let label_left = inset + dot_size + scale(8);
        let percent_width = scale(42).min((width / 3).max(1));
        let full_percent_left = (width - inset - percent_width).max(label_left + scale(8));
        let label_right = full_percent_left.saturating_sub(scale(4).max(1));
        let (dot, label, percent, secondary_label, secondary_percent) = match mode {
            TaskbarLayoutMode::Full => (
                Some(Rect::new(
                    inset,
                    dot_top,
                    inset + dot_size,
                    dot_top + dot_size,
                )),
                Some(Rect::new(
                    label_left,
                    primary_top,
                    label_right,
                    primary_text_bottom,
                )),
                Rect::new(
                    full_percent_left,
                    primary_top,
                    width - inset,
                    primary_text_bottom,
                ),
                Some(Rect::new(
                    label_left,
                    secondary_top,
                    label_right,
                    secondary_text_bottom,
                )),
                Some(Rect::new(
                    full_percent_left,
                    secondary_top,
                    width - inset,
                    secondary_text_bottom,
                )),
            ),
            TaskbarLayoutMode::Compact => (
                Some(Rect::new(
                    inset,
                    dot_top,
                    inset + dot_size,
                    dot_top + dot_size,
                )),
                None,
                Rect::new(label_left, primary_top, width - inset, primary_text_bottom),
                None,
                Some(Rect::new(
                    label_left,
                    secondary_top,
                    width - inset,
                    secondary_text_bottom,
                )),
            ),
            TaskbarLayoutMode::Minimal => (
                None,
                None,
                Rect::new(inset, primary_top, width - inset, primary_text_bottom),
                None,
                None,
            ),
        };

        Self {
            mode,
            window: Rect::new(0, 0, width, height),
            dot,
            label,
            secondary_label,
            percent,
            secondary_percent,
            progress: Rect::new(inset, primary_progress_top, width - inset, primary_bottom),
            secondary_progress: (row_count == 2).then_some(Rect::new(
                inset,
                secondary_progress_top,
                width - inset,
                secondary_bottom,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{windows::UsageRowView, UsageLevel};

    use super::{progress_fill_width, tooltip_text_needs_update, TaskbarLayout};

    #[test]
    fn full_layout_reserves_two_usage_rows() {
        let layout = TaskbarLayout::for_size(208, 48, 96);

        let secondary_label = layout
            .secondary_label
            .expect("full layout should include a secondary label");
        let secondary_percent = layout
            .secondary_percent
            .expect("full layout should include a secondary percent");
        let secondary_progress = layout
            .secondary_progress
            .expect("full layout should include a secondary progress bar");

        assert!(layout.progress.bottom <= secondary_progress.top);
        for rect in [
            layout
                .label
                .expect("full layout should include a primary label"),
            secondary_label,
            layout.percent,
            secondary_percent,
            layout.progress,
            secondary_progress,
        ] {
            assert!(rect.is_inside(layout.window));
        }
    }

    #[test]
    fn minimal_layout_keeps_only_the_primary_usage_row() {
        let layout = TaskbarLayout::for_size(99, 48, 96);

        assert!(layout.secondary_label.is_none());
        assert!(layout.secondary_percent.is_none());
        assert!(layout.secondary_progress.is_none());
    }

    #[test]
    fn single_row_layout_uses_the_available_body_height() {
        let layout = TaskbarLayout::for_rows(208, 48, 96, false);

        assert!(layout.secondary_progress.is_none());
        assert!(layout.progress.bottom > 30);
        assert!(layout.progress.is_inside(layout.window));
    }

    #[test]
    fn two_row_layout_stays_inside_the_minimum_taskbar_height_at_supported_dpis() {
        for dpi in [96, 120, 144, 192] {
            let width = crate::windows::widget::logical_to_physical(208, dpi);
            let height = crate::windows::widget::logical_to_physical(36, dpi);
            let layout = TaskbarLayout::for_size(width, height, dpi);
            let secondary_progress = layout
                .secondary_progress
                .expect("full layout should include a secondary progress bar");

            assert!(layout.progress.is_inside(layout.window));
            assert!(secondary_progress.is_inside(layout.window));
            assert!(layout.progress.bottom <= secondary_progress.top);
        }
    }

    #[test]
    fn taskbar_rows_prioritize_primary_and_promote_secondary_when_missing() {
        let primary = UsageRowView {
            label: "5h".to_owned(),
            used_percent: 10.0,
            display_percent: 10.0,
            percent_text: "10%".to_owned(),
            reset_text: "later".to_owned(),
            level: UsageLevel::Stable,
            forecast: super::super::ForecastView::Hidden,
        };
        let secondary = UsageRowView {
            label: "7d".to_owned(),
            used_percent: 20.0,
            display_percent: 20.0,
            percent_text: "20%".to_owned(),
            reset_text: "tomorrow".to_owned(),
            level: UsageLevel::Stable,
            forecast: super::super::ForecastView::Hidden,
        };

        let rows = super::taskbar_rows(Some(&primary), Some(&secondary));
        assert!(std::ptr::eq(rows[0].expect("primary row"), &primary));
        assert!(std::ptr::eq(rows[1].expect("secondary row"), &secondary));

        let promoted = super::taskbar_rows(None, Some(&secondary));
        assert!(std::ptr::eq(promoted[0].expect("promoted row"), &secondary));
        assert!(promoted[1].is_none());
    }

    #[test]
    fn progress_fill_width_follows_the_display_percent_and_clamps_it() {
        assert_eq!(progress_fill_width(100, 20.0), 20);
        assert_eq!(progress_fill_width(100, 80.0), 80);
        assert_eq!(progress_fill_width(100, -1.0), 0);
        assert_eq!(progress_fill_width(100, 125.0), 100);
    }

    #[test]
    fn unchanged_tooltip_text_does_not_need_a_native_update() {
        let text = "Codex usage\nStatus: Polling";
        let encoded: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();

        assert!(!tooltip_text_needs_update(&encoded, &encoded));
    }

    #[test]
    fn changed_tooltip_text_needs_a_native_update() {
        let previous: Vec<u16> = "Codex usage".encode_utf16().chain(Some(0)).collect();
        let next: Vec<u16> = "Codex usage\nStatus: Polling"
            .encode_utf16()
            .chain(Some(0))
            .collect();

        assert!(tooltip_text_needs_update(&previous, &next));
    }
}
