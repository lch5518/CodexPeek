//! 작업표시줄 위젯의 DPI 독립 레이아웃 계산입니다.

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

/// 96-DPI 논리 좌표를 지정한 DPI의 물리 좌표로 반올림합니다.
pub fn logical_to_physical(value: i32, dpi: u32) -> i32 {
    scale_round(value, dpi.max(1), 96)
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
