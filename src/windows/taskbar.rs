//! 작업 표시줄 위젯 배치와 네이티브 연결 지원입니다.

use super::{
    taskbar_widget::{TASKBAR_MIN_WIDTH_LOGICAL, TASKBAR_WIDTH_LOGICAL},
    widget::{logical_to_physical, Rect},
};
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
    /// 위젯이 침범하면 안 되는 작업 표시줄 버튼 영역입니다.
    pub occupied: Option<Rect>,
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
    minimum_width: i32,
    offset: i32,
) -> Result<Rect, TaskbarPlacementError> {
    if offset < 0 {
        return Err(TaskbarPlacementError::InsufficientSpace);
    }
    if geometry.taskbar.height() > geometry.taskbar.width() {
        return Err(TaskbarPlacementError::VerticalTaskbar);
    }
    let right = geometry.notification.left.saturating_sub(offset);
    let preferred_left = right.saturating_sub(widget_size.0);
    let occupied_right = geometry
        .occupied
        .map(|occupied| occupied.right)
        .unwrap_or(geometry.taskbar.left)
        .max(geometry.taskbar.left);
    let left = preferred_left.max(occupied_right);
    if widget_size.0 <= 0
        || widget_size.1 <= 0
        || minimum_width <= 0
        || minimum_width > widget_size.0
        || right.saturating_sub(left) < minimum_width
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

/// 대상 DPI에서 위젯이 축소될 수 있는 최소 물리 너비를 반환합니다.
pub fn taskbar_widget_minimum_width(dpi: u32) -> i32 {
    logical_to_physical(TASKBAR_MIN_WIDTH_LOGICAL, dpi)
}

/// 작업 표시줄 높이에 맞춘 축약 위젯의 물리 크기를 반환합니다.
///
/// `taskbar_height`와 반환값은 물리 픽셀이며 `dpi`는 대상 작업 표시줄의 DPI입니다. 48 논리 픽셀을
/// 넘는 높이는 사용하지 않고, 2행 축약 렌더러가 읽기 어렵게 되는 36 논리 픽셀 미만만 거부합니다.
pub fn taskbar_widget_size(
    taskbar_height: i32,
    dpi: u32,
) -> Result<(i32, i32), TaskbarPlacementError> {
    let minimum_height = logical_to_physical(36, dpi);
    if taskbar_height < minimum_height {
        Err(TaskbarPlacementError::InsufficientSpace)
    } else {
        Ok((
            logical_to_physical(TASKBAR_WIDTH_LOGICAL, dpi),
            taskbar_height.min(logical_to_physical(48, dpi)),
        ))
    }
}

/// 기존 최상위 창 스타일을 작업 표시줄 자식 창 스타일로 변환합니다.
///
/// `previous_style`에서 팝업 플래그를 제거하고 자식 창 및 형제 클리핑 플래그를 설정한 값을 반환합니다.
const fn taskbar_child_style(previous_style: u32) -> u32 {
    (previous_style & !WS_POPUP_VALUE) | WS_CHILD_VALUE | WS_CLIPSIBLINGS_VALUE
}

/// 작업 표시줄 연결 트랜잭션에서 실패한 단계입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
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
#[doc(hidden)]
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
#[doc(hidden)]
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
#[doc(hidden)]
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
    let operation = (|| -> Result<(), (TaskbarAttachmentStage, String)> {
        backend
            .set_style(child_style)
            .map_err(|error| stage_error(TaskbarAttachmentStage::ApplyChildStyle, error))?;
        let style = backend
            .read_style()
            .map_err(|error| stage_error(TaskbarAttachmentStage::VerifyChildStyle, error))?;
        if style != child_style {
            return Err((
                TaskbarAttachmentStage::VerifyChildStyle,
                format!("style mismatch: expected {child_style:#x}, got {style:#x}"),
            ));
        }
        backend
            .set_parent(Some(target_parent))
            .map_err(|error| stage_error(TaskbarAttachmentStage::SetParent, error))?;
        let parent = backend
            .read_parent()
            .map_err(|error| stage_error(TaskbarAttachmentStage::VerifyParent, error))?;
        if parent != Some(target_parent) {
            return Err((
                TaskbarAttachmentStage::VerifyParent,
                "parent verification mismatch".to_owned(),
            ));
        }
        backend
            .set_position()
            .map_err(|error| stage_error(TaskbarAttachmentStage::SetPosition, error))
    })();
    operation.map_err(|(failed_stage, operation_error)| {
        attachment_failure(
            backend,
            failed_stage,
            operation_error,
            original_parent,
            original_style,
        )
    })
}

fn stage_error(
    stage: TaskbarAttachmentStage,
    error: impl fmt::Display,
) -> (TaskbarAttachmentStage, String) {
    (stage, error.to_string())
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

/// 위젯 창이 작업표시줄에 연결되었는지 또는 독립 플로팅 상태인지 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum WidgetSurface<T> {
    /// 지정한 작업표시줄 대상에 연결된 상태입니다.
    Attached(T),
    /// 작업표시줄과 분리된 최상위 플로팅 창 상태입니다.
    Detached,
}

