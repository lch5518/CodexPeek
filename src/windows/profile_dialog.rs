//! 사용량 프로필 관리 대화상자의 플랫폼 독립 계약입니다.

use std::io;

use crate::{
    localized_text, normalize_profile_label, Language, LocalizationKey, ProfileValidationError,
    UsageProfileId,
};

use super::UsageProfileView;

#[cfg(windows)]
mod platform;

/// 작업 표시줄을 제외한 모니터 작업 영역의 화면 좌표 범위입니다.
///
/// `right`와 `bottom`은 Win32 `RECT`와 같이 배타적 경계이며, 음수 좌표 모니터도 그대로
/// 표현합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogWorkArea {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl DialogWorkArea {
    /// Win32 작업 영역 사각형에서 대화상자 배치 계산용 값을 만듭니다.
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

/// 창의 외곽 크기를 물리 픽셀 단위로 보관합니다.
///
/// 프레임과 캡션을 포함한 크기를 사용하므로 클라이언트 영역 크기로 대체하면 안 됩니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogWindowSize {
    width: i32,
    height: i32,
}

impl DialogWindowSize {
    /// 대화상자 외곽의 너비와 높이로 크기 값을 만듭니다.
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

/// 대화상자의 좌상단 화면 좌표입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogOrigin {
    x: i32,
    y: i32,
}

impl DialogOrigin {
    /// 화면 좌표를 대화상자의 새 좌상단 위치로 만듭니다.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 작업 영역 안에서 대화상자의 외곽을 축별로 가운데 배치합니다.
///
/// 계산은 `i64`로 수행해 다중 모니터의 음수 좌표에서도 오버플로를 피합니다. 창이 한 축에서
/// 작업 영역보다 크면 크기를 바꾸지 않고 해당 축의 작업 영역 시작점에 고정합니다.
pub fn centered_dialog_origin(
    work_area: DialogWorkArea,
    window_size: DialogWindowSize,
) -> DialogOrigin {
    DialogOrigin::new(
        centered_dialog_axis(work_area.left, work_area.right, window_size.width),
        centered_dialog_axis(work_area.top, work_area.bottom, window_size.height),
    )
}

/// 한 축에서 대화상자 외곽의 시작 좌표를 계산합니다.
///
/// 역전되거나 비어 있는 작업 영역과 음수 창 크기는 시작점으로 안전하게 처리합니다.
fn centered_dialog_axis(start: i32, end: i32, window_size: i32) -> i32 {
    let start = i64::from(start);
    let available = (i64::from(end) - start).max(0);
    let window_size = i64::from(window_size).max(0);
    if window_size >= available {
        return start as i32;
    }

    (start + (available - window_size) / 2) as i32
}

/// 대화상자의 기준 모니터를 고르는 네이티브 배치 정책입니다.
///
/// 소유자 창이 사라졌거나 크기가 없으면 `Cursor`를 사용해 보이지 않는 창 위치를 피합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogMonitorAnchor {
    /// 현재 커서가 있는 모니터의 작업 영역을 사용합니다.
    Cursor,
    /// 유효한 소유자 창이 있는 모니터의 작업 영역을 사용합니다.
    Owner(windows::Win32::Foundation::HWND),
}

/// 프로필 관리자 창의 기준 모니터를 현재 커서로 선택합니다.
///
/// 이 선택은 Win32 조회나 창 이동을 수행하지 않으며, 실제 모니터 조회는 플랫폼 계층이 처리합니다.
pub const fn profile_manager_dialog_monitor_anchor() -> DialogMonitorAnchor {
    DialogMonitorAnchor::Cursor
}

/// 프로필 추가 창의 기준 모니터를 소유자 상태에 따라 선택합니다.
///
/// `owner_size`가 없거나 한 축이라도 0 이하이면 소유자가 더 이상 배치 기준으로 안전하지 않으므로
/// 커서 모니터를 반환합니다. 호출자는 라이브 소유자의 외곽 크기만 전달해야 합니다.
pub fn add_profile_dialog_monitor_anchor(
    owner: windows::Win32::Foundation::HWND,
    owner_size: Option<DialogWindowSize>,
) -> DialogMonitorAnchor {
    let Some(owner_size) = owner_size else {
        return DialogMonitorAnchor::Cursor;
    };
    if owner == windows::Win32::Foundation::HWND::default()
        || owner_size.width <= 0
        || owner_size.height <= 0
    {
        DialogMonitorAnchor::Cursor
    } else {
        DialogMonitorAnchor::Owner(owner)
    }
}

/// 메시지 상자가 가운데 배치 프로필 대화상자 경계를 사용할지, 일반 애플리케이션 경계를 사용할지
/// 분류하는 경로입니다.
///
/// 각 경로는 기존 문구, 버튼 스타일, 반환값을 유지하면서 공통 가운데 배치 경계를 사용합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileMessageRoute {
    /// 프로필 관리자에서 안전한 작업 실패 오류를 표시합니다.
    ManagerSafeError,
    /// 프로필 추가 창에서 안전한 작업 실패 오류를 표시합니다.
    AddPromptSafeError,
    /// 관리자 또는 추가 창에서 이름 검증 경고를 표시합니다.
    ValidationWarning,
    /// 관리 프로필 삭제 확인을 표시합니다.
    DeleteConfirmation,
    /// 선택하거나 새로 추가한 프로필의 로그인 확인을 표시합니다.
    LoginConfirmation,
    /// 네이티브 UI 액션 처리 중 프로필 작업 실패 오류를 표시합니다.
    NativeOperationError,
}

