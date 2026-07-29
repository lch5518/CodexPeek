use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    normalize_profile_label, CorrelatedProfileSettingsEvent as RuntimeSettingsEvent,
    NativeProfileFileSystem, PollTrigger, ProfileFileSystem, ProfilePollEvent,
    ProfileRuntimeCommand, ProfileRuntimeState, ProfileSettingsEvent, ProfileSettingsMutation,
    ProfileSettingsOperation, ProfileSettingsRequestId, ProfileSettingsService,
    ProfileSettingsStartupReport, ProfileValidationError, Settings, SettingsStore, UsageError,
    UsageProfileCatalog, UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
};

#[derive(Clone)]
struct RecordingProfileFileSystem {
    operations: Arc<Mutex<Vec<&'static str>>>,
    store: SettingsStore,
}

impl RecordingProfileFileSystem {
    fn new(store: SettingsStore) -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
            store,
        }
    }

    fn operations(&self) -> Vec<&'static str> {
        self.operations.lock().unwrap().clone()
    }
}

impl ProfileFileSystem for RecordingProfileFileSystem {
    fn create_managed_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        self.operations.lock().unwrap().push("create_managed_home");
        Ok(())
    }

    fn remove_empty_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        self.operations.lock().unwrap().push("remove_empty_home");
        Ok(())
    }

    fn stage_delete(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<PathBuf> {
        self.operations.lock().unwrap().push("stage_delete");
        Ok(PathBuf::from("validated-tombstone"))
    }

    fn restore_staged(&self, _staged: &Path, _destination: &Path) -> io::Result<()> {
        self.operations.lock().unwrap().push("restore_staged");
        Ok(())
    }

    fn remove_staged(&self, _staged: &Path) -> io::Result<()> {
        assert!(self.store.load()?.usage_profiles.managed().is_empty());
        let mut operations = self.operations.lock().unwrap();
        operations.push("save_settings");
        operations.push("remove_staged");
        Ok(())
    }

    fn cleanup_staged(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        Ok(())
    }

    fn cleanup_orphaned_homes(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct RollbackFailureProfileFileSystem {
    state: Arc<Mutex<RollbackFailureState>>,
}

#[derive(Default)]
struct RollbackFailureState {
    orphaned: bool,
    fail_rollback_once: bool,
    operations: Vec<&'static str>,
}

impl RollbackFailureProfileFileSystem {
    fn with_one_rollback_failure() -> Self {
        Self {
            state: Arc::new(Mutex::new(RollbackFailureState {
                fail_rollback_once: true,
                ..RollbackFailureState::default()
            })),
        }
    }

    fn operations(&self) -> Vec<&'static str> {
        self.state.lock().unwrap().operations.clone()
    }
}

impl ProfileFileSystem for RollbackFailureProfileFileSystem {
    fn create_managed_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("create_managed_home");
        if state.orphaned {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "injected orphan",
            ));
        }
        state.orphaned = true;
        Ok(())
    }

    fn remove_empty_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.operations.push("remove_empty_home");
        if state.fail_rollback_once {
            state.fail_rollback_once = false;
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected rollback failure",
            ));
        }
        state.orphaned = false;
        Ok(())
    }

    fn stage_delete(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<PathBuf> {
        unreachable!()
    }

    fn restore_staged(&self, _staged: &Path, _destination: &Path) -> io::Result<()> {
        unreachable!()
    }

    fn remove_staged(&self, _staged: &Path) -> io::Result<()> {
        unreachable!()
    }

    fn cleanup_staged(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        Ok(())
    }

    fn cleanup_orphaned_homes(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.orphaned {
            state.operations.push("cleanup_orphaned_home");
            state.orphaned = false;
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct StartupRecordingFileSystem {
    operations: Arc<Mutex<Vec<&'static str>>>,
    validation_failure: Option<UsageProfileId>,
    recovery_failure: bool,
}

impl StartupRecordingFileSystem {
    fn failing_validation(id: UsageProfileId) -> Self {
        Self {
            validation_failure: Some(id),
            ..Self::default()
        }
    }

    fn failing_recovery() -> Self {
        Self {
            recovery_failure: true,
            ..Self::default()
        }
    }

    fn operations(&self) -> Vec<&'static str> {
        self.operations.lock().unwrap().clone()
    }
}

impl ProfileFileSystem for StartupRecordingFileSystem {
    fn create_managed_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        unreachable!()
    }

    fn remove_empty_home(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<()> {
        unreachable!()
    }

    fn stage_delete(&self, _root: &UsageProfileRoot, _id: UsageProfileId) -> io::Result<PathBuf> {
        unreachable!()
    }

    fn restore_staged(&self, _staged: &Path, _destination: &Path) -> io::Result<()> {
        unreachable!()
    }

    fn remove_staged(&self, _staged: &Path) -> io::Result<()> {
        unreachable!()
    }

    fn cleanup_staged(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        self.operations.lock().unwrap().push("recover_tombstones");
        if self.recovery_failure {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "SENTINEL_PRIVATE_TOMBSTONE_PATH",
            ));
        }
        Ok(())
    }

    fn cleanup_orphaned_homes(
        &self,
        _root: &UsageProfileRoot,
        _catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        self.operations.lock().unwrap().push("recover_orphans");
        Ok(())
    }

    fn validate_managed_home(
        &self,
        _root: &UsageProfileRoot,
        id: UsageProfileId,
    ) -> io::Result<()> {
        self.operations
            .lock()
            .unwrap()
            .push("validate_managed_home");
        if self.validation_failure == Some(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SENTINEL_PRIVATE_REPARSE_PATH",
            ));
        }
        Ok(())
    }
}

fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codex-peek-profile-{label}-{nonce}"))
}

struct RecordingProfileRuntime {
    state: ProfileRuntimeState,
    poll_commands: Vec<String>,
    settings_commands: Vec<String>,
    next_request_id: u64,
    last_request_id: Option<ProfileSettingsRequestId>,
}

impl RecordingProfileRuntime {
    fn new(settings: Settings, root: PathBuf) -> Self {
        Self {
            state: ProfileRuntimeState::new(settings, UsageProfileRoot::new(root)),
            poll_commands: Vec::new(),
            settings_commands: Vec::new(),
            next_request_id: 1,
            last_request_id: None,
        }
    }

