use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// 시스템 프로필을 포함해 관리할 수 있는 사용량 프로필의 최대 개수입니다.
pub const MAX_USAGE_PROFILES: usize = 8;

const MAX_PROFILE_LABEL_SCALARS: usize = 40;

/// Codex 사용량을 조회할 프로필의 안정적인 식별자입니다.
#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageProfileId {
    /// 현재 사용자 계정의 기본 Codex 환경을 사용하는 변경 불가 프로필입니다.
    System,
    /// 앱이 생성한 격리 Codex 환경의 숫자 식별자입니다.
    Managed(u32),
}

/// 사용자가 생성한 격리 Codex 환경의 표시 정보입니다.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ManagedUsageProfile {
    sequence: u32,
    label: String,
}

impl ManagedUsageProfile {
    /// 이 관리 프로필의 안정적인 숫자 식별자를 반환합니다.
    pub fn id(&self) -> UsageProfileId {
        UsageProfileId::Managed(self.sequence)
    }

    /// 사용자 화면에 표시할 검증된 프로필 이름을 반환합니다.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// 시스템 프로필과 관리 프로필의 목록 및 현재 선택 상태입니다.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UsageProfileCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_label: Option<String>,
    managed: Vec<ManagedUsageProfile>,
    selected: UsageProfileId,
    next_sequence: u32,
}

impl Default for UsageProfileCatalog {
    fn default() -> Self {
        Self {
            system_label: None,
            managed: Vec::new(),
            selected: UsageProfileId::System,
            next_sequence: 1,
        }
    }
}

impl UsageProfileCatalog {
    /// 카탈로그의 저장 값과 참조 무결성을 검증합니다.
    ///
    /// 유효하지 않은 데이터는 안전하게 거부하며, 호출자는 설정 복구 절차를 선택해야 합니다.
    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        if self.next_sequence == 0 {
            return Err(ProfileValidationError::InvalidId);
        }
        if self.managed.len() + 1 > MAX_USAGE_PROFILES {
            return Err(ProfileValidationError::TooManyProfiles);
        }

        if let Some(system_label) = &self.system_label {
            if normalize_profile_label(system_label)? != *system_label {
                return Err(ProfileValidationError::InvalidLabel);
            }
        }

        for (index, profile) in self.managed.iter().enumerate() {
            if profile.sequence == 0 || profile.sequence >= self.next_sequence {
                return Err(ProfileValidationError::InvalidId);
            }
            if normalize_profile_label(&profile.label)? != profile.label {
                return Err(ProfileValidationError::InvalidLabel);
            }
            if self.managed[..index].iter().any(|other| {
                other.label.to_lowercase() == profile.label.to_lowercase()
                    || other.sequence == profile.sequence
            }) || self.system_label.as_deref().is_some_and(|system_label| {
                system_label.to_lowercase() == profile.label.to_lowercase()
            }) {
                return Err(ProfileValidationError::DuplicateLabel);
            }
        }

        if !self.contains(self.selected) {
            return Err(ProfileValidationError::InvalidId);
        }

