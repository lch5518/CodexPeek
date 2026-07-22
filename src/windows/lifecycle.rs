//! 네이티브 창 수명과 복구 결정을 순수 상태로 모델링합니다.

/// Explorer 또는 타이머가 복구 검사를 요청한 원인입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryEvent {
    /// Explorer가 작업 표시줄을 다시 만들었습니다.
    TaskbarCreated,
    /// 주기적인 상태 검사입니다.
    Timer,
}

/// 현재 위젯 상태에 필요한 복구 동작입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryDecision {
    /// 현재 위젯을 그대로 둡니다.
    Keep,
    /// 기존 위젯에 현재 표시 정책을 다시 적용합니다.
    Reapply,
    /// 새 위젯을 만들고 표시 정책을 적용합니다.
    RecreateAndApply,
    /// 현재 숨김 정책에서는 위젯이 필요하지 않습니다.
    NoWidgetNeeded,
}

/// 종료 시 수행할 네이티브 리소스 정리 동작입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupAction {
    /// 창 타이머를 중지합니다.
    StopTimer,
    /// 트레이 아이콘을 제거합니다.
    RemoveTray,
    /// 위젯 창을 파괴합니다.
    DestroyWidget,
    /// 숨은 소유자 창을 파괴합니다.
    DestroyOwner,
}

/// 네이티브 리소스 소유와 최근 작업 표시줄 연결 상태입니다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeLifecycle {
    owner_exists: bool,
    widget_exists: bool,
    timer_active: bool,
    tray_exists: bool,
    widget_was_taskbar_owned: bool,
}

impl NativeLifecycle {
    /// 숨은 소유자 창 생성을 기록합니다.
    pub fn owner_created(&mut self) {
        self.owner_exists = true;
    }

    /// 위젯 창 생성을 기록합니다.
    pub fn widget_created(&mut self) {
        self.widget_exists = true;
        self.widget_was_taskbar_owned = false;
    }

    /// 타이머 시작을 기록합니다.
    pub fn timer_started(&mut self) {
        self.timer_active = true;
    }

    /// 트레이 아이콘 생성을 기록합니다.
    pub fn tray_created(&mut self) {
        self.tray_exists = true;
    }

    /// 위젯이 작업 표시줄 자식이 되었음을 기록합니다.
    pub fn widget_attached_to_taskbar(&mut self) {
        self.widget_was_taskbar_owned = true;
    }

    /// `WM_NCDESTROY`로 위젯 핸들이 무효화되었음을 기록합니다.
    pub fn widget_destroyed(&mut self) {
        self.widget_exists = false;
    }

    /// 현재 상태와 복구 이벤트에 맞는 동작을 반환합니다.
    pub fn recovery_decision(
        &self,
        event: RecoveryEvent,
        widget_visible: bool,
    ) -> RecoveryDecision {
        if !self.widget_exists {
            return if widget_visible || self.widget_was_taskbar_owned {
                RecoveryDecision::RecreateAndApply
            } else {
                RecoveryDecision::NoWidgetNeeded
            };
        }
        if matches!(event, RecoveryEvent::TaskbarCreated) {
            RecoveryDecision::Reapply
        } else {
            RecoveryDecision::Keep
        }
    }

    /// 현재 소유 리소스를 안전한 종료 순서로 반환합니다.
    pub fn cleanup_actions(&self) -> Vec<CleanupAction> {
        let mut actions = Vec::with_capacity(4);
        if self.timer_active {
            actions.push(CleanupAction::StopTimer);
        }
        if self.tray_exists {
            actions.push(CleanupAction::RemoveTray);
        }
        if self.widget_exists {
            actions.push(CleanupAction::DestroyWidget);
        }
        if self.owner_exists {
            actions.push(CleanupAction::DestroyOwner);
        }
        actions
    }
}