    fn request_add(&mut self, label: String) -> Result<(), ProfileValidationError> {
        let commands = self.state.request_add(label)?;
        self.record(commands);
        Ok(())
    }

    fn request_add_with_login_confirmation(
        &mut self,
        label: String,
        confirmed: bool,
    ) -> Result<(), ProfileValidationError> {
        let commands = self
            .state
            .request_add_with_login_confirmation(label, confirmed)?;
        self.record(commands);
        Ok(())
    }

    fn request_delete(&mut self, id: UsageProfileId) -> Result<(), ProfileValidationError> {
        let commands = self.state.request_delete(id)?;
        self.record(commands);
        Ok(())
    }

    fn apply_settings_event(&mut self, event: RuntimeSettingsEvent) {
        let commands = self.state.apply_settings_event(event);
        self.record(commands);
    }

    fn apply_poll_event(&mut self, event: ProfilePollEvent) {
        let commands = self.state.apply_poll_event(event);
        self.record(commands);
    }

    fn record(&mut self, commands: Vec<ProfileRuntimeCommand>) {
        for command in commands {
            match command {
                ProfileRuntimeCommand::Settings(ProfileSettingsMutation::Add { .. }) => {
                    self.bind_recorded_request(ProfileSettingsOperation::Add);
                    self.settings_commands.push("add".to_owned());
                }
                ProfileRuntimeCommand::Settings(ProfileSettingsMutation::Rename { id, .. }) => {
                    self.bind_recorded_request(ProfileSettingsOperation::Rename);
                    self.settings_commands
                        .push(format!("rename:{}", id_text(id)));
                }
                ProfileRuntimeCommand::Settings(ProfileSettingsMutation::Select { id }) => {
                    self.bind_recorded_request(ProfileSettingsOperation::Select);
                    self.settings_commands
                        .push(format!("select:{}", id_text(id)));
                }
                ProfileRuntimeCommand::Settings(ProfileSettingsMutation::Delete { id }) => {
                    self.bind_recorded_request(ProfileSettingsOperation::Delete);
                    self.settings_commands
                        .push(format!("delete:{}", id_text(id)));
                }
                ProfileRuntimeCommand::AddPollContext(context) => self
                    .poll_commands
                    .push(format!("add:{}", id_text(context.id()))),
                ProfileRuntimeCommand::SelectPoll(id) => {
                    self.poll_commands.push(format!("select:{}", id_text(id)))
                }
                ProfileRuntimeCommand::RefreshSelected(trigger) => {
                    self.poll_commands.push(format!(
                        "refresh:{}",
                        if trigger == PollTrigger::ForcedAuth {
                            "forced-auth"
                        } else {
                            "other"
                        }
                    ))
                }
                ProfileRuntimeCommand::Login(id) => {
                    self.poll_commands.push(format!("login:{}", id_text(id)))
                }
                ProfileRuntimeCommand::Logout(id) => {
                    self.poll_commands.push(format!("logout:{}", id_text(id)))
                }
                ProfileRuntimeCommand::Quiesce(id) => {
                    self.poll_commands.push(format!("quiesce:{}", id_text(id)))
                }
                ProfileRuntimeCommand::Resume(id) => {
                    self.poll_commands.push(format!("resume:{}", id_text(id)))
                }
                ProfileRuntimeCommand::Remove(id) => {
                    self.poll_commands.push(format!("remove:{}", id_text(id)))
                }
            }
        }
    }

    fn poll_commands(&self) -> &[String] {
        &self.poll_commands
    }

    fn settings_commands(&self) -> &[String] {
        &self.settings_commands
    }

    fn last_request_id(&self) -> ProfileSettingsRequestId {
        self.last_request_id.expect("settings request was recorded")
    }

    fn bind_recorded_request(&mut self, operation: ProfileSettingsOperation) {
        let request_id = ProfileSettingsRequestId::new(self.next_request_id).unwrap();
        self.next_request_id += 1;
        assert!(self.state.bind_settings_request(request_id, operation));
        self.last_request_id = Some(request_id);
    }
}

fn id_text(id: UsageProfileId) -> String {
    match id {
        UsageProfileId::System => "system".to_owned(),
        UsageProfileId::Managed(sequence) => format!("managed-{sequence}"),
    }
}

fn settings_with_profile(sequence: u32, label: &str) -> Settings {
    let mut settings = Settings::default();
    for current in 1..=sequence {
        settings
            .usage_profiles
            .add(if current == sequence {
                label
            } else {
                "Earlier"
            })
            .unwrap();
    }
    settings
}

fn profile_runtime_fixture() -> RecordingProfileRuntime {
    RecordingProfileRuntime::new(Settings::default(), test_root("runtime-empty"))
}

fn selected_managed_profile_fixture() -> RecordingProfileRuntime {
    let mut settings = settings_with_profile(1, "개인");
    settings
        .usage_profiles
        .select(UsageProfileId::Managed(1))
        .unwrap();
    RecordingProfileRuntime::new(settings, test_root("runtime-selected"))
}

#[test]
fn added_profile_is_logged_in_only_after_durable_settings_success() {
    let mut runtime = profile_runtime_fixture();

    runtime
        .request_add_with_login_confirmation("개인".into(), true)
        .unwrap();
    assert!(runtime.poll_commands().is_empty());

    runtime.apply_settings_event(RuntimeSettingsEvent::Added {
        request_id: runtime.last_request_id(),
        settings: settings_with_profile(1, "개인"),
        id: UsageProfileId::Managed(1),
    });
    assert_eq!(
        runtime.poll_commands(),
        ["add:managed-1", "login:managed-1"]
    );
}

#[test]
fn cancelled_post_add_login_keeps_login_required_profile_without_login_operation() {
    let mut runtime = profile_runtime_fixture();

    runtime.request_add("개인".into()).unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Added {
        request_id: runtime.last_request_id(),
        settings: settings_with_profile(1, "개인"),
        id: UsageProfileId::Managed(1),
    });

    assert_eq!(runtime.poll_commands(), ["add:managed-1"]);
    assert!(runtime.state.login_required(UsageProfileId::Managed(1)));
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::System
    );
}

