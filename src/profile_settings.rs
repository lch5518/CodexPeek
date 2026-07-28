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

use crate::{Settings, SettingsStore, UsageProfileId, UsageProfileRoot};

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
    /// `root`의 프로필 부모만 열거하며 일반 프로필과 형식이 다른 항목은 그대로 둡니다.
    fn cleanup_staged(&self, root: &UsageProfileRoot) -> io::Result<()>;
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
        fs::remove_dir(&paths.home)?;
        fs::remove_dir(&paths.profile)?;
        let _ = fs::remove_dir(&paths.profiles);
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
        fs::rename(&staged, &destination)?;
        self.forget_authorized(&staged);
        Ok(())
    }

    fn remove_staged(&self, staged: &Path) -> io::Result<()> {
        let staged = absolute_path(staged)?;
        self.require_authorized(&staged)?;
        validate_tombstone_path(&staged)?;
        require_safe_directory(&staged)?;
        fs::remove_dir_all(&staged)?;
        self.forget_authorized(&staged);
        Ok(())
    }

    fn cleanup_staged(&self, root: &UsageProfileRoot) -> io::Result<()> {
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
            if parse_tombstone_name(&entry.file_name()).is_none() {
                continue;
            }
            require_safe_directory(&path)?;
            fs::remove_dir_all(path)?;
        }
        Ok(())
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileSettingsMutation {
    /// 검증한 표시 이름으로 새 관리 프로필을 추가합니다.
    Add { label: String },
    /// 기존 관리 프로필의 표시 이름을 변경합니다.
    Rename { id: UsageProfileId, label: String },
    /// 사용량 표시 대상으로 사용할 프로필을 선택합니다.
    Select { id: UsageProfileId },
    /// 관리 프로필의 설정과 격리 디렉터리를 트랜잭션으로 삭제합니다.
    Delete { id: UsageProfileId },
}

/// 순서가 보장된 프로필 설정 작업의 완료 결과입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
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

enum ProfileSettingsCommand {
    Mutate(ProfileSettingsMutation),
    SavePreferences(Settings),
    Flush(mpsc::SyncSender<io::Result<()>>),
    Stop,
}

/// 프로필 카탈로그와 일반 환경설정을 하나의 순서 보장 worker에서 저장합니다.
pub struct ProfileSettingsService {
    commands: mpsc::Sender<ProfileSettingsCommand>,
    events: mpsc::Receiver<ProfileSettingsEvent>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl ProfileSettingsService {
    /// 저장소의 현재 설정을 worker가 소유하도록 옮기고 비동기 서비스를 시작합니다.
    ///
    /// `backend`의 시작 정리와 이후 파일 I/O는 worker에서 실행됩니다. 호출은 디스크 작업을
    /// 기다리지 않으며, 결과는 `take_events` 또는 `wait_for_event`로 확인합니다.
    pub fn start(
        store: SettingsStore,
        settings: Settings,
        backend: impl ProfileFileSystem + 'static,
    ) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let backend: Arc<dyn ProfileFileSystem> = Arc::new(backend);
        let worker = thread::spawn(move || {
            profile_settings_loop(store, settings, backend, command_receiver, event_sender)
        });
        Self {
            commands: command_sender,
            events: event_receiver,
            worker: Some(worker),
        }
    }

