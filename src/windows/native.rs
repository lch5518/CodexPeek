//! Win32 메시지 루프 진입점입니다.

use std::io;

use super::{profile_dialog::ProfileDialogAction, UiAction, UiBackend, UiSettings};

#[cfg(windows)]
mod platform;

/// 프로세스가 살아 있는 동안 이름 있는 뮤텍스를 보유하는 단일 인스턴스 가드입니다.
pub struct SingleInstanceGuard {
    _inner: platform::SingleInstanceGuard,
}

/// 설정 또는 작업자를 시작하기 전에 단일 인스턴스 소유권을 획득합니다.
pub fn acquire_single_instance() -> io::Result<SingleInstanceGuard> {
    #[cfg(windows)]
    {
        platform::acquire_single_instance().map(|inner| SingleInstanceGuard { _inner: inner })
    }
    #[cfg(not(windows))]
    {
        Ok(SingleInstanceGuard(platform::SingleInstanceGuard))
    }
}

/// 네이티브 단일 인스턴스 UI 메시지 루프를 실행합니다.
pub fn run(backend: &mut dyn UiBackend) -> io::Result<()> {
    #[cfg(windows)]
    {
        platform::run(backend)
    }
    #[cfg(not(windows))]
    {
        let _ = backend;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows UI is unavailable",
        ))
    }
}

/// 프로필 대화상자의 검증된 결과를 애플리케이션 계층의 타입 지정 UI 의도로 변환합니다.
///
/// 입력에 포함된 프로필 식별자와 정규화된 표시 이름을 그대로 보존하며 I/O나 프로세스 변경은
/// 수행하지 않습니다. 실제 작업은 `UiBackend`가 UI 스레드 밖에서 예약해야 합니다.
pub fn profile_dialog_ui_action(action: ProfileDialogAction) -> UiAction {
    match action {
        ProfileDialogAction::Add(label) => UiAction::AddUsageProfile(label),
        ProfileDialogAction::Rename(id, label) => UiAction::RenameUsageProfile(id, label),
        ProfileDialogAction::Login(id) => UiAction::LoginUsageProfile(id),
        ProfileDialogAction::Logout(id) => UiAction::LogoutUsageProfile(id),
        ProfileDialogAction::Delete(id) => UiAction::DeleteUsageProfile(id),
    }
}

/// 로그인 확인 결과를 일반 또는 확인 완료 backend 호출로 구분한 안전한 UI 전달 방식입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileLoginDispatch {
    /// 브라우저 로그인을 예약하지 않는 일반 UI 동작입니다.
    Normal(UiAction),
    /// 사용자 확인 뒤에만 호출할 로그인 허용 UI 동작입니다.
    Confirmed(UiAction),
}

/// 한 번의 브라우저 로그인 확인에 필요한 선택 프로필 표시 정보와 안정적인 UI 동작입니다.
///
/// 표시 이름은 확인 창에만 사용하고 로그·경로·환경에 전달하지 않습니다. 추가 취소는 프로필 생성
/// 동작을 유지하지만 기존 프로필 로그인 취소는 아무 작업도 만들지 않습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileLoginConfirmationRequest {
    label: String,
    confirmed_action: UiAction,
    cancelled_action: Option<UiAction>,
}

impl ProfileLoginConfirmationRequest {
    /// 확인 창에서 식별할 선택 프로필 표시 이름을 반환합니다.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// 사용자 확인 결과를 backend 전달 방식으로 변환합니다.
    ///
    /// 확인하면 `Confirmed`, 추가 취소면 로그인 없는 `Normal`, 기존 프로필 로그인 취소면 `None`을
    /// 반환합니다. 이 함수는 I/O나 브라우저 실행을 수행하지 않습니다.
    pub fn resolve(self, confirmed: bool) -> Option<ProfileLoginDispatch> {
        if confirmed {
            Some(ProfileLoginDispatch::Confirmed(self.confirmed_action))
        } else {
            self.cancelled_action.map(ProfileLoginDispatch::Normal)
        }
    }
}

/// 로그인을 예약할 수 있는 UI 동작을 선택 프로필 표시 이름이 포함된 확인 요청으로 변환합니다.
///
/// 계정 추가, 상위 로그인, 관리자 재로그인을 한 경계로 모읍니다. 상위 로그인은 확인 시점의 선택
/// 프로필 ID로 고정하고, 찾을 수 없는 ID나 로그인과 무관한 동작은 `None`을 반환합니다.
pub fn profile_login_confirmation_request(
    action: &UiAction,
    settings: &UiSettings,
) -> Option<ProfileLoginConfirmationRequest> {
    match action {
        UiAction::AddUsageProfile(label) => Some(ProfileLoginConfirmationRequest {
            label: label.clone(),
            confirmed_action: action.clone(),
            cancelled_action: Some(action.clone()),
        }),
        UiAction::Login => {
            let profile = settings
                .usage_profiles
                .iter()
                .find(|profile| profile.selected)?;
            Some(ProfileLoginConfirmationRequest {
                label: profile.label.clone(),
                confirmed_action: UiAction::LoginUsageProfile(profile.id),
                cancelled_action: None,
            })
        }
        UiAction::LoginUsageProfile(id) => {
            let profile = settings
                .usage_profiles
                .iter()
                .find(|profile| profile.id == *id)?;
            Some(ProfileLoginConfirmationRequest {
                label: profile.label.clone(),
                confirmed_action: action.clone(),
                cancelled_action: None,
            })
        }
        _ => None,
    }
}

/// 진단 모드에서 부모 프로세스의 콘솔에 연결합니다.
pub fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        platform::attach_parent_console();
    }
}

/// 검증된 ChatGPT 로그인 페이지를 기본 브라우저로 엽니다.
///
/// app-server가 제공한 OpenAI HTTPS 인증 URL만 허용하며, 그 외 URL은 브라우저로 전달하지 않습니다.
pub(crate) fn open_validated_login_page(url: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        unsafe { platform::open_validated_login_page(url) }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
}

/// Windows 사용자 UI 언어 식별자와 로캘 이름을 반환합니다.
pub fn user_ui_language() -> (Option<u16>, Option<String>) {
    #[cfg(windows)]
    unsafe {
        platform::user_ui_language()
    }
    #[cfg(not(windows))]
    {
        (None, None)
    }
}

/// 진단 결과를 민감 정보가 없는 모달 Windows 대화 상자로 표시합니다.
pub fn show_diagnostic_summary(title: &str, message: &str) -> io::Result<()> {
    #[cfg(windows)]
    unsafe {
        platform::show_diagnostic_summary(title, message)
    }
    #[cfg(not(windows))]
    {
        let _ = (title, message);
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
}