/// 작업표시줄 연결 실패에도 위젯 창을 유지하는 조정자가 사용하는 창 연산 경계입니다.
///
/// 구현은 `create_detached`로 만든 창을 `surfaces`에 즉시 반영하고, attach/detach 성공 뒤
/// 해당 표면 상태를 갱신해야 합니다. 모든 메서드는 UI 스레드에서 짧은 창 연산만 수행해야
/// 하며 파일 또는 네트워크 I/O를 수행하면 안 됩니다.
#[doc(hidden)]
pub trait WidgetSurfaceBackend {
    /// 조정 중 식별할 창 핸들 형식입니다.
    type Window: Copy + Eq;
    /// 작업표시줄 대상 식별자 형식입니다.
    type Target: Copy + Eq;
    /// 창 생성, 연결, 배치 또는 정리 실패 형식입니다.
    type Error;

    /// 현재 살아 있는 창과 실제 표면 상태의 복사본을 반환합니다.
    fn surfaces(&self) -> Vec<(Self::Window, WidgetSurface<Self::Target>)>;
    /// 안전한 최상위 위치에 플로팅 위젯 하나를 만들고 핸들을 반환합니다.
    fn create_detached(&mut self) -> Result<Self::Window, Self::Error>;
    /// 기존 플로팅 창을 지정한 작업표시줄에 연결합니다.
    fn attach(&mut self, window: Self::Window, target: Self::Target) -> Result<(), Self::Error>;
    /// 기존 창을 최상위 플로팅 상태와 안전한 위치로 복구합니다.
    fn detach(&mut self, window: Self::Window) -> Result<(), Self::Error>;
    /// 더 이상 필요하지 않은 창 하나를 파괴합니다.
    fn destroy(&mut self, window: Self::Window) -> Result<(), Self::Error>;
}

/// 현재 작업표시줄 대상과 위젯 창을 조정하고 복구 가능한 attach 오류를 반환합니다.
///
/// 대상이 없거나 하나 이상의 attach가 실패하면 정확히 한 플로팅 창을 유지합니다. 이후
/// 대상이 나타나면 기존 attached 창과 fallback 창을 먼저 재사용하므로 중복 창을 만들지
/// 않습니다. 반환 벡터에는 플로팅 fallback으로 복구한 attach 오류만 담기며
/// 생성·분리·파괴 실패는 즉시 `Err`로 반환합니다.
#[doc(hidden)]
pub fn reconcile_widget_surfaces<B>(
    backend: &mut B,
    targets: &[B::Target],
) -> Result<Vec<B::Error>, B::Error>
where
    B: WidgetSurfaceBackend,
{
    let current = backend.surfaces();
    if (!targets.is_empty() && surfaces_match_targets(&current, targets))
        || (targets.is_empty()
            && current.len() == 1
            && matches!(current[0].1, WidgetSurface::Detached))
    {
        return Ok(Vec::new());
    }

    if targets.is_empty() {
        let keep = current
            .iter()
            .find(|(_, surface)| matches!(surface, WidgetSurface::Detached))
            .or_else(|| current.first())
            .copied();
        let (window, surface) = match keep {
            Some(existing) => existing,
            None => {
                let window = backend.create_detached()?;
                (window, WidgetSurface::Detached)
            }
        };
        if !matches!(surface, WidgetSurface::Detached) {
            backend.detach(window)?;
        }
        for (candidate, _) in current {
            if candidate != window {
                backend.destroy(candidate)?;
            }
        }
        return Ok(Vec::new());
    }

    let mut attached_targets = Vec::new();
    let mut detached = None;
    for (window, surface) in current {
        match surface {
            WidgetSurface::Attached(target)
                if targets.contains(&target) && !attached_targets.contains(&target) =>
            {
                attached_targets.push(target);
            }
            WidgetSurface::Detached if detached.is_none() => detached = Some(window),
            _ => backend.destroy(window)?,
        }
    }
    if attached_targets.len() == targets.len() {
        if let Some(window) = detached {
            backend.destroy(window)?;
        }
        return Ok(Vec::new());
    }

    let mut errors = Vec::new();
    let mut fallback_locked = false;
    for target in targets.iter().copied() {
        if attached_targets.contains(&target) {
            continue;
        }
        let window = if !fallback_locked {
            detached
                .take()
                .map(Ok)
                .unwrap_or_else(|| backend.create_detached())?
        } else {
            backend.create_detached()?
        };
        match backend.attach(window, target) {
            Ok(()) => attached_targets.push(target),
            Err(error) => {
                errors.push(error);
                if detached.is_none() {
                    detached = Some(window);
                    fallback_locked = true;
                } else {
                    backend.destroy(window)?;
                }
                if attached_targets.is_empty() {
                    break;
                }
            }
        }
    }
    Ok(errors)
}

fn surfaces_match_targets<W: Copy + Eq, T: Copy + Eq>(
    surfaces: &[(W, WidgetSurface<T>)],
    targets: &[T],
) -> bool {
    surfaces.len() == targets.len()
        && surfaces
            .iter()
            .zip(targets)
            .all(|((_, surface), target)| *surface == WidgetSurface::Attached(*target))
}

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub(crate) use platform::{
    attach_to_taskbar, reposition_taskbar_widget, TaskbarObserver, TaskbarTarget,
    TASKBAR_LAYOUT_CHANGED,
};

#[cfg(windows)]
/// 지원 가능한 수평 작업 표시줄과 알림 영역이 있는지 확인합니다.
pub use platform::taskbar_available;

#[cfg(not(windows))]
/// Windows 이외의 플랫폼에서는 작업 표시줄을 사용할 수 없습니다.
pub fn taskbar_available() -> bool {
    false
}
