use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use crate::{Settings, SettingsStore, UsageProfileCatalog, UsageProfileId, UsageProfileRoot};

static TOMBSTONE_NONCE: AtomicU64 = AtomicU64::new(0);

/// 관리 프로필 디렉터리의 생성·격리 삭제·복구를 수행하는 파일 시스템 경계입니다.
///
/// 모든 입력 경로는 신뢰된 `UsageProfileRoot`와 숫자 프로필 ID에서 파생되어야 합니다. 구현은
/// 인증 파일 내용을 읽지 않으며, 삭제 대상이 검증된 관리 디렉터리인지 확인해야 합니다.
pub trait ProfileFileSystem: Send + Sync {
    /// 관리 프로필의 빈 Codex 홈을 생성합니다.
    ///
    /// `root`와 0이 아닌 관리 `id`에서 정확한 경로를 파생합니다. 성공하면 새 빈 디렉터리가
    /// 존재하며, 기존 대상·시스템 프로필·reparse point는 오류로 반환합니다.
    fn create_managed_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()>;

    /// 설정 저장에 실패한 새 프로필의 빈 홈만 제거합니다.
    ///
    /// `root`와 `id`로 대상을 다시 파생하며 비어 있지 않거나 안전 경계를 벗어난 디렉터리는
    /// 보존하고 오류를 반환합니다.
    fn remove_empty_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()>;

