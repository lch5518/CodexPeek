//! 부동 위젯의 DPI 독립 레이아웃 계산입니다.

/// 정수 픽셀 사각형입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// 왼쪽 좌표입니다.
    pub left: i32,
    /// 위쪽 좌표입니다.
    pub top: i32,
    /// 오른쪽 좌표입니다.
    pub right: i32,
    /// 아래쪽 좌표입니다.
    pub bottom: i32,
}

impl Rect {
    /// 네 모서리 좌표로 사각형을 만듭니다.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 사각형 너비를 반환합니다.
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    /// 사각형 높이를 반환합니다.
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    /// 다른 사각형 안에 완전히 포함되는지 반환합니다.
    pub const fn is_inside(self, other: Self) -> bool {
        self.left >= other.left
            && self.top >= other.top
            && self.right <= other.right
            && self.bottom <= other.bottom
    }

    /// 다른 사각형과 면적이 겹치는지 반환합니다.
    pub const fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

/// 380x112 논리 픽셀 부동 위젯의 물리 픽셀 레이아웃입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetLayout {
    /// 전체 창 영역입니다.
    pub window: Rect,
    /// 주 사용량 막대 영역입니다.
    pub primary_bar: Rect,
    /// 보조 사용량 막대 영역입니다.
    pub secondary_bar: Rect,
    /// 상태 문자열 영역입니다.
    pub status: Rect,
}

impl WidgetLayout {
    /// 지정한 DPI에 맞춰 모든 논리 좌표를 일관되게 반올림합니다.
    pub fn for_dpi(dpi: u32) -> Self {
        let scale = |value: i32| -> i32 {
            let dpi = i64::from(dpi.max(1));
            ((i64::from(value) * dpi + 48) / 96) as i32
        };
        let rect = |left, top, right, bottom| {
            Rect::new(scale(left), scale(top), scale(right), scale(bottom))
        };
        Self {
            window: rect(0, 0, 380, 112),
            primary_bar: rect(16, 34, 364, 46),
            secondary_bar: rect(16, 66, 364, 78),
            status: rect(16, 88, 364, 104),
        }
    }
}

/// 96-DPI 논리 좌표를 지정한 DPI의 물리 좌표로 반올림합니다.
pub fn logical_to_physical(value: i32, dpi: u32) -> i32 {
    scale_round(value, dpi.max(1), 96)
}

/// 물리 좌표를 96-DPI 논리 좌표로 반올림합니다.
pub fn physical_to_logical(value: i32, dpi: u32) -> i32 {
    scale_round(value, 96, dpi.max(1))
}

fn scale_round(value: i32, numerator: u32, denominator: u32) -> i32 {
    let product = i64::from(value) * i64::from(numerator);
    let adjustment = i64::from(denominator) / 2;
    let rounded = if product < 0 {
        product - adjustment
    } else {
        product + adjustment
    };
    (rounded / i64::from(denominator)) as i32
}

/// 창의 왼쪽 위 좌표를 작업 영역 안으로 제한합니다.
pub fn clamp_floating_position(
    position: (i32, i32),
    window_size: (i32, i32),
    work_area: Rect,
) -> (i32, i32) {
    let max_x = (work_area.right - window_size.0).max(work_area.left);
    let max_y = (work_area.bottom - window_size.1).max(work_area.top);
    (
        position.0.clamp(work_area.left, max_x),
        position.1.clamp(work_area.top, max_y),
    )
}