        Ok(())
    }

    /// 새 관리 프로필을 추가하고 생성된 프로필 정보를 반환합니다.
    ///
    /// 시스템 프로필을 포함한 최대 개수를 초과하면 프로필을 추가하지 않습니다.
    pub fn add(&mut self, label: &str) -> Result<ManagedUsageProfile, ProfileValidationError> {
        let label = self.validate_new_label(label, None)?;
        if self.managed.len() + 1 >= MAX_USAGE_PROFILES {
            return Err(ProfileValidationError::TooManyProfiles);
        }
        if self.next_sequence == 0 {
            return Err(ProfileValidationError::InvalidId);
        }

        let profile = ManagedUsageProfile {
            sequence: self.next_sequence,
            label,
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ProfileValidationError::InvalidId)?;
        self.managed.push(profile.clone());
        Ok(profile)
    }

    /// 지정한 사용량 프로필의 표시 이름을 변경합니다.
    ///
    /// 시스템 프로필의 이름은 선택 상태나 실행 환경을 바꾸지 않고 저장합니다. 대소문자를 구분하지
    /// 않는 중복 이름과 유효하지 않은 이름은 거부합니다.
    pub fn rename(
        &mut self,
        id: UsageProfileId,
        label: &str,
    ) -> Result<(), ProfileValidationError> {
        match id {
            UsageProfileId::System => {
                self.system_label = Some(self.validate_new_label(label, Some(id))?);
            }
            UsageProfileId::Managed(sequence) => {
                let index = self
                    .index_of(sequence)
                    .ok_or(ProfileValidationError::InvalidId)?;
                self.managed[index].label = self.validate_new_label(label, Some(id))?;
            }
        }
        Ok(())
    }

    /// 사용량을 표시할 프로필을 선택합니다.
    ///
    /// 존재하지 않거나 숫자 0인 관리 프로필은 선택할 수 없습니다.
    pub fn select(&mut self, id: UsageProfileId) -> Result<(), ProfileValidationError> {
        if !self.contains(id) {
            return Err(ProfileValidationError::InvalidId);
        }
        self.selected = id;
        Ok(())
    }

    /// 관리 프로필을 삭제합니다.
    ///
    /// 삭제 대상이 선택되어 있으면 선택 상태는 시스템 프로필로 안전하게 되돌아갑니다.
    pub fn remove(&mut self, id: UsageProfileId) -> Result<(), ProfileValidationError> {
        let UsageProfileId::Managed(sequence) = id else {
            return Err(ProfileValidationError::SystemProfileImmutable);
        };
        let index = self
            .index_of(sequence)
            .ok_or(ProfileValidationError::InvalidId)?;
        self.managed.remove(index);
        if self.selected == id {
            self.selected = UsageProfileId::System;
        }
        Ok(())
    }

    /// 저장된 관리 프로필을 생성 순서대로 반환합니다.
    pub fn managed(&self) -> &[ManagedUsageProfile] {
        &self.managed
    }

    /// 사용자가 지정한 시스템 프로필 표시 이름을 반환합니다.
    ///
    /// 이름이 없는 기존 설정과 기본 카탈로그에서는 `None`을 반환합니다. 이 경우 호출자는
    /// 지역화된 기본 이름을 표시해야 합니다.
    pub fn system_label(&self) -> Option<&str> {
        self.system_label.as_deref()
    }

    /// 현재 사용량 표시 대상으로 선택된 프로필을 반환합니다.
    pub fn selected(&self) -> UsageProfileId {
        self.selected
    }

    /// 프로필 식별자가 현재 카탈로그에서 유효한지 반환합니다.
    pub(crate) fn contains(&self, id: UsageProfileId) -> bool {
        match id {
            UsageProfileId::System => true,
            UsageProfileId::Managed(sequence) => self.index_of(sequence).is_some(),
        }
    }

    /// 다음 add가 사용할 수 있는 관리 프로필 식별자를 반환합니다.
    pub(crate) fn next_managed_id(&self) -> Option<UsageProfileId> {
        (self.next_sequence != 0).then_some(UsageProfileId::Managed(self.next_sequence))
    }

    fn index_of(&self, sequence: u32) -> Option<usize> {
        (sequence != 0).then_some(())?;
        self.managed
            .iter()
            .position(|profile| profile.sequence == sequence)
    }

    fn validate_new_label(
        &self,
        label: &str,
        current: Option<UsageProfileId>,
    ) -> Result<String, ProfileValidationError> {
        let normalized = normalize_profile_label(label)?;
        let normalized_key = normalized.to_lowercase();
        let conflicts_with_system = current != Some(UsageProfileId::System)
            && self
                .system_label
                .as_deref()
                .is_some_and(|system_label| system_label.to_lowercase() == normalized_key);
        let conflicts_with_managed = self.managed.iter().any(|profile| {
            current != Some(profile.id()) && profile.label.to_lowercase() == normalized_key
        });
        if conflicts_with_system || conflicts_with_managed {
            return Err(ProfileValidationError::DuplicateLabel);
        }
        Ok(normalized)
    }
}

/// 프로필 이름 또는 프로필 식별자가 도메인 규칙을 위반했을 때의 안전한 분류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileValidationError {
    /// 비어 있거나 경로에 사용할 수 없는 프로필 이름입니다.
    InvalidLabel,
    /// 다른 프로필과 대소문자를 구분하지 않고 같은 이름입니다.
    DuplicateLabel,
    /// 시스템 프로필을 포함한 최대 프로필 수를 초과했습니다.
    TooManyProfiles,
    /// 존재하지 않거나 안전하지 않은 관리 프로필 식별자입니다.
    InvalidId,
    /// 삭제할 수 없는 시스템 프로필을 삭제하려 했습니다.
    SystemProfileImmutable,
}

/// 사용자 입력 프로필 이름을 저장 가능한 표시 이름으로 검증하고 공백을 제거합니다.
///
/// 앞뒤 공백을 제거한 결과가 1~40개의 유니코드 스칼라 값이어야 하며, 제어 문자와 경로 구분자는
/// 허용하지 않습니다. `.`과 `..` 전체 일치는 거부하지만 표시 이름 안의 마침표는 허용합니다.
pub fn normalize_profile_label(label: &str) -> Result<String, ProfileValidationError> {
    let normalized = label.trim();
    if label.chars().any(char::is_control)
        || normalized.is_empty()
        || matches!(normalized, "." | "..")
        || normalized.chars().count() > MAX_PROFILE_LABEL_SCALARS
        || normalized
            .chars()
            .any(|character| matches!(character, '/' | '\\'))
    {
        return Err(ProfileValidationError::InvalidLabel);
    }
    Ok(normalized.to_owned())
}