    /// 관리 프로필 디렉터리를 같은 볼륨의 검증 가능한 숨김 tombstone으로 이동합니다.
    ///
    /// `root`와 `id`에서 원본을 파생하며 성공 시 내부 생성한 tombstone 경로를 반환합니다.
    fn stage_delete(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<PathBuf>;

    /// 설정 저장 실패 시 tombstone을 원래 관리 프로필 디렉터리로 복구합니다.
    ///
    /// `staged`는 이 backend가 생성한 tombstone이어야 하고 `destination`은 같은 프로필 부모의
    /// 일치하는 숫자 디렉터리여야 합니다. 성공하면 tombstone이 원래 위치로 이동됩니다.
    fn restore_staged(&self, staged: &Path, destination: &Path) -> io::Result<()>;

    /// 검증된 tombstone 디렉터리를 최종 제거합니다.
    ///
    /// `staged`가 이 backend가 생성하고 승인한 경로일 때만 재귀 삭제하며, 그 외 입력은
    /// 보존하고 오류를 반환합니다.
    fn remove_staged(&self, staged: &Path) -> io::Result<()>;

    /// 시작 시 프로필 루트 아래의 검증된 tombstone만 정리합니다.
    ///
    /// `root`의 프로필 부모만 열거합니다. `catalog`가 참조하는 프로필은 원래 위치로 복구하거나
    /// 충돌 시 보존하고, catalog에서 삭제가 커밋된 프로필의 tombstone만 최종 제거합니다.
    fn cleanup_staged(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()>;

    /// 카탈로그에 없는 미완료 add의 빈 관리 홈을 안전하게 정리합니다.
    ///
    /// `root` 아래에서 `catalog`의 정확한 다음 숫자와 일치하는 디렉터리만 검사하고, catalog가
    /// 참조하는 프로필·다른 sequence·비어 있지 않은 홈은 보존합니다. 성공하면 재시도 가능한
    /// 빈 orphan만 제거됩니다.
    fn cleanup_orphaned_homes(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()>;

    /// 관리 프로필의 실행 홈이 안전한 실제 디렉터리인지 검증합니다.
    ///
    /// 구현은 `root`와 `id`에서만 경로를 파생하고 reparse point를 포함한 경로 탈출을 거부해야
    /// 합니다. 기본 구현은 기존 테스트·외부 backend 호환성을 위해 성공하며, 운영 backend는 실제
    /// 파일 시스템 검증을 재정의합니다.
    fn validate_managed_home(
        &self,
        _root: &UsageProfileRoot,
        _id: UsageProfileId,
    ) -> io::Result<()> {
        Ok(())
    }
}

/// 운영 체제 파일 시스템에서 관리 프로필 트랜잭션을 수행하는 안전 구현입니다.
///
/// 삭제 경로는 `UsageProfileRoot`와 숫자 ID에서만 만들며, 현재 프로세스가 생성한 tombstone만
/// 최종 삭제할 수 있습니다. 시작 정리는 별도로 루트 아래의 엄격히 검증된 tombstone만 제거합니다.
#[derive(Clone, Default)]
pub struct NativeProfileFileSystem {
    staged: Arc<Mutex<HashSet<PathBuf>>>,
}

impl ProfileFileSystem for NativeProfileFileSystem {
    fn create_managed_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()> {
        let paths = ManagedPaths::derive(root, id)?;
        reject_reparse_ancestors(&paths.profiles)?;
        fs::create_dir_all(&paths.profiles)?;
        require_safe_directory(&paths.profiles)?;
        if paths.profile.exists() {
            reject_reparse(&paths.profile)?;
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "managed profile directory already exists",
            ));
        }
        fs::create_dir(&paths.profile)?;
        if let Err(error) = fs::create_dir(&paths.home) {
            let _ = fs::remove_dir(&paths.profile);
            return Err(error);
        }
        Ok(())
    }

    fn remove_empty_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()> {
        let paths = ManagedPaths::derive(root, id)?;
        require_safe_directory(&paths.profiles)?;
        require_safe_directory(&paths.profile)?;
        require_safe_directory(&paths.home)?;
        reject_reparse_ancestors(&paths.home)?;
        fs::remove_dir(&paths.home)?;
        reject_reparse_ancestors(&paths.profile)?;
        fs::remove_dir(&paths.profile)?;
        Ok(())
    }

    fn stage_delete(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<PathBuf> {
        let paths = ManagedPaths::derive(root, id)?;
        require_safe_directory(&paths.profiles)?;
        require_safe_directory(&paths.profile)?;
        let UsageProfileId::Managed(sequence) = id else {
            return Err(invalid_profile_path());
        };
        let staged = paths.profiles.join(format!(
            ".deleting-profile-{sequence:04}-{}-{}",
            std::process::id(),
            TOMBSTONE_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        if staged.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "profile tombstone already exists",
            ));
        }
        reject_reparse_ancestors(&paths.profile)?;
        fs::rename(&paths.profile, &staged)?;
        self.staged
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(staged.clone());
        Ok(staged)
    }

    fn restore_staged(&self, staged: &Path, destination: &Path) -> io::Result<()> {
        let staged = absolute_path(staged)?;
        let destination = absolute_path(destination)?;
        self.require_authorized(&staged)?;
        let sequence = validate_tombstone_path(&staged)?;
        let profiles = staged.parent().ok_or_else(invalid_profile_path)?;
        if destination.parent() != Some(profiles)
            || destination.file_name() != Some(format!("profile-{sequence:04}").as_ref())
        {
            return Err(invalid_profile_path());
        }
        require_safe_directory(profiles)?;
        require_safe_directory(&staged)?;
        if destination.exists() {
            reject_reparse(&destination)?;
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "managed profile restore destination exists",
            ));
        }
        reject_reparse_ancestors(&staged)?;
        reject_reparse_ancestors(&destination)?;
        fs::rename(&staged, &destination)?;
        self.forget_authorized(&staged);
        Ok(())
    }

    fn remove_staged(&self, staged: &Path) -> io::Result<()> {
        let staged = absolute_path(staged)?;
        self.require_authorized(&staged)?;
        validate_tombstone_path(&staged)?;
        require_safe_directory(&staged)?;
        reject_reparse_ancestors(&staged)?;
        fs::remove_dir_all(&staged)?;
        self.forget_authorized(&staged);
        Ok(())
    }

    fn cleanup_staged(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        let profiles = profiles_directory(root)?;
        reject_reparse_ancestors(&profiles)?;
        let entries = match fs::read_dir(&profiles) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        require_safe_directory(&profiles)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let Some(sequence) = parse_tombstone_name(&entry.file_name()) else {
                continue;
            };
            require_safe_directory(&path)?;
            let id = UsageProfileId::Managed(sequence);
            if catalog.contains(id) {
                let destination = ManagedPaths::derive(root, id)?.profile;
                if destination.exists() {
                    require_safe_directory(&destination)?;
                    continue;
                }
                reject_reparse_ancestors(&path)?;
                reject_reparse_ancestors(&destination)?;
                fs::rename(path, destination)?;
            } else {
                reject_reparse_ancestors(&path)?;
                fs::remove_dir_all(path)?;
            }
        }
        Ok(())
    }

    fn cleanup_orphaned_homes(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        let profiles = profiles_directory(root)?;
        reject_reparse_ancestors(&profiles)?;
        let entries = match fs::read_dir(&profiles) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        require_safe_directory(&profiles)?;
        for entry in entries {
            let entry = entry?;
            let Some(sequence) = parse_profile_directory_name(&entry.file_name()) else {
                continue;
            };
            let id = UsageProfileId::Managed(sequence);
            if catalog.contains(id) || catalog.next_managed_id() != Some(id) {
                continue;
            }
            let paths = ManagedPaths::derive(root, id)?;
            require_safe_directory(&paths.profile)?;
            match fs::symlink_metadata(&paths.home) {
                Ok(_) => match self.remove_empty_home(root, id) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
                    Err(error) => return Err(error),
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    reject_reparse_ancestors(&paths.profile)?;
                    match fs::remove_dir(&paths.profile) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn validate_managed_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()> {
        let paths = ManagedPaths::derive(root, id)?;
        reject_reparse_ancestors(&paths.home)?;
        require_safe_directory(&paths.profiles)?;
        require_safe_directory(&paths.profile)?;
        require_safe_directory(&paths.home)
    }
}

impl NativeProfileFileSystem {
    fn require_authorized(&self, staged: &Path) -> io::Result<()> {
        if self
            .staged
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(staged)
        {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "profile tombstone is not authorized",
            ))
        }
    }

    fn forget_authorized(&self, staged: &Path) {
        self.staged
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(staged);
    }
}

struct ManagedPaths {
    profiles: PathBuf,
    profile: PathBuf,
    home: PathBuf,
}

