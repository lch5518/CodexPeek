//! 작업 표시줄 위젯 배치와 네이티브 연결 지원입니다.

use super::widget::Rect;

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
