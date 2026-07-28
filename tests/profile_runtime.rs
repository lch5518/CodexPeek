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

    fn cleanup_staged(&self, _root: &UsageProfileRoot) -> io::Result<()> {
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
    service.stop().unwrap();
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
    service.stop().unwrap();
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

    fn cleanup_staged(&self, root: &UsageProfileRoot) -> io::Result<()> {
        self.native.cleanup_staged(root)
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