/// 사용량 프로필 메시지 상자의 화면 배치 계약입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileMessagePlacement {
    /// 선택한 모니터의 작업 영역 가운데에 배치합니다.
    Centered,
}

/// 프로필 메시지 경로에 적용할 화면 배치 계약을 반환합니다.
///
/// `route`는 메시지의 호출 출처이며, 반환값은 플랫폼 코드가 실제 메시지 상자 경계를 선택할 때
/// 사용합니다. 이 함수는 Win32 호출이나 I/O를 수행하지 않습니다.
pub const fn profile_message_placement(route: ProfileMessageRoute) -> ProfileMessagePlacement {
    match route {
        ProfileMessageRoute::ManagerSafeError
        | ProfileMessageRoute::AddPromptSafeError
        | ProfileMessageRoute::ValidationWarning
        | ProfileMessageRoute::DeleteConfirmation
        | ProfileMessageRoute::LoginConfirmation
        | ProfileMessageRoute::NativeOperationError => ProfileMessagePlacement::Centered,
    }
}

/// 한 번의 `MessageBoxW` 활성화에서 사용할 복사 가능한 가운데 배치 요청입니다.
///
/// 메시지 루프 안으로 Rust 참조를 가져가지 않도록 호출 전에 해석한 작업 영역만 보관합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CenteredMessageBoxRequest {
    work_area: DialogWorkArea,
}

impl CenteredMessageBoxRequest {
    /// 조회가 끝난 모니터 작업 영역으로 한 번 소비할 요청을 만듭니다.
    const fn new(work_area: DialogWorkArea) -> Self {
        Self { work_area }
    }
}

/// 현재 스레드의 메시지 상자 가운데 배치 요청과 중첩 호출 복원 상태를 관리합니다.
///
/// 설치 시 이전 요청을 값으로 반환하고, 활성화 훅은 `consume`으로 현재 요청을 정확히 한 번
/// 가져갑니다. 호출 종료 시 반환받은 값을 `restore`해야 바깥쪽 중첩 호출의 상태가 보존됩니다.
#[derive(Debug, Default)]
struct CenteredMessageBoxRequestState {
    current: Option<CenteredMessageBoxRequest>,
}

impl CenteredMessageBoxRequestState {
    /// 새 요청을 설치하고 중첩 호출 종료 시 복원할 이전 요청을 반환합니다.
    fn install(&mut self, request: CenteredMessageBoxRequest) -> Option<CenteredMessageBoxRequest> {
        self.current.replace(request)
    }

    /// 현재 요청을 값으로 꺼내며, 같은 활성화 요청에서 다시 호출하면 `None`을 반환합니다.
    fn consume(&mut self) -> Option<CenteredMessageBoxRequest> {
        self.current.take()
    }

    /// 중첩 메시지 상자 호출 전에 저장한 요청 상태를 복원합니다.
    fn restore(&mut self, previous: Option<CenteredMessageBoxRequest>) {
        self.current = previous;
    }
}

/// 40개 유니코드 스칼라 프로필 이름을 손실 없이 보관하는 최대 UTF-16 코드 단위 수입니다.
///
/// 모든 스칼라가 보조 평면 문자여도 각각 서로게이트 쌍 두 단위를 사용하므로 80단위면 공용
/// `normalize_profile_label` 제한을 자르지 않고 수용합니다. 네이티브 edit 버퍼는 널 종료를 위해
/// 한 단위를 추가로 확보해야 합니다.
pub const PROFILE_LABEL_MAX_UTF16_UNITS: usize = 80;

/// 프로필 관리 대화상자가 애플리케이션 계층에 전달하는 변경 요청입니다.
///
/// 문자열은 공용 프로필 이름 검증을 통과한 값만 포함하며, 실제 파일·설정·로그인 I/O는
/// 이 타입을 소비하는 백그라운드 애플리케이션 계층에서 수행해야 합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileDialogAction {
    /// 검증된 표시 이름으로 관리 프로필을 추가합니다.
    Add(String),
    /// 지정한 사용량 프로필(시스템 또는 관리)의 표시 이름을 변경합니다.
    Rename(UsageProfileId, String),
    /// 지정한 프로필의 브라우저 로그인을 요청합니다.
    Login(UsageProfileId),
    /// 지정한 프로필의 로그아웃을 요청합니다.
    Logout(UsageProfileId),
    /// 지정한 관리 프로필의 삭제를 요청합니다.
    Delete(UsageProfileId),
}

/// 선택한 프로필에서 대화상자가 제공할 수 있는 명령입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileDialogCommand {
    /// 표시 이름을 변경합니다.
    Rename,
    /// 브라우저 로그인을 시작합니다.
    Login,
    /// 현재 프로필에서 로그아웃합니다.
    Logout,
    /// 관리 프로필을 삭제합니다.
    Delete,
}

/// 네이티브 프로필 관리자에 생성되는 변경 컨트롤의 역할입니다.
///
/// `AddBelowList`는 목록 바로 아래의 작은 추가 버튼이고, 나머지 항목은 하단 작업 행에
/// 배치됩니다. 이 계약은 플랫폼 구현과 결정적 테스트가 같은 컨트롤 구성을 사용하게 합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileManagerControl {
    /// 프로필 목록 바로 아래에서 추가 입력창을 엽니다.
    AddBelowList,
    /// 선택한 프로필의 표시 이름을 변경합니다.
    Rename,
    /// 선택한 프로필의 브라우저 로그인을 요청합니다.
    Login,
    /// 선택한 관리 프로필에서 로그아웃합니다.
    Logout,
    /// 선택한 관리 프로필을 삭제합니다.
    Delete,
}

