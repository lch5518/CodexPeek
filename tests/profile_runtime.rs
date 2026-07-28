use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    normalize_profile_label, NativeProfileFileSystem, ProfileFileSystem, ProfileSettingsEvent,
    ProfileSettingsMutation, ProfileSettingsService, ProfileValidationError, Settings,
    SettingsStore, UsageProfileCatalog, UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
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

fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codex-peek-profile-{label}-{nonce}"))
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

    let restarted =
        ProfileSettingsService::start(store, settings, NativeProfileFileSystem::default());
    restarted.flush().unwrap();
    restarted.stop().unwrap();

    assert!(root.codex_home(id).unwrap().is_dir());
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(root_path);
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
fn catalog_maintains_selection_and_rejects_system_mutations() {
    let mut catalog = UsageProfileCatalog::default();
    let profile_id = catalog.add("Work").unwrap().id();

    catalog.select(profile_id).unwrap();
    catalog.rename(profile_id, "Office").unwrap();
    assert_eq!(catalog.managed()[0].label(), "Office");

    catalog.remove(profile_id).unwrap();
    assert_eq!(catalog.selected(), UsageProfileId::System);
    assert_eq!(
        catalog.rename(UsageProfileId::System, "Changed"),
        Err(ProfileValidationError::SystemProfileImmutable)
    );
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
