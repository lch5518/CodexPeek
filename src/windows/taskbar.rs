//! 작업 표시줄 위젯 배치와 네이티브 연결 지원입니다.

use super::widget::{logical_to_physical, Rect};
use std::fmt;

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

/// 작업 표시줄 연결 트랜잭션에서 실패한 단계입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarAttachmentStage {
    /// 기존 창 스타일을 읽는 단계입니다.
    ReadOriginalStyle,
    /// 기존 부모 창을 읽는 단계입니다.
    ReadOriginalParent,
    /// 자식 창 스타일을 적용하는 단계입니다.
    ApplyChildStyle,
    /// 적용된 자식 창 스타일을 다시 확인하는 단계입니다.
    VerifyChildStyle,
    /// 작업 표시줄 부모를 설정하는 단계입니다.
    SetParent,
    /// 설정된 부모 창을 다시 확인하는 단계입니다.
    VerifyParent,
    /// 작업 표시줄 안의 위치와 프레임을 적용하는 단계입니다.
    SetPosition,
}

/// 작업 표시줄 연결 트랜잭션이 사용하는 최소 창 조작 인터페이스입니다.
///
/// 실제 Windows 구현과 상태를 기록하는 테스트 구현이 동일한 순서 및 롤백 로직을 실행하도록 합니다.
pub trait TaskbarAttachmentBackend {
    /// 부모 창을 식별하는 복사 가능한 값입니다.
    type Parent: Copy + Eq;
    /// 창 조작 실패의 원인을 설명하는 오류입니다.
    type Error: fmt::Display;

    /// 현재 창 스타일을 읽습니다.
    fn read_style(&mut self) -> Result<u32, Self::Error>;
    /// 현재 부모 창을 읽으며 최상위 창이면 `None`을 반환합니다.
    fn read_parent(&mut self) -> Result<Option<Self::Parent>, Self::Error>;
    /// 창 스타일을 설정합니다.
    fn set_style(&mut self, style: u32) -> Result<(), Self::Error>;
    /// 부모 창을 설정하며 `None`은 최상위 창으로 되돌립니다.
    fn set_parent(&mut self, parent: Option<Self::Parent>) -> Result<(), Self::Error>;
    /// 계산된 작업 표시줄 위치와 프레임 변경을 적용합니다.
    fn set_position(&mut self) -> Result<(), Self::Error>;
    /// 롤백한 스타일의 비클라이언트 프레임을 다시 계산합니다.
    fn refresh_frame(&mut self) -> Result<(), Self::Error>;
}

/// 작업 표시줄 연결 실패와 롤백 실패 여부를 함께 보존하는 오류입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskbarAttachmentError {
    failed_stage: TaskbarAttachmentStage,
    operation_error: String,
    rollback_error: Option<String>,
}

impl TaskbarAttachmentError {
    /// 최초로 실패한 연결 단계를 반환합니다.
    pub const fn failed_stage(&self) -> TaskbarAttachmentStage {
        self.failed_stage
    }

    /// 원래 부모와 스타일을 복구하는 과정에서도 오류가 발생했는지 반환합니다.
    pub const fn rollback_failed(&self) -> bool {
        self.rollback_error.is_some()
    }
}