/// 프로필 관리자가 생성하는 컨트롤 역할과 순서를 정의합니다.
///
/// 추가는 목록 아래에만 존재하며, 하단 행에는 이름 변경·로그인·로그아웃·삭제만 배치됩니다.
pub const PROFILE_MANAGER_CONTROLS: [ProfileManagerControl; 5] = [
    ProfileManagerControl::AddBelowList,
    ProfileManagerControl::Rename,
    ProfileManagerControl::Login,
    ProfileManagerControl::Logout,
    ProfileManagerControl::Delete,
];

/// 프로필 관리자 컨트롤의 화면 문구와 보조 설명입니다.
///
/// `visible_text`는 네이티브 컨트롤에 직접 표시하고, `accessible_description`은 화면 문구만으로
/// 목적을 알기 어려운 컨트롤의 지역화된 tooltip 또는 동등한 접근성 이름에 사용합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileManagerControlSpec {
    /// 컨트롤에 표시할 지역화된 화면 문구입니다.
    pub visible_text: &'static str,
    /// tooltip 또는 동등한 접근성 표면에 제공할 지역화된 설명입니다.
    pub accessible_description: Option<&'static str>,
}

/// 관리자 컨트롤 역할을 화면 문구와 접근성 설명으로 변환합니다.
///
/// `language`의 기존 지역화 표를 사용하며 I/O를 수행하지 않습니다. 목록 아래 추가 컨트롤은
/// 화면에는 `+`만 유지하고 `MenuAddUsageProfile` 문구를 보조 설명으로 제공합니다.
pub fn profile_manager_control_spec(
    control: ProfileManagerControl,
    language: Language,
) -> ProfileManagerControlSpec {
    let (visible_text, accessible_description) = match control {
        ProfileManagerControl::AddBelowList => (
            "+",
            Some(localized_text(
                LocalizationKey::MenuAddUsageProfile,
                language,
            )),
        ),
        ProfileManagerControl::Rename => (
            localized_text(LocalizationKey::UsageProfileRename, language),
            None,
        ),
        ProfileManagerControl::Login => (
            localized_text(LocalizationKey::UsageProfileLogin, language),
            None,
        ),
        ProfileManagerControl::Logout => (
            localized_text(LocalizationKey::UsageProfileLogout, language),
            None,
        ),
        ProfileManagerControl::Delete => (
            localized_text(LocalizationKey::UsageProfileDelete, language),
            None,
        ),
    };
    ProfileManagerControlSpec {
        visible_text,
        accessible_description,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileManagerDialogPhase {
    Live,
    AddPromptActive,
    Closed,
}

/// 프로필 관리자와 소유 추가 입력창 사이의 단일 중첩 전이를 관리합니다.
///
/// 관리자가 활성 상태일 때만 추가 입력창을 열 수 있고, 입력창이 열린 동안 두 번째 자식이나
/// 관리자 명령을 거부합니다. 취소는 관리자를 다시 활성 상태로 만들며, 제출은 정확히 한 개의
/// 검증된 Add 작업을 보관하고 관리자를 닫을 상태로 전환합니다.
#[derive(Clone, Debug)]
pub struct ProfileManagerDialogState {
    phase: ProfileManagerDialogPhase,
    result: Option<ProfileDialogAction>,
}

impl ProfileManagerDialogState {
    /// 작업 결과가 없는 활성 관리자 상태를 만듭니다.
    pub const fn new() -> Self {
        Self {
            phase: ProfileManagerDialogPhase::Live,
            result: None,
        }
    }

    /// 관리자가 현재 사용자 명령을 받을 수 있는지 반환합니다.
    pub const fn accepts_manager_commands(&self) -> bool {
        matches!(self.phase, ProfileManagerDialogPhase::Live)
    }

    /// 추가가 활성화되고 다른 자식이 없을 때만 추가 입력창 시작을 기록합니다.
    ///
    /// 시작에 성공하면 입력창 완료 전까지 관리자 명령과 추가 입력창 재진입을 거부합니다.
    pub fn begin_add_prompt(&mut self, add_enabled: bool) -> bool {
        if !add_enabled || !self.accepts_manager_commands() {
            return false;
        }
        self.phase = ProfileManagerDialogPhase::AddPromptActive;
        true
    }

    /// 열린 추가 입력창의 결과를 관리자 상태에 한 번 반영합니다.
    ///
    /// `None`은 작업 없이 활성 관리자로 복귀합니다. Add 작업은 관리자를 닫을 상태로 전환하고
    /// 한 번만 꺼낼 수 있게 보관합니다. 열린 자식이 없거나 Add 이외 작업이면 거부합니다.
    pub fn finish_add_prompt(&mut self, result: Option<ProfileDialogAction>) -> bool {
        if self.phase != ProfileManagerDialogPhase::AddPromptActive {
            return false;
        }
        match result {
            None => {
                self.phase = ProfileManagerDialogPhase::Live;
                true
            }
            Some(action @ ProfileDialogAction::Add(_)) => {
                self.result = Some(action);
                self.phase = ProfileManagerDialogPhase::Closed;
                true
            }
            Some(_) => {
                self.phase = ProfileManagerDialogPhase::Live;
                false
            }
        }
    }

    /// 활성 관리자를 지정 작업으로 닫고 결과를 한 번 보관합니다.
    ///
    /// 자식 입력창이 열렸거나 이미 닫힌 상태이면 작업을 거부합니다.
    pub(crate) fn close_with_action(&mut self, action: ProfileDialogAction) -> bool {
        if !self.accepts_manager_commands() {
            return false;
        }
        self.result = Some(action);
        self.phase = ProfileManagerDialogPhase::Closed;
        true
    }

    /// 관리자가 보관한 작업을 한 번 꺼내며, 작업이 없거나 이미 소비됐으면 `None`을 반환합니다.
    pub fn take_result(&mut self) -> Option<ProfileDialogAction> {
        self.result.take()
    }
}

impl Default for ProfileManagerDialogState {
    fn default() -> Self {
        Self::new()
    }
}

/// 사용량 프로필 추가 입력창에서 사용자가 선택한 순수 명령입니다.
///
/// `Submit`은 표시 이름 검증을 수행하지만, `Cancel`은 입력값과 무관하게 창을 닫는
/// 결과만 나타냅니다. 이 열거형은 UI 이벤트를 파일·설정·Codex I/O 없이 처리합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddProfilePromptCommand {
    /// 입력한 표시 이름으로 프로필 추가를 요청합니다.
    Submit,
    /// 변경 요청 없이 입력창을 닫습니다.
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddProfilePromptPhase {
    Live,
    Handling,
    Warning,
    Closed,
}

/// 추가 입력창의 명령 처리와 중첩 경고 수명을 직렬화하는 순수 상태입니다.
///
/// 네이티브 계층은 `Live`에서 시작한 명령 하나만 처리하고, 경고 메시지 상자의 중첩 메시지 루프
/// 동안 모든 제출·취소·닫기 재진입을 거부합니다. 성공 또는 취소로 닫힌 뒤에도 두 번째 결과를
/// 만들 수 없습니다.
#[derive(Clone, Debug)]
pub struct AddProfilePromptState {
    phase: AddProfilePromptPhase,
}

impl AddProfilePromptState {
    /// 새 명령을 받을 수 있는 추가 입력창 상태를 만듭니다.
    pub const fn new() -> Self {
        Self {
            phase: AddProfilePromptPhase::Live,
        }
    }

    /// 현재 추가 입력창이 제출·취소·닫기 명령을 받을 수 있는지 반환합니다.
    pub const fn accepts_commands(&self) -> bool {
        matches!(self.phase, AddProfilePromptPhase::Live)
    }

    /// 활성 입력창의 사용자 명령 처리를 한 번 시작합니다.
    ///
    /// 이미 명령·경고를 처리 중이거나 닫힌 상태이면 `false`를 반환해 재진입을 거부합니다.
    pub fn begin_command(&mut self) -> bool {
        if !self.accepts_commands() {
            return false;
        }
        self.phase = AddProfilePromptPhase::Handling;
        true
    }

    /// 처리 중인 명령이 경고 메시지 상자를 열기 직전임을 기록합니다.
    pub fn begin_warning(&mut self) -> bool {
        if self.phase != AddProfilePromptPhase::Handling {
            return false;
        }
        self.phase = AddProfilePromptPhase::Warning;
        true
    }

    /// 중첩 경고가 닫힌 뒤 입력창을 다시 명령 가능한 상태로 복구합니다.
    pub fn finish_warning(&mut self) -> bool {
        if self.phase != AddProfilePromptPhase::Warning {
            return false;
        }
        self.phase = AddProfilePromptPhase::Live;
        true
    }

    /// 처리 중인 성공 또는 취소 명령으로 입력창을 한 번만 닫을 상태로 전환합니다.
    pub fn finish_close(&mut self) -> bool {
        if self.phase != AddProfilePromptPhase::Handling {
            return false;
        }
        self.phase = AddProfilePromptPhase::Closed;
        true
    }
}

impl Default for AddProfilePromptState {
    fn default() -> Self {
        Self::new()
    }
}

/// `IsDialogMessageW`가 변환한 표준 모달 키보드 명령입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileDialogKeyboardCommand {
    /// Enter 키가 생성하는 표준 확인 명령입니다.
    Accept,
    /// Escape 키가 생성하는 표준 취소 명령입니다.
    Cancel,
}

/// 표준 모달 키보드 명령을 처리한 뒤의 대화상자 동작입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileDialogKeyboardResult {
    /// 기본 mutation을 추론하지 않고 현재 대화상자를 유지합니다.
    Ignore,
    /// 작업을 만들지 않고 대화상자를 닫습니다.
    CloseWithoutAction,
}