impl ManagedPaths {
    fn derive(root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<Self> {
        let profiles = profiles_directory(root)?;
        let home = root.codex_home(id).map_err(|_| invalid_profile_path())?;
        let home = absolute_path(&home)?;
        let profile = home
            .parent()
            .ok_or_else(invalid_profile_path)?
            .to_path_buf();
        if profile.parent() != Some(profiles.as_path()) {
            return Err(invalid_profile_path());
        }
        Ok(Self {
            profiles,
            profile,
            home,
        })
    }
}

fn profiles_directory(root: &UsageProfileRoot) -> io::Result<PathBuf> {
    Ok(absolute_path(root.base())?.join("profiles"))
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    std::path::absolute(path).map_err(|_| invalid_profile_path())
}

fn invalid_profile_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid managed profile path")
}

fn validate_tombstone_path(path: &Path) -> io::Result<u32> {
    let parent = path.parent().ok_or_else(invalid_profile_path)?;
    if parent.file_name() != Some("profiles".as_ref()) {
        return Err(invalid_profile_path());
    }
    parse_tombstone_name(path.file_name().ok_or_else(invalid_profile_path)?)
        .ok_or_else(invalid_profile_path)
}

fn parse_tombstone_name(name: &std::ffi::OsStr) -> Option<u32> {
    let name = name.to_str()?;
    let suffix = name.strip_prefix(".deleting-profile-")?;
    let mut parts = suffix.split('-');
    let sequence_text = parts.next()?;
    let process_text = parts.next()?;
    let nonce_text = parts.next()?;
    if parts.next().is_some()
        || sequence_text.len() < 4
        || !sequence_text.bytes().all(|byte| byte.is_ascii_digit())
        || !process_text.bytes().all(|byte| byte.is_ascii_digit())
        || !nonce_text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let sequence = sequence_text.parse::<u32>().ok()?;
    let process = process_text.parse::<u32>().ok()?;
    let nonce = nonce_text.parse::<u64>().ok()?;
    if sequence == 0
        || process == 0
        || sequence_text != format!("{sequence:04}")
        || process_text != process.to_string()
        || nonce_text != nonce.to_string()
    {
        return None;
    }
    Some(sequence)
}

fn parse_profile_directory_name(name: &std::ffi::OsStr) -> Option<u32> {
    let name = name.to_str()?;
    let sequence_text = name.strip_prefix("profile-")?;
    if sequence_text.len() < 4 || !sequence_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let sequence = sequence_text.parse::<u32>().ok()?;
    if sequence == 0 || sequence_text != format!("{sequence:04}") {
        return None;
    }
    Some(sequence)
}

fn reject_reparse_ancestors(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => reject_reparse_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn reject_reparse(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    reject_reparse_metadata(&metadata)
}

fn require_safe_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    reject_reparse_metadata(&metadata)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed profile path is not a directory",
        ))
    }
}

fn reject_reparse_metadata(metadata: &fs::Metadata) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "managed profile path is a reparse point",
            ));
        }
    }
    #[cfg(not(windows))]
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "managed profile path is a symbolic link",
        ));
    }
    Ok(())
}

/// 설정과 프로필 디렉터리를 같은 작업 큐에서 변경하는 명령입니다.
#[derive(Clone, PartialEq, Eq)]
pub enum ProfileSettingsMutation {
    /// 검증한 표시 이름으로 새 관리 프로필을 추가합니다.
    Add { label: String },
    /// 지정한 사용량 프로필(시스템 또는 관리)의 표시 이름을 변경합니다.
    Rename { id: UsageProfileId, label: String },
    /// 사용량 표시 대상으로 사용할 프로필을 선택합니다.
    Select { id: UsageProfileId },
    /// 관리 프로필의 설정과 격리 디렉터리를 트랜잭션으로 삭제합니다.
    Delete { id: UsageProfileId },
}

impl std::fmt::Debug for ProfileSettingsMutation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add { .. } => formatter.write_str("Add { label: [redacted] }"),
            Self::Rename { id, .. } => formatter
                .debug_struct("Rename")
                .field("id", id)
                .field("label", &"[redacted]")
                .finish(),
            Self::Select { id } => formatter.debug_struct("Select").field("id", id).finish(),
            Self::Delete { id } => formatter.debug_struct("Delete").field("id", id).finish(),
        }
    }
}

impl ProfileSettingsMutation {
    pub(crate) fn operation(&self) -> ProfileSettingsOperation {
        match self {
            Self::Add { .. } => ProfileSettingsOperation::Add,
            Self::Rename { .. } => ProfileSettingsOperation::Rename,
            Self::Select { .. } => ProfileSettingsOperation::Select,
            Self::Delete { .. } => ProfileSettingsOperation::Delete,
        }
    }
}

/// 프로필 설정 요청과 완료 이벤트를 연결하는 프로세스 내부 식별자입니다.
///
/// 값은 서비스 인스턴스 안에서 1부터 단조 증가하며 프로필 ID, 이름 또는 경로에서 파생되지
/// 않습니다. 로그나 사용자 화면에 표시하지 않습니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileSettingsRequestId(u64);