impl fmt::Display for TaskbarAttachmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "taskbar attachment {:?} failed: {}",
            self.failed_stage, self.operation_error
        )?;
        if let Some(rollback_error) = &self.rollback_error {
            write!(formatter, "; rollback failed: {rollback_error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for TaskbarAttachmentError {}

/// 작업 표시줄 자식 스타일, 부모, 위치를 순서대로 적용하고 실패 시 원래 상태로 되돌립니다.
///
/// `backend`는 실제 또는 테스트 창 조작기이며 `target_parent`는 연결할 작업 표시줄입니다. 성공하면 모든
/// 단계가 읽기 검증을 통과한 것이며, 실패하면 최초 실패 단계와 롤백 오류를 함께 반환합니다.
pub fn run_taskbar_attachment<B: TaskbarAttachmentBackend>(
    backend: &mut B,
    target_parent: B::Parent,
) -> Result<(), TaskbarAttachmentError> {
    let original_style = backend
        .read_style()
        .map_err(|error| TaskbarAttachmentError {
            failed_stage: TaskbarAttachmentStage::ReadOriginalStyle,
            operation_error: error.to_string(),
            rollback_error: None,
        })?;
    let original_parent = backend
        .read_parent()
        .map_err(|error| TaskbarAttachmentError {
            failed_stage: TaskbarAttachmentStage::ReadOriginalParent,
            operation_error: error.to_string(),
            rollback_error: None,
        })?;
    let child_style = taskbar_child_style(original_style);

    if let Err(error) = backend.set_style(child_style) {
        return Err(attachment_failure(
            backend,
            TaskbarAttachmentStage::ApplyChildStyle,
            error.to_string(),
            original_parent,
            original_style,
        ));
    }
    match backend.read_style() {
        Ok(style) if style == child_style => {}
        Ok(style) => {
            return Err(attachment_failure(
                backend,
                TaskbarAttachmentStage::VerifyChildStyle,
                format!("style mismatch: expected {child_style:#x}, got {style:#x}"),
                original_parent,
                original_style,
            ));
        }
        Err(error) => {
            return Err(attachment_failure(
                backend,
                TaskbarAttachmentStage::VerifyChildStyle,
                error.to_string(),
                original_parent,
                original_style,
            ));
        }
    }
    if let Err(error) = backend.set_parent(Some(target_parent)) {
        return Err(attachment_failure(
            backend,
            TaskbarAttachmentStage::SetParent,
            error.to_string(),
            original_parent,
            original_style,
        ));
    }
    match backend.read_parent() {
        Ok(parent) if parent == Some(target_parent) => {}
        Ok(_) => {
            return Err(attachment_failure(
                backend,
                TaskbarAttachmentStage::VerifyParent,
                "parent verification mismatch".to_owned(),
                original_parent,
                original_style,
            ));
        }
        Err(error) => {
            return Err(attachment_failure(
                backend,
                TaskbarAttachmentStage::VerifyParent,
                error.to_string(),
                original_parent,
                original_style,
            ));
        }
    }
    if let Err(error) = backend.set_position() {
        return Err(attachment_failure(
            backend,
            TaskbarAttachmentStage::SetPosition,
            error.to_string(),
            original_parent,
            original_style,
        ));
    }
    Ok(())
}

fn attachment_failure<B: TaskbarAttachmentBackend>(
    backend: &mut B,
    failed_stage: TaskbarAttachmentStage,
    operation_error: String,
    original_parent: Option<B::Parent>,
    original_style: u32,
) -> TaskbarAttachmentError {
    TaskbarAttachmentError {
        failed_stage,
        operation_error,
        rollback_error: rollback_attachment(backend, original_parent, original_style),
    }
}

fn rollback_attachment<B: TaskbarAttachmentBackend>(
    backend: &mut B,
    original_parent: Option<B::Parent>,
    original_style: u32,
) -> Option<String> {
    let mut errors = Vec::new();
    if let Err(error) = backend.set_parent(original_parent) {
        errors.push(error.to_string());
    }
    match backend.read_parent() {
        Ok(parent) if parent == original_parent => {}
        Ok(_) => errors.push("parent rollback verification mismatch".to_owned()),
        Err(error) => errors.push(error.to_string()),
    }
    if let Err(error) = backend.set_style(original_style) {
        errors.push(error.to_string());
    }
    match backend.read_style() {
        Ok(style) if style == original_style => {}
        Ok(_) => errors.push("style rollback verification mismatch".to_owned()),
        Err(error) => errors.push(error.to_string()),
    }
    if let Err(error) = backend.refresh_frame() {
        errors.push(error.to_string());
    }
    (!errors.is_empty()).then(|| errors.join(", "))
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