/// 표준 모달 키보드 명령을 안전한 대화상자 결과로 변환합니다.
///
/// Escape는 취소로 닫고, Enter는 명시적인 기본 mutation 버튼을 두지 않았으므로 무시합니다.
/// 이 함수는 프로필 작업을 생성하거나 I/O를 수행하지 않습니다.
pub const fn profile_dialog_keyboard_result(
    command: ProfileDialogKeyboardCommand,
) -> ProfileDialogKeyboardResult {
    match command {
        ProfileDialogKeyboardCommand::Accept => ProfileDialogKeyboardResult::Ignore,
        ProfileDialogKeyboardCommand::Cancel => ProfileDialogKeyboardResult::CloseWithoutAction,
    }
}

/// 모달 프로필 창 종료 시 Win32 계층이 수행해야 하는 정리 작업입니다.
///
/// 작업 순서는 사용자 데이터 포인터 제거, 살아 있는 창 파괴, 이 대화상자가 직접 비활성화한
/// 소유자 복원 순서입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalCleanupAction {
    /// 창의 `GWLP_USERDATA` 포인터를 먼저 제거합니다.
    ClearWindowState,
    /// 아직 살아 있는 모달 창을 파괴합니다.
    DestroyWindow,
    /// 이 모달이 비활성화했던 소유자 창을 다시 활성화합니다.
    RestoreOwner,
}

