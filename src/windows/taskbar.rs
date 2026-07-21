//! 작업 표시줄 위젯 배치와 네이티브 연결 지원입니다.

use super::widget::{logical_to_physical, Rect};

const WS_CHILD_VALUE: u32 = 0x4000_0000;
const WS_POPUP_VALUE: u32 = 0x8000_0000;
const WS_CLIPSIBLINGS_VALUE: u32 = 0x0400_0000;

/// 작업 표시줄과 알림 영역의 화면 좌표입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarGeometry {
    /// 작업 표시줄 화면 좌표입니다.
    pub taskbar: Rect,
    /// 알림 영역 화면 좌표입니다.
    pub notification: Rect,
}

/// 작업 표시줄 배치를 안전하게 수행할 수 없는 이유입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarPlacementError {
    /// 세로 작업 표시줄은 지원하지 않습니다.
    VerticalTaskbar,
    /// 알림 영역 앞에 위젯을 배치할 공간이 없습니다.
    InsufficientSpace,
}

/// 알림 영역과 겹치지 않는 수평 작업 표시줄 자식 좌표를 계산합니다.
pub fn place_taskbar_widget(
    geometry: TaskbarGeometry,
    widget_size: (i32, i32),
    offset: i32,
) -> Result<Rect, TaskbarPlacementError> {
    if offset < 0 {
        return Err(TaskbarPlacementError::InsufficientSpace);
    }
    if geometry.taskbar.height() > geometry.taskbar.width() {
        return Err(TaskbarPlacementError::VerticalTaskbar);
    }
    let right = geometry.notification.left.saturating_sub(offset);
    let left = right.saturating_sub(widget_size.0);
    if widget_size.0 <= 0
        || widget_size.1 <= 0
        || left < geometry.taskbar.left
        || right > geometry.taskbar.right
        || widget_size.1 > geometry.taskbar.height()
    {
        return Err(TaskbarPlacementError::InsufficientSpace);
    }
    Ok(Rect::new(
        left,
        geometry.taskbar.top,
        right,
        geometry.taskbar.top + widget_size.1,
    ))
}

/// 작업 표시줄 높이가 축소되지 않은 위젯 높이를 수용하는지 확인하고 물리 크기를 반환합니다.
///
/// `taskbar_height`와 반환값은 물리 픽셀이며 `dpi`는 대상 작업 표시줄의 DPI입니다. 필요한 높이보다
/// 낮은 작업 표시줄은 축소 렌더링하지 않고 `InsufficientSpace`로 거부합니다.
pub fn taskbar_widget_size(
    taskbar_height: i32,
    dpi: u32,
) -> Result<(i32, i32), TaskbarPlacementError> {
    let size = (logical_to_physical(380, dpi), logical_to_physical(48, dpi));
    if taskbar_height < size.1 {
        Err(TaskbarPlacementError::InsufficientSpace)
    } else {
        Ok(size)
    }
}

/// 기존 최상위 창 스타일을 작업 표시줄 자식 창 스타일로 변환합니다.
///
/// `previous_style`에서 팝업 플래그를 제거하고 자식 창 및 형제 클리핑 플래그를 설정한 값을 반환합니다.
pub const fn taskbar_child_style(previous_style: u32) -> u32 {
    (previous_style & !WS_POPUP_VALUE) | WS_CHILD_VALUE | WS_CLIPSIBLINGS_VALUE
}

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::attach_to_taskbar;

#[cfg(windows)]
/// 지원 가능한 수평 작업 표시줄과 알림 영역이 있는지 확인합니다.
pub use platform::taskbar_available;

#[cfg(not(windows))]
/// Windows 이외의 플랫폼에서는 작업 표시줄을 사용할 수 없습니다.
pub fn taskbar_available() -> bool {
    false
}