impl ProfileSettingsRequestId {
    /// 0이 아닌 불투명 요청 번호를 생성합니다.
    ///
    /// `value`가 0이면 `None`을 반환합니다. 테스트와 런타임 조정 계층에서 이미 발급된 번호를
    /// 형식화할 때 사용하며 프로필 정보는 포함하지 않습니다.
    pub fn new(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }
}

/// 프로필 설정 이벤트의 민감하지 않은 작업 종류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileSettingsOperation {
    /// 관리 프로필 추가입니다.
    Add,
    /// 시스템 또는 관리 프로필의 표시 이름 변경입니다.
    Rename,
    /// 표시 프로필 선택입니다.
    Select,
    /// 관리 프로필 삭제입니다.
    Delete,
    /// 일반 UI 환경설정 저장입니다.
    Preferences,
    /// 시작 시 tombstone 정리입니다.
    Cleanup,
    /// 시작 또는 add 재시도 전 orphan 정리입니다.
    AddCleanup,
    /// 시작 시 관리 프로필 실행 경로 검증입니다.
    StartupValidation,
}

/// 시작 복구와 관리 프로필 실행 경로 검증의 민감하지 않은 집계 결과입니다.
///
/// 개수에는 시스템 프로필을 포함하며 이름, 내부 식별자와 경로를 보관하지 않습니다. 복구 실패는
/// 모든 관리 프로필을 보수적으로 차단하고, 검증 실패는 해당 관리 프로필만 차단합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSettingsStartupReport {
    /// 설정 catalog의 전체 프로필 수입니다.
    pub configured: u8,
    /// 시스템 프로필을 포함해 이번 실행에서 사용할 수 있는 프로필 수입니다.
    pub launchable: u8,
    /// tombstone 또는 미완료 add 복구가 실패했는지 나타냅니다.
    pub recovery_failed: bool,
    /// 안전 경로 검증에 실패한 관리 프로필 수입니다.
    pub validation_failed: u8,
}

/// 설정 worker를 시작하기 전에 완료된 복구·검증 handshake 결과입니다.
///
/// 실행 컨텍스트는 복구가 끝난 뒤 안전 검증을 통과한 프로필에 대해서만 생성됩니다. 공개 디버그
/// 출력은 집계 결과만 포함하며 관리 프로필 식별자나 경로를 노출하지 않습니다.
pub struct ProfileSettingsStartup {
    execution_contexts: Vec<crate::ProfileExecutionContext>,
    report: ProfileSettingsStartupReport,
}

impl ProfileSettingsStartup {
    /// 검증을 통과한 시스템·관리 프로필 실행 컨텍스트를 순서대로 반환합니다.
    ///
    /// 시스템 프로필은 항상 첫 항목이며 관리 컨텍스트는 설정 catalog 순서를 유지합니다.
    pub fn execution_contexts(&self) -> &[crate::ProfileExecutionContext] {
        &self.execution_contexts
    }

    /// 이름·식별자·경로가 없는 시작 복구 집계 결과를 반환합니다.
    pub fn report(&self) -> ProfileSettingsStartupReport {
        self.report
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<crate::ProfileExecutionContext>,
        ProfileSettingsStartupReport,
    ) {
        (self.execution_contexts, self.report)
    }
}

impl std::fmt::Debug for ProfileSettingsStartup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfileSettingsStartup")
            .field("report", &self.report)
            .finish()
    }
}

/// 기존 호출자가 소비하는 순서 보장 프로필 설정 작업의 완료 결과입니다.
///
/// 요청 상관관계가 필요한 런타임은 `CorrelatedProfileSettingsEvent`를 사용합니다. 이 호환 타입은
/// 기존 variant shape를 유지하며 custom `Debug`에서 설정과 표시 이름을 생략합니다.
#[derive(Clone, PartialEq, Eq)]
pub enum ProfileSettingsEvent {
    /// 새 프로필과 저장된 최신 설정을 반환합니다.
    Added {
        settings: Settings,
        id: UsageProfileId,
    },
    /// 이름이 변경된 프로필과 저장된 최신 설정을 반환합니다.
    Renamed {
        settings: Settings,
        id: UsageProfileId,
    },
    /// 선택된 프로필과 저장된 최신 설정을 반환합니다.
    Selected {
        settings: Settings,
        id: UsageProfileId,
    },
    /// 삭제된 프로필과 저장된 최신 설정을 반환합니다.
    Deleted {
        settings: Settings,
        id: UsageProfileId,
    },
    /// 작업 단계와 외부 노출에 안전한 오류 종류를 반환합니다.
    Failed {
        operation: &'static str,
        kind: io::ErrorKind,
    },
}

impl std::fmt::Debug for ProfileSettingsEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Added { id, .. } => formatter.debug_struct("Added").field("id", id).finish(),
            Self::Renamed { id, .. } => formatter.debug_struct("Renamed").field("id", id).finish(),
            Self::Selected { id, .. } => {
                formatter.debug_struct("Selected").field("id", id).finish()
            }
            Self::Deleted { id, .. } => formatter.debug_struct("Deleted").field("id", id).finish(),
            Self::Failed { operation, kind } => formatter
                .debug_struct("Failed")
                .field("operation", operation)
                .field("kind", kind)
                .finish(),
        }
    }
}