/// 모달 프로필 창과 소유자 활성 상태의 플랫폼 독립 수명 모델입니다.
///
/// Win32 호출 성공 여부와 무관하게 정리 순서를 결정하며, 이미 비활성화된 소유자는 이 모달이
/// 소유하지 않는 상태이므로 복원 대상으로 기록하지 않습니다.
#[derive(Clone, Debug)]
pub struct ModalDialogLifecycle {
    owner_present: bool,
    owner_was_enabled: bool,
    owner_disabled_by_dialog: bool,
    window_alive: bool,
}

impl ModalDialogLifecycle {
    /// 모달 시작 전 소유자 존재 여부와 활성 상태를 기록합니다.
    pub const fn new(owner_present: bool, owner_was_enabled: bool) -> Self {
        Self {
            owner_present,
            owner_was_enabled,
            owner_disabled_by_dialog: false,
            window_alive: false,
        }
    }

    /// 네이티브 창이 생성되어 사용자 데이터 포인터를 보유하기 시작했음을 기록합니다.
    pub fn window_created(&mut self) {
        self.window_alive = true;
    }

    /// 네이티브 창 파괴가 완료되어 포인터 제거·창 파괴가 더 필요하지 않음을 기록합니다.
    pub fn window_destroyed(&mut self) {
        self.window_alive = false;
    }

    /// 현재 모달이 소유자 창을 비활성화해야 하는지 반환합니다.
    pub const fn should_disable_owner(&self) -> bool {
        self.owner_present && self.owner_was_enabled && !self.owner_disabled_by_dialog
    }

    /// 이 모달이 활성 상태였던 소유자 창을 직접 비활성화했음을 기록합니다.
    pub fn owner_disabled(&mut self) {
        if self.should_disable_owner() {
            self.owner_disabled_by_dialog = true;
        }
    }

    /// 현재 수명 상태에서 필요한 정리 작업을 안전한 실행 순서로 반환합니다.
    pub fn cleanup_actions(&self) -> Vec<ModalCleanupAction> {
        let mut actions = Vec::with_capacity(3);
        if self.window_alive {
            actions.push(ModalCleanupAction::ClearWindowState);
            actions.push(ModalCleanupAction::DestroyWindow);
        }
        if self.owner_disabled_by_dialog {
            actions.push(ModalCleanupAction::RestoreOwner);
        }
        actions
    }
}

/// 프로필 관리 대화상자의 선택과 컨트롤 활성 상태를 결정하는 순수 모델입니다.
///
/// 프로필 표시 복사본만 소유하며 파일, 설정, Codex 프로세스에 접근하지 않습니다. Win32 계층과
/// 결정적 테스트가 같은 정책을 공유하도록 사용합니다.
#[derive(Clone, Debug)]
pub struct ProfileDialogController {
    profiles: Vec<UsageProfileView>,
    selected_index: Option<usize>,
    mutation_pending: bool,
}

impl ProfileDialogController {
    /// 표시 프로필과 현재 변경 진행 상태로 대화상자 모델을 만듭니다.
    ///
    /// `selected` 표시가 있는 첫 항목을 초기 선택하고, 없으면 첫 항목을 선택합니다. 빈 목록도
    /// 허용하며 이때 선택 기반 명령은 모두 비활성화됩니다.
    pub fn new(profiles: &[UsageProfileView], mutation_pending: bool) -> Self {
        let selected_index = profiles
            .iter()
            .position(|profile| profile.selected)
            .or_else(|| (!profiles.is_empty()).then_some(0));
        Self {
            profiles: profiles.to_vec(),
            selected_index,
            mutation_pending,
        }
    }

    /// 프로필 최대 개수와 변경 진행 상태를 반영해 추가 명령 활성 여부를 반환합니다.
    pub fn can_add(&self) -> bool {
        !self.mutation_pending && self.profiles.len() < crate::MAX_USAGE_PROFILES
    }

    /// 현재 선택된 프로필에서 지정 명령을 실행할 수 있는지 반환합니다.
    ///
    /// 변경 작업이 진행 중이면 모든 명령을 거부합니다. 시스템 프로필은 이름 변경과 로그인만
    /// 허용하고, 로그아웃·삭제는 항상 거부합니다.
    pub fn command_enabled(&self, command: ProfileDialogCommand) -> bool {
        if self.mutation_pending {
            return false;
        }
        self.selected_profile()
            .map(|profile| available_profile_actions(profile).contains(&command))
            .unwrap_or(false)
    }