#[test]
fn existing_profile_login_requires_explicit_confirmation_to_emit_operation() {
    let id = UsageProfileId::Managed(1);
    let mut state = ProfileRuntimeState::new(
        settings_with_profile(1, "Work"),
        UsageProfileRoot::new(test_root("confirmed-existing-login")),
    );

    assert!(state.request_login(id).unwrap().is_empty());
    assert!(!state.mutation_pending());
    assert!(state
        .request_login_with_confirmation(id, false)
        .unwrap()
        .is_empty());
    assert_eq!(
        state.request_login_with_confirmation(id, true).unwrap(),
        [ProfileRuntimeCommand::Login(id)]
    );
}

#[test]
fn delete_waits_for_quiesce_before_storage_delete() {
    let mut runtime = selected_managed_profile_fixture();

    runtime.request_delete(UsageProfileId::Managed(1)).unwrap();
    assert_eq!(runtime.poll_commands(), ["quiesce:managed-1"]);
    assert!(runtime.settings_commands().is_empty());

    runtime.apply_poll_event(ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
        1,
    )));
    assert_eq!(runtime.settings_commands(), ["delete:managed-1"]);
}

#[test]
fn failed_selection_save_retains_the_previous_render_selection() {
    let mut runtime = selected_managed_profile_fixture();

    let commands = runtime
        .state
        .request_select(UsageProfileId::System)
        .unwrap();
    runtime.record(commands);
    runtime.apply_settings_event(RuntimeSettingsEvent::Failed {
        request_id: Some(runtime.last_request_id()),
        operation: ProfileSettingsOperation::Select,
        stage: "select",
        kind: io::ErrorKind::PermissionDenied,
    });

    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::Managed(1)
    );
    assert!(!runtime.state.mutation_pending());
}

#[test]
fn system_rename_is_normalized_and_applied_only_after_durable_success() {
    let mut runtime = profile_runtime_fixture();

    let commands = runtime
        .state
        .request_rename(UsageProfileId::System, "  Main  ".to_owned())
        .unwrap();
    runtime.record(commands);

    assert_eq!(runtime.settings_commands(), ["rename:system"]);
    assert_eq!(runtime.state.settings().usage_profiles.system_label(), None);
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::System
    );
    assert!(runtime.poll_commands().is_empty());

    let mut renamed = Settings::default();
    renamed
        .usage_profiles
        .rename(UsageProfileId::System, "Main")
        .unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Renamed {
        request_id: runtime.last_request_id(),
        settings: renamed,
        id: UsageProfileId::System,
    });

    assert_eq!(
        runtime.state.settings().usage_profiles.system_label(),
        Some("Main")
    );
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::System
    );
    assert!(!runtime.state.mutation_pending());
    assert!(runtime.poll_commands().is_empty());
}

#[test]
fn preference_and_stale_failures_do_not_clear_profile_pending_request() {
    let mut runtime = selected_managed_profile_fixture();
    runtime
        .state
        .request_select(UsageProfileId::System)
        .unwrap();
    let active = ProfileSettingsRequestId::new(41).unwrap();
    runtime
        .state
        .bind_settings_request(active, ProfileSettingsOperation::Select);

    runtime.apply_settings_event(RuntimeSettingsEvent::Failed {
        request_id: None,
        operation: ProfileSettingsOperation::Preferences,
        stage: "preferences",
        kind: io::ErrorKind::PermissionDenied,
    });
    assert!(runtime.state.mutation_pending());

    runtime.apply_settings_event(RuntimeSettingsEvent::Failed {
        request_id: Some(ProfileSettingsRequestId::new(40).unwrap()),
        operation: ProfileSettingsOperation::Select,
        stage: "select",
        kind: io::ErrorKind::PermissionDenied,
    });
    assert!(runtime.state.mutation_pending());

    runtime.apply_settings_event(RuntimeSettingsEvent::Failed {
        request_id: Some(active),
        operation: ProfileSettingsOperation::Select,
        stage: "select",
        kind: io::ErrorKind::PermissionDenied,
    });
    assert!(!runtime.state.mutation_pending());
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::Managed(1)
    );
}

#[test]
fn mismatched_success_event_cannot_update_pending_selection() {
    let mut runtime = selected_managed_profile_fixture();
    runtime
        .state
        .request_select(UsageProfileId::System)
        .unwrap();
    let active = ProfileSettingsRequestId::new(51).unwrap();
    runtime
        .state
        .bind_settings_request(active, ProfileSettingsOperation::Select);
    let mut selected_system = runtime.state.settings().clone();
    selected_system
        .usage_profiles
        .select(UsageProfileId::System)
        .unwrap();

    runtime.apply_settings_event(RuntimeSettingsEvent::Selected {
        request_id: ProfileSettingsRequestId::new(52).unwrap(),
        settings: selected_system,
        id: UsageProfileId::System,
    });

    assert!(runtime.state.mutation_pending());
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::Managed(1)
    );
    assert!(runtime.poll_commands().is_empty());
}

#[test]
fn successful_login_selects_durably_before_forced_refresh() {
    let mut runtime = profile_runtime_fixture();
    runtime
        .request_add_with_login_confirmation("개인".into(), true)
        .unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Added {
        request_id: runtime.last_request_id(),
        settings: settings_with_profile(1, "개인"),
        id: UsageProfileId::Managed(1),
    });
    runtime.poll_commands.clear();
    runtime.settings_commands.clear();

    runtime.apply_poll_event(ProfilePollEvent::LoginFinished {
        id: UsageProfileId::Managed(1),
        result: Ok(true),
    });
    assert_eq!(runtime.settings_commands(), ["select:managed-1"]);
    assert!(runtime.poll_commands().is_empty());

    let mut selected = settings_with_profile(1, "개인");
    selected
        .usage_profiles
        .select(UsageProfileId::Managed(1))
        .unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Selected {
        request_id: runtime.last_request_id(),
        settings: selected,
        id: UsageProfileId::Managed(1),
    });
    assert_eq!(
        runtime.poll_commands(),
        ["select:managed-1", "refresh:forced-auth"]
    );
}