/// 요청 ID와 타입 지정 작업 종류를 포함하는 런타임용 프로필 설정 완료 이벤트입니다.
///
/// 설정 worker가 발급한 요청 ID를 그대로 돌려주며 preference/시작 정리 실패는 `None`을
/// 사용합니다. custom `Debug`는 설정, 프로필 이름과 관리 경로를 출력하지 않습니다.
#[derive(Clone, PartialEq, Eq)]
pub enum CorrelatedProfileSettingsEvent {
    /// 상관 ID와 내구성 있게 추가된 프로필 설정입니다.
    Added {
        request_id: ProfileSettingsRequestId,
        settings: Settings,
        id: UsageProfileId,
    },
    /// 상관 ID와 내구성 있게 이름이 변경된 설정입니다.
    Renamed {
        request_id: ProfileSettingsRequestId,
        settings: Settings,
        id: UsageProfileId,
    },
    /// 상관 ID와 내구성 있게 선택된 설정입니다.
    Selected {
        request_id: ProfileSettingsRequestId,
        settings: Settings,
        id: UsageProfileId,
    },
    /// 상관 ID와 내구성 있게 삭제된 설정입니다.
    Deleted {
        request_id: ProfileSettingsRequestId,
        settings: Settings,
        id: UsageProfileId,
    },
    /// 요청 ID, 타입 지정 작업, 호환 stage와 안전 오류 종류입니다.
    Failed {
        request_id: Option<ProfileSettingsRequestId>,
        operation: ProfileSettingsOperation,
        stage: &'static str,
        kind: io::ErrorKind,
    },
}

impl std::fmt::Debug for CorrelatedProfileSettingsEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Added { request_id, id, .. } => formatter
                .debug_struct("Added")
                .field("request_id", request_id)
                .field("id", id)
                .finish(),
            Self::Renamed { request_id, id, .. } => formatter
                .debug_struct("Renamed")
                .field("request_id", request_id)
                .field("id", id)
                .finish(),
            Self::Selected { request_id, id, .. } => formatter
                .debug_struct("Selected")
                .field("request_id", request_id)
                .field("id", id)
                .finish(),
            Self::Deleted { request_id, id, .. } => formatter
                .debug_struct("Deleted")
                .field("request_id", request_id)
                .field("id", id)
                .finish(),
            Self::Failed {
                request_id,
                operation,
                stage,
                kind,
            } => formatter
                .debug_struct("Failed")
                .field("request_id", request_id)
                .field("operation", operation)
                .field("stage", stage)
                .field("kind", kind)
                .finish(),
        }
    }
}

impl CorrelatedProfileSettingsEvent {
    fn into_legacy(self) -> ProfileSettingsEvent {
        match self {
            Self::Added { settings, id, .. } => ProfileSettingsEvent::Added { settings, id },
            Self::Renamed { settings, id, .. } => ProfileSettingsEvent::Renamed { settings, id },
            Self::Selected { settings, id, .. } => ProfileSettingsEvent::Selected { settings, id },
            Self::Deleted { settings, id, .. } => ProfileSettingsEvent::Deleted { settings, id },
            Self::Failed { stage, kind, .. } => ProfileSettingsEvent::Failed {
                operation: stage,
                kind,
            },
        }
    }
}

enum ProfileSettingsCommand {
    Mutate {
        request_id: ProfileSettingsRequestId,
        mutation: ProfileSettingsMutation,
    },
    SavePreferences(Settings),
    Flush(mpsc::SyncSender<io::Result<()>>),
    Stop,
}

/// 프로필 카탈로그와 일반 환경설정을 하나의 순서 보장 worker에서 저장합니다.
pub struct ProfileSettingsService {
    commands: mpsc::Sender<ProfileSettingsCommand>,
    events: mpsc::Receiver<CorrelatedProfileSettingsEvent>,
    worker: Option<JoinHandle<io::Result<()>>>,
    next_request_id: AtomicU64,
}

impl ProfileSettingsService {
    /// 시작 복구와 경로 검증을 완료한 뒤 현재 설정을 worker가 소유하도록 옮깁니다.
    ///
    /// 이 호환 진입점은 복구 결과를 버리지만 관리 프로필을 사용할 수 있는 상태가 결정될 때까지
    /// 반환하지 않습니다. 이후 변경 I/O는 worker에서 실행되고 결과는 이벤트로 확인합니다.
    pub fn start(
        store: SettingsStore,
        settings: Settings,
        backend: impl ProfileFileSystem + 'static,
    ) -> Self {
        Self::start_with_recovery(store, settings, backend).0
    }