    /// 목록 인덱스를 선택하고 유효한 항목인지 반환합니다.
    ///
    /// 범위를 벗어난 인덱스는 현재 선택을 변경하지 않습니다.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.profiles.len() {
            return false;
        }
        self.selected_index = Some(index);
        true
    }

    /// 현재 선택된 민감하지 않은 표시 프로필을 반환합니다.
    pub fn selected_profile(&self) -> Option<&UsageProfileView> {
        self.selected_index
            .and_then(|index| self.profiles.get(index))
    }

    /// 목록 인덱스에 해당하는 민감하지 않은 표시 프로필을 반환합니다.
    ///
    /// `index`가 범위를 벗어나면 `None`을 반환하며 선택 상태를 변경하거나 I/O를 수행하지
    /// 않습니다. 반환 참조에는 `UsageProfileView`의 표시용 필드만 포함됩니다.
    pub fn profile_at(&self, index: usize) -> Option<&UsageProfileView> {
        self.profiles.get(index)
    }

    /// 추가 입력을 검증해 타입 지정 작업으로 변환합니다.
    ///
    /// 최대 개수에 도달했거나 변경 중이면 `Ok(None)`을 반환합니다. 입력이 잘못되면 공용
    /// `ProfileValidationError`를 반환하며 어떤 I/O도 수행하지 않습니다.
    pub fn submit_add(
        &self,
        value: &str,
    ) -> Result<Option<ProfileDialogAction>, ProfileValidationError> {
        if !self.can_add() {
            return Ok(None);
        }
        validated_label(value).map(|label| Some(ProfileDialogAction::Add(label)))
    }

    /// 선택된 프로필의 이름 변경 입력을 검증해 타입 지정 작업으로 변환합니다.
    ///
    /// 변경 중이거나 선택 항목이 없으면 `Ok(None)`을 반환하며, 실제 설정 변경은 수행하지
    /// 않습니다.
    pub fn submit_rename(
        &self,
        value: &str,
    ) -> Result<Option<ProfileDialogAction>, ProfileValidationError> {
        if !self.command_enabled(ProfileDialogCommand::Rename) {
            return Ok(None);
        }
        let Some(profile) = self.selected_profile() else {
            return Ok(None);
        };
        validated_label(value).map(|label| Some(ProfileDialogAction::Rename(profile.id, label)))
    }

    /// 선택 명령이 허용되고 사용자가 필요한 확인을 완료했을 때만 타입 지정 작업을 반환합니다.
    ///
    /// `confirmed`가 거짓이면 취소로 처리합니다. 이름 변경은 별도 입력 검증이 필요하므로 이
    /// 메서드에서 생성하지 않습니다.
    pub fn confirmed_command(
        &self,
        command: ProfileDialogCommand,
        confirmed: bool,
    ) -> Option<ProfileDialogAction> {
        if !confirmed || !self.command_enabled(command) {
            return None;
        }
        let id = self.selected_profile()?.id;
        match command {
            ProfileDialogCommand::Login => Some(ProfileDialogAction::Login(id)),
            ProfileDialogCommand::Logout => Some(ProfileDialogAction::Logout(id)),
            ProfileDialogCommand::Delete => Some(ProfileDialogAction::Delete(id)),
            ProfileDialogCommand::Rename => None,
        }
    }
}

/// 관리자 컨트롤의 활성 상태를 선택·변경 진행·프로필 제한 정책으로 계산합니다.
///
/// 네이티브 계층은 모든 관리자 버튼에 이 매핑을 적용하므로 추가 버튼은 `can_add()`가 참일
/// 때만 활성화되고, 선택 기반 작업은 공용 명령 가용성 규칙을 그대로 따릅니다.
pub fn profile_manager_control_enabled(
    controller: &ProfileDialogController,
    control: ProfileManagerControl,
) -> bool {
    match control {
        ProfileManagerControl::AddBelowList => controller.can_add(),
        ProfileManagerControl::Rename => controller.command_enabled(ProfileDialogCommand::Rename),
        ProfileManagerControl::Login => controller.command_enabled(ProfileDialogCommand::Login),
        ProfileManagerControl::Logout => controller.command_enabled(ProfileDialogCommand::Logout),
        ProfileManagerControl::Delete => controller.command_enabled(ProfileDialogCommand::Delete),
    }
}

/// 프로필 종류와 로그인 상태에 따라 허용되는 관리 명령을 반환합니다.
///
/// 시스템 프로필에는 이름 변경과 로그인을 제공하고, 로그아웃·삭제는 관리 프로필에만 제공합니다.
/// 반환값 계산은 I/O나 환경 변경을 수행하지 않습니다.
pub fn available_profile_actions(profile: &UsageProfileView) -> Vec<ProfileDialogCommand> {
    let mut actions = Vec::with_capacity(5);
    let mutable_managed_profile =
        profile.managed && matches!(profile.id, UsageProfileId::Managed(_));
    actions.push(ProfileDialogCommand::Rename);
    actions.push(ProfileDialogCommand::Login);
    if mutable_managed_profile && !profile.login_required {
        actions.push(ProfileDialogCommand::Logout);
    }
    if mutable_managed_profile {
        actions.push(ProfileDialogCommand::Delete);
    }
    actions
}

/// 프로필 관리자 목록에 표시할 프로필 이름과 시스템 기본 계정 표식을 만듭니다.
///
/// 시스템 프로필에만 현재 언어의 기본 계정 표식을 덧붙이며, 사용자 지정 이름이 기본 이름과 같으면
/// 중복 표기를 피합니다. 입력 프로필을 변경하거나 I/O를 수행하지 않습니다.
pub fn profile_manager_row_label(profile: &UsageProfileView, language: Language) -> String {
    if profile.id != UsageProfileId::System {
        return profile.label.clone();
    }
    let marker = localized_text(LocalizationKey::UsageProfileSystem, language);
    if profile.label == marker {
        profile.label.clone()
    } else {
        format!("{} ({marker})", profile.label)
    }
}

/// 프로필 관리자 owner-draw 행에 전달할 안전한 표시 문자열 모음입니다.
///
/// 이름과 기존 지역화 summary는 그대로 복사하며, 시스템 계정과 현재 표시 프로필의 역할은
/// 별도 표식으로 반환합니다. 계정·인증·경로 정보에 접근하거나 입력을 변경하지 않습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileManagerRowText {
    /// 사용자가 지정했거나 지역화된 프로필 표시 이름입니다.
    pub name: String,
    /// 호출자가 이미 안전하게 구성한 사용량 또는 로그인 상태 요약입니다.
    pub summary: String,
    /// 시스템 계정과 현재 표시 프로필을 색상 외 텍스트로 구분하는 지역화 표식입니다.
    pub markers: Vec<&'static str>,
}