#[test]
fn cancelled_login_retains_login_required_without_selecting_profile() {
    let mut runtime = profile_runtime_fixture();
    runtime
        .request_add_with_login_confirmation("개인".into(), true)
        .unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Added {
        request_id: runtime.last_request_id(),
        settings: settings_with_profile(1, "개인"),
        id: UsageProfileId::Managed(1),
    });

    runtime.apply_poll_event(ProfilePollEvent::LoginFinished {
        id: UsageProfileId::Managed(1),
        result: Ok(false),
    });

    assert!(runtime.state.login_required(UsageProfileId::Managed(1)));
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::System
    );
}

#[test]
fn login_error_is_profile_local() {
    let mut settings = settings_with_profile(2, "두 번째");
    settings
        .usage_profiles
        .rename(UsageProfileId::Managed(1), "첫 번째")
        .unwrap();
    let mut runtime = RecordingProfileRuntime::new(settings, test_root("runtime-errors"));

    runtime.apply_poll_event(ProfilePollEvent::LoginFinished {
        id: UsageProfileId::Managed(1),
        result: Err(UsageError::AuthenticationExpired),
    });

    assert!(runtime.state.login_required(UsageProfileId::Managed(1)));
    assert!(!runtime.state.login_required(UsageProfileId::Managed(2)));
}

#[test]
fn deleting_selected_profile_falls_back_to_system_after_durable_delete() {
    let mut runtime = selected_managed_profile_fixture();
    runtime.request_delete(UsageProfileId::Managed(1)).unwrap();
    runtime.apply_poll_event(ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
        1,
    )));
    runtime.poll_commands.clear();

    runtime.apply_settings_event(RuntimeSettingsEvent::Deleted {
        request_id: runtime.last_request_id(),
        settings: Settings::default(),
        id: UsageProfileId::Managed(1),
    });

    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::System
    );
    assert_eq!(
        runtime.poll_commands(),
        ["remove:managed-1", "select:system"]
    );
}

#[test]
fn deleting_unselected_profile_keeps_authoritative_managed_selection() {
    let mut settings = settings_with_profile(2, "두 번째");
    settings
        .usage_profiles
        .select(UsageProfileId::Managed(1))
        .unwrap();
    let mut runtime = RecordingProfileRuntime::new(settings, test_root("runtime-delete-other"));
    runtime.request_delete(UsageProfileId::Managed(2)).unwrap();
    runtime.apply_poll_event(ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
        2,
    )));
    runtime.poll_commands.clear();

    let mut deleted = settings_with_profile(2, "두 번째");
    deleted
        .usage_profiles
        .select(UsageProfileId::Managed(1))
        .unwrap();
    deleted
        .usage_profiles
        .remove(UsageProfileId::Managed(2))
        .unwrap();
    runtime.apply_settings_event(RuntimeSettingsEvent::Deleted {
        request_id: runtime.last_request_id(),
        settings: deleted,
        id: UsageProfileId::Managed(2),
    });

    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::Managed(1)
    );
    assert_eq!(
        runtime.poll_commands(),
        ["remove:managed-2", "select:managed-1"]
    );
}

#[test]
fn failed_staged_delete_resumes_only_the_quiesced_profile() {
    let mut runtime = selected_managed_profile_fixture();
    runtime.request_delete(UsageProfileId::Managed(1)).unwrap();
    runtime.apply_poll_event(ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
        1,
    )));
    runtime.poll_commands.clear();

    runtime.apply_settings_event(RuntimeSettingsEvent::Failed {
        request_id: Some(runtime.last_request_id()),
        operation: ProfileSettingsOperation::Delete,
        stage: "delete",
        kind: io::ErrorKind::PermissionDenied,
    });

    assert_eq!(runtime.poll_commands(), ["resume:managed-1"]);
    assert_eq!(
        runtime.state.settings().usage_profiles.selected(),
        UsageProfileId::Managed(1)
    );
    assert!(!runtime.state.mutation_pending());
}

#[test]
fn successful_logout_marks_only_the_target_profile_login_required() {
    let settings = settings_with_profile(2, "두 번째");
    let mut runtime = RecordingProfileRuntime::new(settings, test_root("runtime-logout"));
    runtime
        .state
        .request_logout(UsageProfileId::Managed(2))
        .unwrap();

    runtime.apply_poll_event(ProfilePollEvent::LogoutFinished {
        id: UsageProfileId::Managed(2),
        result: Ok(()),
    });

    assert!(!runtime.state.login_required(UsageProfileId::Managed(1)));
    assert!(runtime.state.login_required(UsageProfileId::Managed(2)));
    assert!(!runtime.state.mutation_pending());
}