    /// 시작 복구·검증을 동기적으로 완료하고 설정 worker와 실행 가능한 컨텍스트를 반환합니다.
    ///
    /// tombstone 복구와 미완료 add 정리가 관리 컨텍스트 생성보다 먼저 끝납니다. 복구 실패 시 모든
    /// 관리 프로필을, 개별 경로 검증 실패 시 해당 프로필만 제외하며 시스템 프로필은 유지합니다.
    /// 실패 상세는 경로·이름 없는 타입 지정 이벤트와 집계 보고서로만 노출됩니다.
    pub fn start_with_recovery(
        store: SettingsStore,
        settings: Settings,
        backend: impl ProfileFileSystem + 'static,
    ) -> (Self, ProfileSettingsStartup) {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let backend: Arc<dyn ProfileFileSystem> = Arc::new(backend);
        let root = UsageProfileRoot::new(store.root().to_path_buf());
        let startup = prepare_profile_startup(
            &root,
            &settings.usage_profiles,
            backend.as_ref(),
            &event_sender,
        );
        let worker = thread::spawn(move || {
            profile_settings_loop(store, settings, backend, command_receiver, event_sender)
        });
        (
            Self {
                commands: command_sender,
                events: event_receiver,
                worker: Some(worker),
                next_request_id: AtomicU64::new(1),
            },
            startup,
        )
    }

    /// 프로필 변경을 기다리지 않고 순서 보장 작업 큐에 추가합니다.
    ///
    /// `mutation`은 worker가 설정과 파일 시스템을 함께 변경합니다. 큐 제출 성공 여부만 즉시
    /// 반환하며 실제 결과는 같은 요청 ID를 포함한 이벤트로 전달됩니다.
    pub fn submit(&self, mutation: ProfileSettingsMutation) -> io::Result<()> {
        self.submit_correlated(mutation).map(|_| ())
    }

    /// 프로필 변경을 제출하고 완료 이벤트와 연결할 불투명 요청 ID를 반환합니다.
    ///
    /// 기존 `submit`과 같은 worker 큐를 사용하며 외부 I/O를 기다리지 않습니다. 런타임은 반환된
    /// ID를 `take_correlated_events`의 이벤트와 비교해야 합니다.
    pub fn submit_correlated(
        &self,
        mutation: ProfileSettingsMutation,
    ) -> io::Result<ProfileSettingsRequestId> {
        let raw = self
            .next_request_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| io::Error::other("profile settings request id exhausted"))?;
        let request_id = ProfileSettingsRequestId(raw);
        self.commands
            .send(ProfileSettingsCommand::Mutate {
                request_id,
                mutation,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile settings stopped"))?;
        Ok(request_id)
    }

    /// 일반 환경설정 복사본을 프로필 변경과 같은 순서 보장 작업 큐에 추가합니다.
    ///
    /// 입력의 프로필 카탈로그와 스키마 버전은 복사하지 않으므로 오래된 UI 스냅샷이 worker의 최신
    /// 프로필 상태를 되돌릴 수 없습니다. 실제 저장 결과는 `flush`로 확인할 수 있습니다.
    pub fn save_preferences(&self, preferences: Settings) -> io::Result<()> {
        self.commands
            .send(ProfileSettingsCommand::SavePreferences(preferences))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile settings stopped"))
    }

    /// 앞서 제출한 모든 작업이 끝날 때까지 기다리고 첫 설정 저장 오류를 반환합니다.
    ///
    /// 테스트, 진단 또는 종료 과정에서만 사용하며 UI 이벤트 처리 중에는 호출하지 않습니다.
    pub fn flush(&self) -> io::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.commands
            .send(ProfileSettingsCommand::Flush(sender))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile settings stopped"))?;
        receiver.recv().map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "profile settings response lost")
        })?
    }

    /// 현재 도착한 완료 이벤트를 기다리지 않고 모두 반환합니다.
    ///
    /// 반환 시점에 아직 처리 중인 작업은 포함하지 않으며 디스크 I/O를 수행하지 않습니다.
    pub fn take_events(&self) -> Vec<ProfileSettingsEvent> {
        self.take_correlated_events()
            .into_iter()
            .map(CorrelatedProfileSettingsEvent::into_legacy)
            .collect()
    }

    /// 현재 도착한 상관 ID 포함 완료 이벤트를 기다리지 않고 모두 반환합니다.
    ///
    /// AppRuntime처럼 profile pending 작업을 정확히 연결해야 하는 호출자가 사용합니다. 호출은
    /// 디스크 I/O를 수행하지 않으며 기존 `take_events`와 동시에 사용하면 안 됩니다.
    pub fn take_correlated_events(&self) -> Vec<CorrelatedProfileSettingsEvent> {
        self.events.try_iter().collect()
    }

    /// 다음 완료 이벤트가 도착할 때까지 기다립니다.
    ///
    /// 테스트나 종료 조정처럼 UI 이벤트 처리 밖에서만 호출해야 합니다.
    pub fn wait_for_event(&self) -> io::Result<ProfileSettingsEvent> {
        self.wait_for_correlated_event()
            .map(CorrelatedProfileSettingsEvent::into_legacy)
    }

    /// 다음 상관 ID 포함 완료 이벤트가 도착할 때까지 기다립니다.
    ///
    /// 테스트 또는 종료 조정에서만 사용하며 UI 이벤트 처리에서는 호출하지 않습니다.
    pub fn wait_for_correlated_event(&self) -> io::Result<CorrelatedProfileSettingsEvent> {
        self.events
            .recv()
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile settings event lost"))
    }

    /// 앞선 명령을 처리한 뒤 worker를 종료하고 worker 오류를 반환합니다.
    pub fn stop(mut self) -> io::Result<()> {
        let _ = self.commands.send(ProfileSettingsCommand::Stop);
        join_worker(self.worker.take())
    }
}