/// 프로필 관리자 owner-draw 행의 이름, summary, 역할 표식을 만듭니다.
///
/// `profile`은 민감하지 않은 표시 모델이어야 합니다. 시스템 프로필의 이름이 이미 현재 언어의
/// 시스템 이름과 같으면 중복 표식을 생략하고, 현재 표시 프로필에는 별도의 텍스트 표식을
/// 추가합니다. 반환값은 소유 문자열과 정적 지역화 표식만 포함하며 I/O를 수행하지 않습니다.
pub fn profile_manager_row_text(
    profile: &UsageProfileView,
    language: Language,
) -> ProfileManagerRowText {
    let system_marker = localized_text(LocalizationKey::UsageProfileSystem, language);
    let mut markers = Vec::with_capacity(2);
    if profile.id == UsageProfileId::System && profile.label != system_marker {
        markers.push(system_marker);
    }
    if profile.selected {
        markers.push(localized_text(
            LocalizationKey::UsageProfileDisplayed,
            language,
        ));
    }
    ProfileManagerRowText {
        name: profile.label.clone(),
        summary: profile.summary.clone(),
        markers,
    }
}

/// 네이티브 listbox와 보조 기술에 전달할 프로필 행의 안전한 단일 문자열을 만듭니다.
///
/// `profile`의 표시 이름, 지역화된 시스템·현재 표식, 기존 안전 summary만 포함합니다. 표식은
/// 괄호 안에서 쉼표로 구분하고 summary는 em dash 뒤에 보존합니다. 빈 표식이나 빈 summary는
/// 생략하며 인증·계정·경로 데이터에 접근하거나 I/O를 수행하지 않습니다.
pub fn profile_manager_accessible_row_text(
    profile: &UsageProfileView,
    language: Language,
) -> String {
    let copy = profile_manager_row_text(profile, language);
    let mut text = copy.name;
    if !copy.markers.is_empty() {
        text.push_str(" (");
        text.push_str(&copy.markers.join(", "));
        text.push(')');
    }
    if !copy.summary.trim().is_empty() {
        text.push_str(" — ");
        text.push_str(&copy.summary);
    }
    text
}

/// 대화상자 입력을 공용 프로필 이름 규칙으로 정규화하고 검증합니다.
///
/// `value`의 앞뒤 공백은 제거되며, 비어 있거나 제한을 넘거나 경로 문자를 포함하면
/// `ProfileValidationError`를 반환합니다.
pub fn validated_label(value: &str) -> Result<String, ProfileValidationError> {
    normalize_profile_label(value)
}

/// 프로필 추가 입력창 명령을 검증된 프로필 작업으로 변환합니다.
///
/// `value`는 `Submit`일 때 공유 표시 이름 규칙으로 정규화·검증됩니다. 잘못된 값은
/// `ProfileValidationError`로 반환되어 UI가 입력창을 닫지 않고 오류를 표시할 수 있습니다.
/// `Cancel`은 입력값을 검사하지 않고 `Ok(None)`을 반환하며, 이 함수는 I/O를 수행하지 않습니다.
pub fn add_profile_prompt_result(
    value: &str,
    command: AddProfilePromptCommand,
) -> Result<Option<ProfileDialogAction>, ProfileValidationError> {
    match command {
        AddProfilePromptCommand::Submit => {
            validated_label(value).map(|label| Some(ProfileDialogAction::Add(label)))
        }
        AddProfilePromptCommand::Cancel => Ok(None),
    }
}

/// 선택한 표시 프로필의 브라우저 로그인 확인 문구를 만듭니다.
///
/// `label`은 화면 표시용으로만 삽입하며 로그·경로·환경에는 사용하지 않습니다. 반환 문구는
/// 브라우저 계정 확인과 기존 Codex CLI·IDE 로그인 비변경 범위를 함께 설명합니다.
pub fn profile_login_confirmation(label: &str, language: Language) -> String {
    format!(
        "{}\n\n{}: {label}\n{}",
        localized_text(LocalizationKey::UsageProfileConfirmBrowserAccount, language),
        localized_text(LocalizationKey::UsageProfileDisplayed, language),
        localized_text(LocalizationKey::UsageProfileCliIdeUnchanged, language),
    )
}

/// 지정한 표시 프로필의 로컬 데이터 삭제 확인 문구를 만듭니다.
///
/// `label`은 사용자 확인에만 표시하며, 반환 문구는 삭제 데이터가 복구되지 않는다는 제약을
/// 명시합니다.
pub fn profile_delete_confirmation(label: &str, language: Language) -> String {
    format!(
        "{}\n\n{}: {label}\n{}",
        localized_text(LocalizationKey::UsageProfileDeleteConfirm, language),
        localized_text(LocalizationKey::UsageProfileDisplayed, language),
        localized_text(LocalizationKey::UsageProfileDeleteIrrecoverable, language),
    )
}

/// 프로필 목록을 소유 모달 대화상자로 표시하고 한 개의 타입 지정 작업을 반환합니다.
///
/// `profiles`는 민감 정보가 없는 표시 복사본이며 `mutation_pending`이 참이면 모든 변경
/// 컨트롤을 비활성화합니다. 비 Windows 플랫폼에서는 부작용 없이 `Unsupported`를 반환합니다.
pub fn show_profile_manager(
    profiles: &[UsageProfileView],
    mutation_pending: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>> {
    #[cfg(windows)]
    {
        platform::show_profile_manager(profiles, mutation_pending, language)
    }
    #[cfg(not(windows))]
    {
        let _ = (profiles, mutation_pending, language);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows profile dialog is unavailable",
        ))
    }
}