/// 관리 프로필별 Codex 홈 디렉터리를 고정된 경로 구성 요소로 파생합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageProfileRoot {
    base: PathBuf,
}

/// 특정 사용량 프로필로 Codex 자식 프로세스를 실행하기 위한 격리된 컨텍스트입니다.
///
/// 관리 프로필의 실제 경로는 디버그 출력에 포함되지 않으며, 신뢰된 `UsageProfileRoot`에서만
/// 파생됩니다. 시스템 프로필은 현재 프로세스의 Codex 환경을 그대로 사용합니다.
#[derive(Clone, PartialEq, Eq)]
pub struct ProfileExecutionContext {
    id: UsageProfileId,
    codex_home: Option<PathBuf>,
    force_file_credentials: bool,
}

impl ProfileExecutionContext {
    /// 현재 사용자 계정의 기본 Codex 환경을 사용하는 시스템 컨텍스트를 생성합니다.
    ///
    /// 반환된 컨텍스트는 자식 프로세스에 환경 변수나 자격 증명 저장소 설정을 추가하지 않습니다.
    pub fn system() -> Self {
        Self {
            id: UsageProfileId::System,
            codex_home: None,
            force_file_credentials: false,
        }
    }

    /// 관리 프로필 전용 Codex 환경을 사용하는 실행 컨텍스트를 생성합니다.
    ///
    /// `root`와 `id`로부터 격리 경로를 내부에서 파생합니다. 시스템 프로필이나 유효하지 않은 관리
    /// 식별자는 거부하며, 호출자가 임의 경로를 주입할 수 없습니다.
    pub fn managed(
        root: &UsageProfileRoot,
        id: UsageProfileId,
    ) -> Result<Self, ProfileValidationError> {
        Ok(Self {
            id,
            codex_home: Some(root.codex_home(id)?),
            force_file_credentials: true,
        })
    }

    /// 이 컨텍스트가 나타내는 안정적인 프로필 식별자를 반환합니다.
    pub fn id(&self) -> UsageProfileId {
        self.id
    }

    pub(crate) fn codex_home(&self) -> Option<&Path> {
        self.codex_home.as_deref()
    }

    pub(crate) fn force_file_credentials(&self) -> bool {
        self.force_file_credentials
    }
}

impl fmt::Debug for ProfileExecutionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileExecutionContext")
            .field("id", &self.id)
            .field("managed", &self.codex_home.is_some())
            .finish()
    }
}

impl UsageProfileRoot {
    /// 애플리케이션 데이터 루트를 사용해 프로필 경로 파생기를 생성합니다.
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// 지정한 관리 프로필의 격리 Codex 홈 경로를 반환합니다.
    ///
    /// 시스템 프로필과 숫자 0은 외부 경로를 만들 수 없도록 거부하며, 사용자 입력은 경로에 직접
    /// 포함하지 않습니다.
    pub fn codex_home(&self, id: UsageProfileId) -> Result<PathBuf, ProfileValidationError> {
        Ok(self.managed_directory(id)?.join("codex-home"))
    }

    /// 관리 프로필 하나를 소유하는 앱 전용 디렉터리를 숫자 식별자에서 파생합니다.
    ///
    /// 파일 시스템 트랜잭션 내부에서만 사용하며 시스템 프로필과 숫자 0은 거부합니다. 반환 경로는
    /// 프로필 전체를 같은 볼륨에서 격리 이동하기 위한 경계이고 사용자 입력을 포함하지 않습니다.
    pub(crate) fn managed_directory(
        &self,
        id: UsageProfileId,
    ) -> Result<PathBuf, ProfileValidationError> {
        let UsageProfileId::Managed(sequence) = id else {
            return Err(ProfileValidationError::InvalidId);
        };
        if sequence == 0 {
            return Err(ProfileValidationError::InvalidId);
        }

        Ok(self
            .base
            .join("profiles")
            .join(format!("profile-{sequence:04}")))
    }

    /// 프로필 경로의 기준 애플리케이션 데이터 디렉터리를 반환합니다.
    pub fn base(&self) -> &Path {
        &self.base
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ProfileExecutionContext, UsageProfileId, UsageProfileRoot};

    #[test]
    fn execution_context_debug_redacts_managed_profile_path() {
        let root = UsageProfileRoot::new(PathBuf::from(r"C:\never-log-this"));
        let context = ProfileExecutionContext::managed(&root, UsageProfileId::Managed(2)).unwrap();

        let debug = format!("{context:?}");

        assert!(debug.contains("Managed(2)"));
        assert!(debug.contains("managed: true"));
        assert!(!debug.contains("never-log-this"));
        assert!(!debug.contains("codex-home"));
    }
}