    /// 프로필 변경을 기다리지 않고 순서 보장 작업 큐에 추가합니다.
    ///
    /// `mutation`은 worker가 설정과 파일 시스템을 함께 변경합니다. 큐 제출 성공 여부만 즉시
    /// 반환하며 실제 결과는 이벤트로 전달됩니다.
    pub fn submit(&self, mutation: ProfileSettingsMutation) -> io::Result<()> {
        self.commands
            .send(ProfileSettingsCommand::Mutate(mutation))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "profile settings stopped"))
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
        self.events.try_iter().collect()
    }

    /// 다음 완료 이벤트가 도착할 때까지 기다립니다.
    ///
    /// 테스트나 종료 조정처럼 UI 이벤트 처리 밖에서만 호출해야 합니다.
    pub fn wait_for_event(&self) -> io::Result<ProfileSettingsEvent> {
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

fn profile_settings_loop(
    store: SettingsStore,
    mut settings: Settings,
    backend: Arc<dyn ProfileFileSystem>,
    commands: mpsc::Receiver<ProfileSettingsCommand>,
    events: mpsc::Sender<ProfileSettingsEvent>,
) -> io::Result<()> {
    let root = UsageProfileRoot::new(store.root().to_path_buf());
    let mut first_error: Option<io::ErrorKind> = None;
    if let Err(error) = backend.cleanup_staged(&root) {
        let _ = events.send(ProfileSettingsEvent::Failed {
            operation: "cleanup",
            kind: error.kind(),
        });
    }

    while let Ok(command) = commands.recv() {
        match command {
            ProfileSettingsCommand::Mutate(ProfileSettingsMutation::Add { label }) => {
                add_profile(
                    &store,
                    &root,
                    backend.as_ref(),
                    &mut settings,
                    &label,
                    &events,
                );
            }
            ProfileSettingsCommand::Mutate(ProfileSettingsMutation::Rename { id, label }) => {
                rename_profile(&store, &mut settings, id, &label, &events);
            }
            ProfileSettingsCommand::Mutate(ProfileSettingsMutation::Select { id }) => {
                select_profile(&store, &mut settings, id, &events);
            }
            ProfileSettingsCommand::Mutate(ProfileSettingsMutation::Delete { id }) => {
                delete_profile(&store, &root, backend.as_ref(), &mut settings, id, &events);
            }
            ProfileSettingsCommand::SavePreferences(mut preferences) => {
                preferences.schema_version = settings.schema_version;
                preferences.usage_profiles = settings.usage_profiles.clone();
                match store.save(&preferences) {
                    Ok(()) => settings = preferences,
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error.kind());
                        }
                        send_failed(&events, "preferences", error.kind());
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
    events: &mpsc::Sender<ProfileSettingsEvent>,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.rename(id, label).is_err() {
        send_failed(events, "rename", io::ErrorKind::InvalidInput);
        return;
    }
    if let Err(error) = store.save(&candidate) {
        send_failed(events, "rename", error.kind());
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(ProfileSettingsEvent::Renamed {
        settings: candidate,
        id,
    });
}

fn select_profile(
    store: &SettingsStore,
    settings: &mut Settings,
    id: UsageProfileId,
    events: &mpsc::Sender<ProfileSettingsEvent>,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.select(id).is_err() {
        send_failed(events, "select", io::ErrorKind::InvalidInput);
        return;
    }
    if let Err(error) = store.save(&candidate) {
        send_failed(events, "select", error.kind());
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(ProfileSettingsEvent::Selected {
        settings: candidate,
        id,
    });
}

fn add_profile(
    store: &SettingsStore,
    root: &UsageProfileRoot,
    backend: &dyn ProfileFileSystem,
    settings: &mut Settings,
    label: &str,
    events: &mpsc::Sender<ProfileSettingsEvent>,
) {
    let mut candidate = settings.clone();
    let profile = match candidate.usage_profiles.add(label) {
        Ok(profile) => profile,
        Err(_) => {
            send_failed(events, "add", io::ErrorKind::InvalidInput);
            return;
        }
    };
    let id = profile.id();
    if let Err(error) = backend.create_managed_home(root, id) {
        send_failed(events, "add", error.kind());
        return;
    }
    if let Err(error) = store.save(&candidate) {
        let _ = backend.remove_empty_home(root, id);
        send_failed(events, "add", error.kind());
        return;
    }
    *settings = candidate.clone();
    let _ = events.send(ProfileSettingsEvent::Added {
        settings: candidate,
        id,
    });
}

fn delete_profile(
    store: &SettingsStore,
    root: &UsageProfileRoot,
    backend: &dyn ProfileFileSystem,
    settings: &mut Settings,
    id: UsageProfileId,
    events: &mpsc::Sender<ProfileSettingsEvent>,
) {
    let mut candidate = settings.clone();
    if candidate.usage_profiles.remove(id).is_err() {
        send_failed(events, "delete", io::ErrorKind::InvalidInput);
        return;
    }
    let staged = match backend.stage_delete(root, id) {
        Ok(staged) => staged,
        Err(error) => {
            send_failed(events, "delete", error.kind());
            return;
        }
    };
    if let Err(error) = store.save(&candidate) {
        let destination = root.managed_directory(id).ok();
        if let Some(destination) = destination {
            if let Err(restore_error) = backend.restore_staged(&staged, &destination) {
                send_failed(events, "delete_restore", restore_error.kind());
                return;
            }
        }
        send_failed(events, "delete", error.kind());
        return;
    }
    *settings = candidate.clone();
    let _ = backend.remove_staged(&staged);
    let _ = events.send(ProfileSettingsEvent::Deleted {
        settings: candidate,
        id,
    });
}

fn send_failed(
    events: &mpsc::Sender<ProfileSettingsEvent>,
    operation: &'static str,
    kind: io::ErrorKind,
) {
    let _ = events.send(ProfileSettingsEvent::Failed { operation, kind });
}

fn join_worker(worker: Option<JoinHandle<io::Result<()>>>) -> io::Result<()> {
    match worker {
        Some(worker) => worker
            .join()
            .map_err(|_| io::Error::other("profile settings worker panicked"))?,
        None => Ok(()),
    }
}
