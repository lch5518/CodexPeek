//! 작업 표시줄 전용 주간 사용량 표현과 DPI 레이아웃입니다.

use super::{widget::logical_to_physical, UsageRowView};
use crate::windows::widget::Rect;

/// 작업 표시줄에 여유 공간이 있을 때 사용하는 위젯의 기본 논리 너비입니다.
pub const TASKBAR_WIDTH_LOGICAL: i32 = 208;

/// 작업 표시줄 아이콘과 겹치지 않으면서 내용을 유지할 수 있는 최소 논리 너비입니다.
pub const TASKBAR_MIN_WIDTH_LOGICAL: i32 = 88;

/// hover 밝기를 약 150ms 동안 현재 값에서 목표 값으로 이동시킵니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoverTransition {
    value: u8,
    target: u8,
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

/// 보조 사용량 창을 주간 값으로 우선 선택하고 없으면 유일한 기본 창을 반환합니다.
pub fn select_weekly_row<'a>(
    primary: Option<&'a UsageRowView>,
    secondary: Option<&'a UsageRowView>,
) -> Option<&'a UsageRowView> {
    secondary.or(primary)
}

/// 진행 막대 너비와 표시 비율을 사용해 실제 채움 너비를 계산합니다.
///
/// 표시 비율은 0~100%로 제한되며, 잘못된 음수나 초과 값이 레이아웃 밖으로 그려지지 않게 합니다.
pub(crate) fn progress_fill_width(width: i32, display_percent: f64) -> i32 {
    (f64::from(width) * display_percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

/// 작업 표시줄 글라스 위젯의 고정 영역입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarLayout {
    /// 전체 클라이언트 영역입니다.
    pub window: Rect,
    /// 상태 점 영역입니다.
    pub dot: Rect,
    /// 주간 사용량 레이블 영역입니다.
    pub label: Rect,
    /// 오른쪽 고정 퍼센트 영역입니다.
    pub percent: Rect,
    /// 진행 막대 영역입니다.
    pub progress: Rect,
}

impl TaskbarLayout {
    /// 실제 클라이언트 크기와 DPI에 맞춰 고정 영역을 계산합니다.
    pub fn for_size(width: i32, height: i32, dpi: u32) -> Self {
        let scale = |value| logical_to_physical(value, dpi);
        let inset = scale(11).min((width / 4).max(1));
        let dot_size = scale(6).min((height / 3).max(1));
        let top = scale(9).min((height - dot_size - 4).max(1));
        let progress_height = scale(3).min((height / 4).max(1));
        let progress_bottom = (height - scale(8)).max(top + dot_size + progress_height);
        let progress_top = (progress_bottom - progress_height).max(top + dot_size + 2);
        let percent_width = scale(42).min((width / 3).max(1));
        let percent_left = (width - inset - percent_width).max(inset + dot_size + scale(8));
        let label_left = inset + dot_size + scale(8);

        Self {
            window: Rect::new(0, 0, width, height),
            dot: Rect::new(inset, top, inset + dot_size, top + dot_size),
            label: Rect::new(
                label_left,
                scale(5),
                percent_left - scale(4),
                progress_top - 2,
            ),
            percent: Rect::new(percent_left, scale(5), width - inset, progress_top - 2),
            progress: Rect::new(inset, progress_top, width - inset, progress_bottom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::progress_fill_width;

    #[test]
    fn progress_fill_width_follows_the_display_percent_and_clamps_it() {
        assert_eq!(progress_fill_width(100, 20.0), 20);
        assert_eq!(progress_fill_width(100, 80.0), 80);
        assert_eq!(progress_fill_width(100, -1.0), 0);
        assert_eq!(progress_fill_width(100, 125.0), 100);
    }
}