impl Drop for ProfileSettingsService {
    fn drop(&mut self) {
        let _ = self.commands.send(ProfileSettingsCommand::Stop);
        let _ = join_worker(self.worker.take());
    }
}

fn prepare_profile_startup(
    root: &UsageProfileRoot,
    catalog: &UsageProfileCatalog,
    backend: &dyn ProfileFileSystem,
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
) -> ProfileSettingsStartup {
    let mut recovery_failed = false;
    if let Err(error) = backend.cleanup_staged(root, catalog) {
        recovery_failed = true;
        send_failed(
            events,
            None,
            ProfileSettingsOperation::Cleanup,
            "cleanup",
            error.kind(),
        );
    }
    if !recovery_failed {
        if let Err(error) = backend.cleanup_orphaned_homes(root, catalog) {
            recovery_failed = true;
            send_failed(
                events,
                None,
                ProfileSettingsOperation::AddCleanup,
                "add_cleanup",
                error.kind(),
            );
        }
    }

    let mut execution_contexts = vec![crate::ProfileExecutionContext::system()];
    let mut validation_failed = 0_u8;
    if !recovery_failed {
        for profile in catalog.managed() {
            let id = profile.id();
            let context = backend.validate_managed_home(root, id).and_then(|()| {
                crate::ProfileExecutionContext::managed(root, id).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid managed profile")
                })
            });
            match context {
                Ok(context) => execution_contexts.push(context),
                Err(error) => {
                    validation_failed = validation_failed.saturating_add(1);
                    send_failed(
                        events,
                        None,
                        ProfileSettingsOperation::StartupValidation,
                        "startup_validation",
                        error.kind(),
                    );
                }
            }
        }
    }

    ProfileSettingsStartup {
        report: ProfileSettingsStartupReport {
            configured: (catalog.managed().len() + 1).min(crate::MAX_USAGE_PROFILES) as u8,
            launchable: execution_contexts.len().min(crate::MAX_USAGE_PROFILES) as u8,
            recovery_failed,
            validation_failed,
        },
        execution_contexts,
    }
}

fn profile_settings_loop(
    store: SettingsStore,
    mut settings: Settings,
    backend: Arc<dyn ProfileFileSystem>,
    commands: mpsc::Receiver<ProfileSettingsCommand>,
    events: mpsc::Sender<CorrelatedProfileSettingsEvent>,
) -> io::Result<()> {
    let root = UsageProfileRoot::new(store.root().to_path_buf());
    let mut first_error: Option<io::ErrorKind> = None;

    while let Ok(command) = commands.recv() {
        match command {
            ProfileSettingsCommand::Mutate {
                request_id,
                mutation: ProfileSettingsMutation::Add { label },
            } => {
                add_profile(
                    &store,
                    &root,
                    backend.as_ref(),
                    &mut settings,
                    &label,
                    &events,
                    &mut first_error,
                    request_id,
                );
            }
            ProfileSettingsCommand::Mutate {
                request_id,
                mutation: ProfileSettingsMutation::Rename { id, label },
            } => {
                rename_profile(
                    &store,
                    &mut settings,
                    id,
                    &label,
                    &events,
                    &mut first_error,
                    request_id,
                );
            }
            ProfileSettingsCommand::Mutate {
                request_id,
                mutation: ProfileSettingsMutation::Select { id },
            } => {
                select_profile(
                    &store,
                    &mut settings,
                    id,
                    &events,
                    &mut first_error,
                    request_id,
                );
            }
            ProfileSettingsCommand::Mutate {
                request_id,
                mutation: ProfileSettingsMutation::Delete { id },
            } => {
                delete_profile(
                    &store,
                    &root,
                    backend.as_ref(),
                    &mut settings,
                    id,
                    &events,
                    &mut first_error,
                    request_id,
                );
            }
            ProfileSettingsCommand::SavePreferences(mut preferences) => {
                preferences.schema_version = settings.schema_version;
                preferences.usage_profiles = settings.usage_profiles.clone();
                match store.save(&preferences) {
                    Ok(()) => settings = preferences,
                    Err(error) => {
                        remember_first_save_error(&mut first_error, error.kind());
                        send_failed(
                            &events,
                            None,
                            ProfileSettingsOperation::Preferences,
                            "preferences",
                            error.kind(),
                        );
                    }
                }
            }
            ProfileSettingsCommand::Flush(sender) => {
                let result = first_error
                    .map(|kind| Err(io::Error::new(kind, "settings save failed")))
                    .unwrap_or(Ok(()));
                let _ = sender.send(result);
            }
            ProfileSettingsCommand::Stop => break,
        }
    }
    first_error
        .map(|kind| Err(io::Error::new(kind, "settings save failed")))
        .unwrap_or(Ok(()))
}