#[cfg(windows)]
fn create_junction(link: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    let output = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn profile_settings_delete_stages_directory_saves_then_removes_staged_directory() {
    let root = test_root("ordered-delete");
    let store = SettingsStore::for_root(&root);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let backend = RecordingProfileFileSystem::new(store.clone());
    let service = ProfileSettingsService::start(store.clone(), settings, backend.clone());

    service
        .submit(ProfileSettingsMutation::Delete { id })
        .unwrap();

    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Deleted { id: deleted, .. } if deleted == id
    ));
    assert_eq!(
        backend.operations(),
        ["stage_delete", "save_settings", "remove_staged"]
    );
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn profile_settings_events_echo_monotonic_request_ids() {
    let root = test_root("request-correlation");
    let store = SettingsStore::for_root(&root);
    let settings = Settings::default();
    store.save(&settings).unwrap();
    let service = ProfileSettingsService::start(
        store.clone(),
        settings,
        RecordingProfileFileSystem::new(store),
    );

    let add_request = service
        .submit_correlated(ProfileSettingsMutation::Add {
            label: "Work".to_owned(),
        })
        .unwrap();
    let added = service.wait_for_correlated_event().unwrap();
    assert!(matches!(
        added,
        RuntimeSettingsEvent::Added { request_id, .. } if request_id == add_request
    ));

    let rename_request = service
        .submit_correlated(ProfileSettingsMutation::Rename {
            id: UsageProfileId::Managed(1),
            label: "Office".to_owned(),
        })
        .unwrap();
    let renamed = service.wait_for_correlated_event().unwrap();
    assert!(rename_request > add_request);
    assert!(matches!(
        renamed,
        RuntimeSettingsEvent::Renamed { request_id, .. } if request_id == rename_request
    ));
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn legacy_profile_settings_api_signature_and_event_shape_remain_compatible() {
    let _submit: fn(&ProfileSettingsService, ProfileSettingsMutation) -> io::Result<()> =
        ProfileSettingsService::submit;
    let event = ProfileSettingsEvent::Failed {
        operation: "preferences",
        kind: io::ErrorKind::PermissionDenied,
    };

    assert!(matches!(
        event,
        ProfileSettingsEvent::Failed {
            operation: "preferences",
            kind: io::ErrorKind::PermissionDenied,
        }
    ));
}

#[test]
fn profile_settings_event_debug_never_contains_labels_or_paths() {
    let root = test_root("debug-redaction");
    let store = SettingsStore::for_root(&root);
    let settings = Settings::default();
    store.save(&settings).unwrap();
    let service = ProfileSettingsService::start(
        store.clone(),
        settings,
        RecordingProfileFileSystem::new(store),
    );
    let sentinel_label = "SENTINEL_PRIVATE_LABEL";

    service
        .submit_correlated(ProfileSettingsMutation::Add {
            label: sentinel_label.to_owned(),
        })
        .unwrap();
    let event = service.wait_for_correlated_event().unwrap();
    let debug = format!("{event:?}");

    assert!(!debug.contains(sentinel_label));
    assert!(!debug.contains(root.to_string_lossy().as_ref()));
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn profile_settings_add_save_failure_removes_only_new_empty_home() {
    let root = test_root("add-save-rollback");
    let store = SettingsStore::for_root(&root);
    let settings = Settings {
        refresh_interval_minutes: 2,
        ..Settings::default()
    };
    let backend = RecordingProfileFileSystem::new(store.clone());
    let service = ProfileSettingsService::start(store, settings, backend.clone());

    service
        .submit(ProfileSettingsMutation::Add {
            label: "Work".to_owned(),
        })
        .unwrap();

    assert_eq!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Failed {
            operation: "add",
            kind: io::ErrorKind::InvalidInput,
        }
    );
    assert_eq!(
        backend.operations(),
        ["create_managed_home", "remove_empty_home"]
    );
    assert_eq!(
        service.flush().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        service.stop().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn add_rollback_failure_is_reported_and_orphan_is_retried_before_next_add() {
    let root = test_root("add-rollback-retry");
    let store = SettingsStore::for_root(&root);
    let invalid = Settings {
        refresh_interval_minutes: 2,
        ..Settings::default()
    };
    let backend = RollbackFailureProfileFileSystem::with_one_rollback_failure();
    let service = ProfileSettingsService::start(store.clone(), invalid, backend.clone());

    service
        .submit(ProfileSettingsMutation::Add {
            label: "Work".to_owned(),
        })
        .unwrap();
    assert_eq!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Failed {
            operation: "add_rollback",
            kind: io::ErrorKind::PermissionDenied,
        }
    );

    service.save_preferences(Settings::default()).unwrap();
    service
        .submit(ProfileSettingsMutation::Add {
            label: "Work".to_owned(),
        })
        .unwrap();
    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Added { .. }
    ));
    assert_eq!(store.load().unwrap().usage_profiles.managed().len(), 1);
    assert_eq!(
        backend.operations(),
        [
            "create_managed_home",
            "remove_empty_home",
            "cleanup_orphaned_home",
            "create_managed_home",
        ]
    );
    assert_eq!(
        service.stop().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn profile_settings_delete_save_failure_restores_staged_directory() {
    let root = test_root("delete-save-rollback");
    let store = SettingsStore::for_root(&root);
    let mut settings = Settings {
        refresh_interval_minutes: 2,
        ..Settings::default()
    };
    let id = settings.usage_profiles.add("Work").unwrap().id();
    let backend = RecordingProfileFileSystem::new(store.clone());
    let service = ProfileSettingsService::start(store, settings, backend.clone());

    service
        .submit(ProfileSettingsMutation::Delete { id })
        .unwrap();

    assert_eq!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Failed {
            operation: "delete",
            kind: io::ErrorKind::InvalidInput,
        }
    );
    assert_eq!(backend.operations(), ["stage_delete", "restore_staged"]);
    assert_eq!(
        service.flush().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        service.stop().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rename_and_select_save_failures_are_retained_by_flush_and_stop() {
    let root = test_root("rename-select-save-errors");
    let store = SettingsStore::for_root(&root);
    let mut settings = Settings {
        refresh_interval_minutes: 2,
        ..Settings::default()
    };
    let id = settings.usage_profiles.add("Work").unwrap().id();
    let backend = RecordingProfileFileSystem::new(store.clone());
    let service = ProfileSettingsService::start(store, settings, backend);

    service
        .submit(ProfileSettingsMutation::Rename {
            id,
            label: "Office".to_owned(),
        })
        .unwrap();
    assert_eq!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Failed {
            operation: "rename",
            kind: io::ErrorKind::InvalidInput,
        }
    );
    service
        .submit(ProfileSettingsMutation::Select { id })
        .unwrap();
    assert_eq!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Failed {
            operation: "select",
            kind: io::ErrorKind::InvalidInput,
        }
    );

    assert_eq!(
        service.flush().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        service.stop().unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let _ = std::fs::remove_dir_all(root);
}

#[derive(Clone, Default)]
struct FailFinalRemovalFileSystem {
    native: NativeProfileFileSystem,
    staged: Arc<Mutex<Option<PathBuf>>>,
}

impl ProfileFileSystem for FailFinalRemovalFileSystem {
    fn create_managed_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()> {
        self.native.create_managed_home(root, id)
    }

    fn remove_empty_home(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<()> {
        self.native.remove_empty_home(root, id)
    }

    fn stage_delete(&self, root: &UsageProfileRoot, id: UsageProfileId) -> io::Result<PathBuf> {
        let staged = self.native.stage_delete(root, id)?;
        *self.staged.lock().unwrap() = Some(staged.clone());
        Ok(staged)
    }

    fn restore_staged(&self, staged: &Path, destination: &Path) -> io::Result<()> {
        self.native.restore_staged(staged, destination)
    }

    fn remove_staged(&self, _staged: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected final removal failure",
        ))
    }

    fn cleanup_staged(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        self.native.cleanup_staged(root, catalog)
    }

    fn cleanup_orphaned_homes(
        &self,
        root: &UsageProfileRoot,
        catalog: &UsageProfileCatalog,
    ) -> io::Result<()> {
        self.native.cleanup_orphaned_homes(root, catalog)
    }
}

#[test]
fn native_profile_filesystem_rejects_system_and_zero_ids() {
    let root_path = test_root("invalid-native-id");
    let root = UsageProfileRoot::new(root_path.clone());
    let backend = NativeProfileFileSystem::default();

    assert_eq!(
        backend
            .create_managed_home(&root, UsageProfileId::System)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        backend
            .create_managed_home(&root, UsageProfileId::Managed(0))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!root_path.exists());
}

#[cfg(windows)]
#[test]
fn stage_delete_rejects_a_reparse_point_in_the_settings_root_ancestor() {
    let container = test_root("stage-reparse-ancestor");
    let target = container.join("target");
    let linked_root = container.join("linked-root");
    let id = UsageProfileId::Managed(1);
    let target_root = UsageProfileRoot::new(target.clone());
    std::fs::create_dir_all(target_root.codex_home(id).unwrap()).unwrap();
    create_junction(&linked_root, &target);
    let linked = UsageProfileRoot::new(linked_root.clone());

    let error = NativeProfileFileSystem::default()
        .stage_delete(&linked, id)
        .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(target_root.codex_home(id).unwrap().is_dir());
    std::fs::remove_dir(&linked_root).unwrap();
    let _ = std::fs::remove_dir_all(container);
}

#[cfg(windows)]
#[test]
fn empty_home_rollback_rejects_a_reparse_point_in_the_settings_root_ancestor() {
    let container = test_root("rollback-reparse-ancestor");
    let target = container.join("target");
    let linked_root = container.join("linked-root");
    let id = UsageProfileId::Managed(1);
    let target_root = UsageProfileRoot::new(target.clone());
    std::fs::create_dir_all(target_root.codex_home(id).unwrap()).unwrap();
    create_junction(&linked_root, &target);
    let linked = UsageProfileRoot::new(linked_root.clone());

    let error = NativeProfileFileSystem::default()
        .remove_empty_home(&linked, id)
        .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(target_root.codex_home(id).unwrap().is_dir());
    std::fs::remove_dir(&linked_root).unwrap();
    let _ = std::fs::remove_dir_all(container);
}

#[cfg(windows)]
fn authorized_tombstone_behind_reparse_root(
    label: &str,
) -> (
    PathBuf,
    PathBuf,
    UsageProfileRoot,
    UsageProfileId,
    NativeProfileFileSystem,
    PathBuf,
) {
    let container = test_root(label);
    let root_path = container.join("settings-root");
    let moved_root = container.join("moved-root");
    let root = UsageProfileRoot::new(root_path.clone());
    let id = UsageProfileId::Managed(1);
    let backend = NativeProfileFileSystem::default();
    backend.create_managed_home(&root, id).unwrap();
    let staged = backend.stage_delete(&root, id).unwrap();
    std::fs::rename(&root_path, &moved_root).unwrap();
    create_junction(&root_path, &moved_root);
    (container, root_path, root, id, backend, staged)
}

#[cfg(windows)]
#[test]
fn restore_staged_rejects_a_reparse_point_inserted_in_an_ancestor() {
    let (container, linked_root, root, id, backend, staged) =
        authorized_tombstone_behind_reparse_root("restore-reparse-ancestor");
    let destination = root.codex_home(id).unwrap().parent().unwrap().to_path_buf();

    let error = backend.restore_staged(&staged, &destination).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(staged.is_dir());
    assert!(!destination.exists());
    std::fs::remove_dir(&linked_root).unwrap();
    let _ = std::fs::remove_dir_all(container);
}

#[cfg(windows)]
#[test]
fn remove_staged_rejects_a_reparse_point_inserted_in_an_ancestor() {
    let (container, linked_root, _root, _id, backend, staged) =
        authorized_tombstone_behind_reparse_root("remove-reparse-ancestor");

    let error = backend.remove_staged(&staged).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(staged.is_dir());
    std::fs::remove_dir(&linked_root).unwrap();
    let _ = std::fs::remove_dir_all(container);
}

#[test]
fn native_empty_home_rollback_never_removes_a_nonempty_home() {
    let root_path = test_root("nonempty-rollback");
    let root = UsageProfileRoot::new(root_path.clone());
    let id = UsageProfileId::Managed(1);
    let backend = NativeProfileFileSystem::default();
    backend.create_managed_home(&root, id).unwrap();
    let marker = root.codex_home(id).unwrap().join("preserve.me");
    std::fs::write(&marker, b"marker").unwrap();

    assert!(backend.remove_empty_home(&root, id).is_err());
    assert_eq!(std::fs::read(&marker).unwrap(), b"marker");

    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn failed_final_removal_leaves_only_a_valid_tombstone_for_startup_cleanup() {
    let root_path = test_root("tombstone-retry");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let backend = FailFinalRemovalFileSystem::default();
    backend.create_managed_home(&root, id).unwrap();
    let service = ProfileSettingsService::start(store.clone(), settings, backend.clone());

    service
        .submit(ProfileSettingsMutation::Delete { id })
        .unwrap();
    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Deleted { id: deleted, .. } if deleted == id
    ));
    let staged = backend.staged.lock().unwrap().clone().unwrap();
    assert!(staged.is_dir());
    assert!(staged
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(".deleting-profile-"));
    service.stop().unwrap();

    let profiles = root_path.join("profiles");
    let ordinary = profiles.join("profile-9999");
    let lookalike = profiles.join(".deleting-profile-invalid");
    std::fs::create_dir_all(&ordinary).unwrap();
    std::fs::create_dir_all(&lookalike).unwrap();
    let cleanup_service = ProfileSettingsService::start(
        store,
        Settings::default(),
        NativeProfileFileSystem::default(),
    );
    cleanup_service.flush().unwrap();
    cleanup_service.stop().unwrap();

    assert!(!staged.exists());
    assert!(ordinary.is_dir());
    assert!(lookalike.is_dir());
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn startup_restores_an_interrupted_delete_when_catalog_still_references_profile() {
    let root_path = test_root("interrupted-delete");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let interrupted = NativeProfileFileSystem::default();
    interrupted.create_managed_home(&root, id).unwrap();
    let staged = interrupted.stage_delete(&root, id).unwrap();
    assert!(!root.codex_home(id).unwrap().exists());
    assert!(staged.is_dir());

    let (restarted, startup) = ProfileSettingsService::start_with_recovery(
        store,
        settings,
        NativeProfileFileSystem::default(),
    );
    assert!(root.codex_home(id).unwrap().is_dir());
    assert_eq!(
        startup
            .execution_contexts()
            .iter()
            .map(|context| context.id())
            .collect::<Vec<_>>(),
        [UsageProfileId::System, id]
    );
    restarted.flush().unwrap();
    restarted.stop().unwrap();

    assert!(root.codex_home(id).unwrap().is_dir());
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn startup_recovery_and_validation_finish_before_contexts_are_returned() {
    let root_path = test_root("startup-handshake-order");
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    let blocked = settings.usage_profiles.add("Blocked").unwrap().id();
    let available = settings.usage_profiles.add("Available").unwrap().id();
    store.save(&settings).unwrap();
    let backend = StartupRecordingFileSystem::failing_validation(blocked);

    let (service, startup) =
        ProfileSettingsService::start_with_recovery(store, settings, backend.clone());

    assert_eq!(
        backend.operations(),
        [
            "recover_tombstones",
            "recover_orphans",
            "validate_managed_home",
            "validate_managed_home",
        ]
    );
    assert_eq!(
        startup
            .execution_contexts()
            .iter()
            .map(|context| context.id())
            .collect::<Vec<_>>(),
        [UsageProfileId::System, available]
    );
    assert_eq!(
        startup.report(),
        ProfileSettingsStartupReport {
            configured: 3,
            launchable: 2,
            recovery_failed: false,
            validation_failed: 1,
        }
    );
    let failure = service.wait_for_correlated_event().unwrap();
    assert!(matches!(
        failure,
        RuntimeSettingsEvent::Failed {
            request_id: None,
            operation: ProfileSettingsOperation::StartupValidation,
            kind: io::ErrorKind::InvalidInput,
            ..
        }
    ));
    let debug = format!("{failure:?}");
    assert!(!debug.contains("SENTINEL_PRIVATE_REPARSE_PATH"));
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn startup_recovery_failure_blocks_all_managed_contexts_with_safe_aggregate_status() {
    let root_path = test_root("startup-recovery-failure");
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    settings.usage_profiles.add("Work").unwrap();
    settings.usage_profiles.add("Personal").unwrap();
    store.save(&settings).unwrap();
    let backend = StartupRecordingFileSystem::failing_recovery();

    let (service, startup) =
        ProfileSettingsService::start_with_recovery(store, settings, backend.clone());

    assert_eq!(backend.operations(), ["recover_tombstones"]);
    assert_eq!(
        startup
            .execution_contexts()
            .iter()
            .map(|context| context.id())
            .collect::<Vec<_>>(),
        [UsageProfileId::System]
    );
    assert_eq!(
        startup.report(),
        ProfileSettingsStartupReport {
            configured: 3,
            launchable: 1,
            recovery_failed: true,
            validation_failed: 0,
        }
    );
    let failure = service.wait_for_correlated_event().unwrap();
    assert!(matches!(
        failure,
        RuntimeSettingsEvent::Failed {
            request_id: None,
            operation: ProfileSettingsOperation::Cleanup,
            kind: io::ErrorKind::PermissionDenied,
            ..
        }
    ));
    assert!(!format!("{failure:?}").contains("SENTINEL_PRIVATE_TOMBSTONE_PATH"));
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root_path);
}

#[cfg(windows)]
#[test]
fn startup_reparse_validation_blocks_only_the_managed_context() {
    let root_path = test_root("startup-reparse-validation");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let profile = root.codex_home(id).unwrap().parent().unwrap().to_path_buf();
    let outside = test_root("startup-reparse-target");
    std::fs::create_dir_all(outside.join("codex-home")).unwrap();
    create_junction(&profile, &outside);

    let (service, startup) = ProfileSettingsService::start_with_recovery(
        store,
        settings,
        NativeProfileFileSystem::default(),
    );

    assert_eq!(
        startup
            .execution_contexts()
            .iter()
            .map(|context| context.id())
            .collect::<Vec<_>>(),
        [UsageProfileId::System]
    );
    assert_eq!(startup.report().validation_failed, 1);
    service.stop().unwrap();
    std::fs::remove_dir(&profile).unwrap();
    let _ = std::fs::remove_dir_all(root_path);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn startup_preserves_tombstone_when_referenced_destination_already_exists() {
    let root_path = test_root("interrupted-delete-conflict");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let interrupted = NativeProfileFileSystem::default();
    interrupted.create_managed_home(&root, id).unwrap();
    let staged = interrupted.stage_delete(&root, id).unwrap();
    NativeProfileFileSystem::default()
        .create_managed_home(&root, id)
        .unwrap();

    let restarted =
        ProfileSettingsService::start(store, settings, NativeProfileFileSystem::default());
    restarted.flush().unwrap();
    restarted.stop().unwrap();

    assert!(root.codex_home(id).unwrap().is_dir());
    assert!(staged.is_dir());
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn startup_removes_only_an_empty_orphaned_home_from_interrupted_add() {
    let root_path = test_root("interrupted-add");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let settings = Settings::default();
    store.save(&settings).unwrap();
    let orphan_id = UsageProfileId::Managed(1);
    NativeProfileFileSystem::default()
        .create_managed_home(&root, orphan_id)
        .unwrap();

    let restarted =
        ProfileSettingsService::start(store, settings, NativeProfileFileSystem::default());
    restarted.flush().unwrap();
    restarted.stop().unwrap();

    assert!(!root.codex_home(orphan_id).unwrap().exists());
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn startup_removes_partial_empty_orphan_only_at_the_next_catalog_sequence() {
    let root_path = test_root("partial-interrupted-add");
    let root = UsageProfileRoot::new(root_path.clone());
    let store = SettingsStore::for_root(&root_path);
    let settings = Settings::default();
    store.save(&settings).unwrap();
    let pending_profile = root
        .codex_home(UsageProfileId::Managed(1))
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let unrelated = root_path.join("profiles").join("profile-9999");
    std::fs::create_dir_all(&pending_profile).unwrap();
    std::fs::create_dir_all(&unrelated).unwrap();

    let restarted =
        ProfileSettingsService::start(store, settings, NativeProfileFileSystem::default());
    restarted.flush().unwrap();
    restarted.stop().unwrap();

    assert!(!pending_profile.exists());
    assert!(unrelated.is_dir());
    let _ = std::fs::remove_dir_all(root_path);
}

#[test]
fn profile_settings_rename_and_select_persist_authoritative_settings() {
    let root = test_root("rename-select");
    let store = SettingsStore::for_root(&root);
    let mut settings = Settings::default();
    let id = settings.usage_profiles.add("Work").unwrap().id();
    store.save(&settings).unwrap();
    let backend = RecordingProfileFileSystem::new(store.clone());
    let service = ProfileSettingsService::start(store.clone(), settings, backend);

    service
        .submit(ProfileSettingsMutation::Rename {
            id,
            label: "Office".to_owned(),
        })
        .unwrap();
    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Renamed { id: renamed, .. } if renamed == id
    ));
    assert_eq!(
        store.load().unwrap().usage_profiles.managed()[0].label(),
        "Office"
    );

    service
        .submit(ProfileSettingsMutation::Select { id })
        .unwrap();
    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Selected { id: selected, .. } if selected == id
    ));
    assert_eq!(store.load().unwrap().usage_profiles.selected(), id);
    service.stop().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn profile_labels_are_trimmed_bounded_and_case_insensitively_unique() {
    assert_eq!(normalize_profile_label("  개인  ").unwrap(), "개인");
    assert_eq!(
        normalize_profile_label(".."),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label("bad/name"),
        Err(ProfileValidationError::InvalidLabel)
    );

    let mut catalog = UsageProfileCatalog::default();
    assert_eq!(
        catalog.add("Work").unwrap().id(),
        UsageProfileId::Managed(1)
    );
    assert_eq!(
        catalog.add(" work "),
        Err(ProfileValidationError::DuplicateLabel)
    );
}

#[test]
fn system_profile_name_is_optional_and_can_be_renamed_without_changing_identity() {
    let mut catalog = UsageProfileCatalog::default();
    assert_eq!(catalog.system_label(), None);

    catalog
        .rename(UsageProfileId::System, "  Main account  ")
        .unwrap();

    assert_eq!(catalog.system_label(), Some("Main account"));
    assert_eq!(catalog.selected(), UsageProfileId::System);
    assert_eq!(
        catalog.remove(UsageProfileId::System),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
}

#[test]
fn custom_system_name_and_managed_names_are_unique_case_insensitively() {
    let mut catalog = UsageProfileCatalog::default();
    catalog.rename(UsageProfileId::System, "Main").unwrap();
    assert_eq!(
        catalog.add("main"),
        Err(ProfileValidationError::DuplicateLabel)
    );

    catalog.add("Work").unwrap();
    assert_eq!(
        catalog.rename(UsageProfileId::System, "WORK"),
        Err(ProfileValidationError::DuplicateLabel)
    );
}

#[test]
fn profile_labels_allow_embedded_periods_but_reject_dot_components() {
    assert_eq!(normalize_profile_label("Work 2.0").unwrap(), "Work 2.0");
    assert_eq!(
        normalize_profile_label("."),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label(" .. "),
        Err(ProfileValidationError::InvalidLabel)
    );
}

#[test]
fn managed_paths_are_derived_only_from_numeric_ids() {
    let root = UsageProfileRoot::new(std::path::PathBuf::from(r"C:\safe\appdata"));
    assert_eq!(
        root.codex_home(UsageProfileId::Managed(7)).unwrap(),
        std::path::PathBuf::from(r"C:\safe\appdata\profiles\profile-0007\codex-home")
    );
    assert!(root.codex_home(UsageProfileId::System).is_err());
}

#[test]
fn catalog_rejects_an_eighth_managed_profile() {
    let mut catalog = UsageProfileCatalog::default();

    for sequence in 1..MAX_USAGE_PROFILES {
        catalog.add(&format!("Profile {sequence}")).unwrap();
    }

    assert_eq!(
        catalog.add("Overflow"),
        Err(ProfileValidationError::TooManyProfiles)
    );
}

#[test]
fn catalog_maintains_selection_and_rejects_system_deletion() {
    let mut catalog = UsageProfileCatalog::default();
    let profile_id = catalog.add("Work").unwrap().id();

    catalog.select(profile_id).unwrap();
    catalog.rename(profile_id, "Office").unwrap();
    assert_eq!(catalog.managed()[0].label(), "Office");

    catalog.remove(profile_id).unwrap();
    assert_eq!(catalog.selected(), UsageProfileId::System);
    assert_eq!(
        catalog.remove(UsageProfileId::System),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
}

#[test]
fn labels_reject_invalid_unicode_scalar_counts_and_control_characters() {
    assert_eq!(
        normalize_profile_label("\nwork"),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label(" "),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        normalize_profile_label(&"a".repeat(41)),
        Err(ProfileValidationError::InvalidLabel)
    );
}

#[test]
fn catalog_validation_rejects_noncanonical_labels_and_wrapped_sequences() {
    let noncanonical: UsageProfileCatalog = serde_json::from_str(
        r#"{"managed":[{"sequence":1,"label":" Work "}],"selected":{"managed":1},"next_sequence":2}"#,
    )
    .unwrap();
    let wrapped_sequence: UsageProfileCatalog =
        serde_json::from_str(r#"{"managed":[],"selected":"system","next_sequence":0}"#).unwrap();

    assert_eq!(
        noncanonical.validate(),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        wrapped_sequence.validate(),
        Err(ProfileValidationError::InvalidId)
    );
}

#[test]
fn catalog_validation_rejects_invalid_or_duplicate_system_labels() {
    let noncanonical: UsageProfileCatalog = serde_json::from_str(
        r#"{"system_label":" Work ","managed":[],"selected":"system","next_sequence":1}"#,
    )
    .unwrap();
    let duplicate: UsageProfileCatalog = serde_json::from_str(
        r#"{"system_label":"WORK","managed":[{"sequence":1,"label":"Work"}],"selected":"system","next_sequence":2}"#,
    )
    .unwrap();

    assert_eq!(
        noncanonical.validate(),
        Err(ProfileValidationError::InvalidLabel)
    );
    assert_eq!(
        duplicate.validate(),
        Err(ProfileValidationError::DuplicateLabel)
    );
}