/// 숨은 메시지 루프 창을 소유자로 사용해 프로필 관리 대화상자를 표시합니다.
///
/// `owner`는 대화상자가 열린 동안만 비활성화되며 종료 시 복원됩니다. 반환된 작업은 검증된
/// UI 의도일 뿐이며 이 함수는 Codex·파일·설정 I/O를 수행하지 않습니다.
#[cfg(windows)]
pub(crate) unsafe fn show_profile_manager_owned(
    owner: windows::Win32::Foundation::HWND,
    profiles: &[UsageProfileView],
    mutation_pending: bool,
    language: Language,
) -> io::Result<Option<ProfileDialogAction>> {
    platform::show_profile_manager_owned(owner, profiles, mutation_pending, language)
}

/// 숨은 메시지 루프 창을 소유자로 사용해 선택 프로필의 브라우저 로그인을 한 번 확인합니다.
///
/// `label`은 확인 창에만 표시하며, 문구는 Codex CLI와 IDE 로그인이 바뀌지 않음을 함께 안내합니다.
/// 실제 로그인·브라우저 작업은 수행하지 않습니다.
/// 사용량 프로필 흐름의 메시지를 경로별 배치 계약에 따라 표시합니다.
///
/// `route`는 호출 출처를 구분하며, `owner`, 문구, 제목, 버튼 스타일은 변경하지 않고 플랫폼의
/// 공통 메시지 상자 경계로 전달합니다. 반환값은 원래 `MessageBoxW` 결과를 보존합니다.
///
/// # Safety
///
/// `owner`가 0이 아니면 호출 동안 유효한 Win32 창 핸들이어야 합니다. 이 함수는
/// `MessageBoxW`의 중첩 메시지 루프를 동기적으로 실행합니다.
#[cfg(windows)]
pub(crate) unsafe fn show_profile_message(
    route: ProfileMessageRoute,
    owner: windows::Win32::Foundation::HWND,
    message: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> io::Result<windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT> {
    match profile_message_placement(route) {
        ProfileMessagePlacement::Centered => {
            show_centered_profile_message(owner, message, title, style)
        }
    }
}

/// 사용량 프로필 메시지 상자를 선택한 작업 영역 가운데에 표시합니다.
///
/// 유효하고 보이는 `owner`가 있으면 그 창의 모니터를 사용하고, 그렇지 않으면 커서 모니터를
/// 사용합니다. 모니터 조회, 훅 설치, 창 이동 실패는 Windows 기본 배치로 안전하게 대체됩니다.
///
/// # Safety
///
/// `owner`가 0이 아니면 호출 동안 Win32가 처리할 수 있는 창 핸들이어야 합니다. 이 함수는
/// `MessageBoxW`의 중첩 메시지 루프를 동기적으로 실행합니다.
#[cfg(windows)]
pub(crate) unsafe fn show_centered_profile_message(
    owner: windows::Win32::Foundation::HWND,
    message: &str,
    title: &str,
    style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> io::Result<windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT> {
    platform::show_centered_profile_message(owner, message, title, style)
}

#[cfg(windows)]
pub(crate) unsafe fn confirm_profile_login_owned(
    owner: windows::Win32::Foundation::HWND,
    label: &str,
    language: Language,
) -> io::Result<bool> {
    platform::confirm_profile_login_owned(owner, label, language)
}

/// 선택된 프로필의 브라우저 로그인을 시작할지 확인합니다.
///
/// 확인 창은 Codex CLI와 IDE 로그인이 바뀌지 않음을 안내합니다. 비 Windows 플랫폼에서는
/// 부작용 없이 `Unsupported`를 반환합니다.
pub fn confirm_profile_login(language: Language) -> io::Result<bool> {
    #[cfg(windows)]
    {
        platform::confirm_profile_login(language)
    }
    #[cfg(not(windows))]
    {
        let _ = language;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows profile confirmation is unavailable",
        ))
    }
}

/// 지정한 표시 이름의 로컬 프로필 데이터를 삭제할지 확인합니다.
///
/// `label`은 화면 문구에만 사용되며 경로·클래스명·명령 ID로 변환하지 않습니다. 비 Windows
/// 플랫폼에서는 부작용 없이 `Unsupported`를 반환합니다.
pub fn confirm_profile_delete(label: &str, language: Language) -> io::Result<bool> {
    #[cfg(windows)]
    {
        platform::confirm_profile_delete(label, language)
    }
    #[cfg(not(windows))]
    {
        let _ = (label, language);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows profile confirmation is unavailable",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_dialog_centers_a_standard_manager_in_the_work_area() {
        let origin = centered_dialog_origin(
            DialogWorkArea::new(0, 0, 1920, 1040),
            DialogWindowSize::new(650, 410),
        );

        assert_eq!(origin, DialogOrigin::new(635, 315));
    }

    #[test]
    fn centered_dialog_preserves_negative_secondary_monitor_coordinates() {
        let origin = centered_dialog_origin(
            DialogWorkArea::new(-1920, -40, 0, 1000),
            DialogWindowSize::new(513, 401),
        );

        assert_eq!(origin, DialogOrigin::new(-1217, 279));
    }

    #[test]
    fn centered_dialog_rounds_odd_dimensions_down_from_the_work_area_start() {
        let origin = centered_dialog_origin(
            DialogWorkArea::new(10, 15, 1931, 1056),
            DialogWindowSize::new(651, 411),
        );

        assert_eq!(origin, DialogOrigin::new(645, 330));
    }

    #[test]
    fn centered_dialog_anchors_an_oversized_window_at_the_work_area_start() {
        let origin = centered_dialog_origin(
            DialogWorkArea::new(100, 50, 2020, 1090),
            DialogWindowSize::new(2000, 1200),
        );

        assert_eq!(origin, DialogOrigin::new(100, 50));
    }
}