fn rename_profile(
    store: &SettingsStore,
    settings: &mut Settings,
    id: UsageProfileId,
    label: &str,
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
    first_error: &mut Option<io::ErrorKind>,
    request_id: ProfileSettingsRequestId,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.rename(id, label).is_err() {
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Rename,
            "rename",
            io::ErrorKind::InvalidInput,
        );
        return;
    }
    if let Err(error) = store.save(&candidate) {
        remember_first_save_error(first_error, error.kind());
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Rename,
            "rename",
            error.kind(),
        );
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(CorrelatedProfileSettingsEvent::Renamed {
        request_id,
        settings: candidate,
        id,
    });
}

fn select_profile(
    store: &SettingsStore,
    settings: &mut Settings,
    id: UsageProfileId,
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
    first_error: &mut Option<io::ErrorKind>,
    request_id: ProfileSettingsRequestId,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.select(id).is_err() {
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Select,
            "select",
            io::ErrorKind::InvalidInput,
        );
        return;
    }
    if let Err(error) = store.save(&candidate) {
        remember_first_save_error(first_error, error.kind());
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Select,
            "select",
            error.kind(),
        );
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(CorrelatedProfileSettingsEvent::Selected {
        request_id,
        settings: candidate,
        id,
    });
}

#[allow(clippy::too_many_arguments)]
fn add_profile(
    store: &SettingsStore,
    root: &UsageProfileRoot,
    backend: &dyn ProfileFileSystem,
    settings: &mut Settings,
    label: &str,
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
    first_error: &mut Option<io::ErrorKind>,
    request_id: ProfileSettingsRequestId,
) {
    if let Err(error) = backend.cleanup_orphaned_homes(root, &settings.usage_profiles) {
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Add,
            "add_cleanup",
            error.kind(),
        );
        return;
    }
    let mut candidate = settings.clone();
    let profile = match candidate.usage_profiles.add(label) {
        Ok(profile) => profile,
        Err(_) => {
            send_failed(
                events,
                Some(request_id),
                ProfileSettingsOperation::Add,
                "add",
                io::ErrorKind::InvalidInput,
            );
            return;
        }
    };
    let id = profile.id();
    if let Err(error) = backend.create_managed_home(root, id) {
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Add,
            "add",
            error.kind(),
        );
        return;
    }
    if let Err(error) = store.save(&candidate) {
        remember_first_save_error(first_error, error.kind());
        if let Err(rollback_error) = backend.remove_empty_home(root, id) {
            send_failed(
                events,
                Some(request_id),
                ProfileSettingsOperation::Add,
                "add_rollback",
                rollback_error.kind(),
            );
            return;
        }
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Add,
            "add",
            error.kind(),
        );
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(CorrelatedProfileSettingsEvent::Added {
        request_id,
        settings: candidate,
        id,
    });
}

#[allow(clippy::too_many_arguments)]
fn delete_profile(
    store: &SettingsStore,
    root: &UsageProfileRoot,
    backend: &dyn ProfileFileSystem,
    settings: &mut Settings,
    id: UsageProfileId,
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
    first_error: &mut Option<io::ErrorKind>,
    request_id: ProfileSettingsRequestId,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.remove(id).is_err() {
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Delete,
            "delete",
            io::ErrorKind::InvalidInput,
        );
        return;
    }
    let staged = match backend.stage_delete(root, id) {
        Ok(staged) => staged,
        Err(error) => {
            send_failed(
                events,
                Some(request_id),
                ProfileSettingsOperation::Delete,
                "delete",
                error.kind(),
            );
            return;
        }
    };
    if let Err(error) = store.save(&candidate) {
        remember_first_save_error(first_error, error.kind());
        let destination = root.managed_directory(id).ok();
        if let Some(destination) = destination {
            if let Err(restore_error) = backend.restore_staged(&staged, &destination) {
                send_failed(
                    events,
                    Some(request_id),
                    ProfileSettingsOperation::Delete,
                    "delete_restore",
                    restore_error.kind(),
                );
                return;
            }
        }
        send_failed(
            events,
            Some(request_id),
            ProfileSettingsOperation::Delete,
            "delete",
            error.kind(),
        );
        return;
    }
    *settings = candidate.clone();
    let _ = backend.remove_staged(&staged);
    let _ = events.send(CorrelatedProfileSettingsEvent::Deleted {
        request_id,
        settings: candidate,
        id,
    });
}

fn remember_first_save_error(first_error: &mut Option<io::ErrorKind>, kind: io::ErrorKind) {
    if first_error.is_none() {
        *first_error = Some(kind);
    }
}

fn send_failed(
    events: &mpsc::Sender<CorrelatedProfileSettingsEvent>,
    request_id: Option<ProfileSettingsRequestId>,
    operation: ProfileSettingsOperation,
    stage: &'static str,
    kind: io::ErrorKind,
) {
    let _ = events.send(CorrelatedProfileSettingsEvent::Failed {
        request_id,
        operation,
        stage,
        kind,
    });
}

fn join_worker(worker: Option<JoinHandle<io::Result<()>>>) -> io::Result<()> {
    match worker {
        Some(worker) => worker
            .join()
            .map_err(|_| io::Error::other("profile settings worker panicked"))?,
        None => Ok(()),
    }
}
