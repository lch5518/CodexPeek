//! 사용량 프로필 관리 대화상자의 플랫폼 독립 계약입니다.

use std::io;

use crate::{
    localized_text, normalize_profile_label, Language, LocalizationKey, ProfileValidationError,
    UsageProfileId,
};

use super::UsageProfileView;

#[cfg(windows)]
mod platform;

/// 프로필 관리 대화상자가 애플리케이션 계층에 전달하는 변경 요청입니다.
///
/// 문자열은 공용 프로필 이름 검증을 통과한 값만 포함하며, 실제 파일·설정·로그인 I/O는
/// 이 타입을 소비하는 백그라운드 애플리케이션 계층에서 수행해야 합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileDialogAction {
    /// 검증된 표시 이름으로 관리 프로필을 추가합니다.
    Add(String),
    /// 지정한 관리 프로필의 표시 이름을 변경합니다.
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
    /// 변경 작업이 진행 중이면 모든 명령을 거부하고, 시스템 프로필의 이름 변경·삭제도
    /// 항상 거부합니다.
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

    /// 선택된 관리 프로필의 이름 변경 입력을 검증해 타입 지정 작업으로 변환합니다.
    ///
    /// 시스템 프로필이거나 변경 중이면 `Ok(None)`을 반환하며, 실제 설정 변경은 수행하지
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

/// 프로필 종류와 로그인 상태에 따라 허용되는 관리 명령을 반환합니다.
///
/// 시스템 프로필에는 이름 변경과 삭제를 절대 제공하지 않습니다. 반환값 계산은 I/O나
/// 환경 변경을 수행하지 않습니다.
pub fn available_profile_actions(profile: &UsageProfileView) -> Vec<ProfileDialogCommand> {
    let mut actions = Vec::with_capacity(5);
    let mutable_managed_profile =
        profile.managed && matches!(profile.id, UsageProfileId::Managed(_));
    if mutable_managed_profile {
        actions.push(ProfileDialogCommand::Rename);
    }
    actions.push(ProfileDialogCommand::Login);
    if !profile.login_required {
        actions.push(ProfileDialogCommand::Logout);
    }
    if mutable_managed_profile {
        actions.push(ProfileDialogCommand::Delete);
    }
    actions
}

/// 대화상자 입력을 공용 프로필 이름 규칙으로 정규화하고 검증합니다.
///
/// `value`의 앞뒤 공백은 제거되며, 비어 있거나 제한을 넘거나 경로 문자를 포함하면
/// `ProfileValidationError`를 반환합니다.
pub fn validated_label(value: &str) -> Result<String, ProfileValidationError> {
    normalize_profile_label(value)
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
