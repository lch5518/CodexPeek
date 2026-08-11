use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    LanguagePreference, NativeProfileFileSystem, ProfileSettingsEvent, ProfileSettingsMutation,
    ProfileSettingsService, Settings, SettingsStore, StartupView, TaskbarDisplayMode,
    UsageProfileId,
};

fn test_root(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codex-peek-{label}-{nonce}"))
}

fn valid_schema_v1_json() -> &'static [u8] {
    br#"{
  "schema_version": 1,
  "refresh_interval_minutes": 15,
  "widget_visible": false,
  "taskbar_offset": 24,
  "taskbar_display_mode": "primary",
  "start_with_windows": true,
  "startup_view": "tray_only",
  "auto_auth_refresh": false,
  "language": "korean",
  "last_update_check_unix": 1234,
  "show_remaining_percent": true
}"#
}

#[test]
fn schema_v1_migrates_to_system_profile_and_is_persisted_as_v2() {
    let root = test_root("profile-migration");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("settings.json"), valid_schema_v1_json()).unwrap();
    let store = SettingsStore::for_root(root.clone());

    assert_eq!(store.root(), root.as_path());
    let loaded = store.load().unwrap();

    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.usage_profiles.selected(), UsageProfileId::System);
    assert_eq!(loaded.refresh_interval_minutes, 15);
    assert!(loaded.usage_forecast_enabled);
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("settings.json")).unwrap()).unwrap();
    assert_eq!(persisted["schema_version"], 2);
    assert!(persisted.get("usage_profiles").is_some());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn system_profile_label_defaults_for_v2_and_round_trips() {
    let root = test_root("system-profile-label");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        br#"{
  "schema_version": 2,
  "refresh_interval_minutes": 5,
  "widget_visible": true,
  "taskbar_offset": 0,
  "taskbar_display_mode": "all",
  "start_with_windows": false,
  "startup_view": "widget",
  "auto_auth_refresh": true,
  "language": "auto",
  "last_update_check_unix": null,
  "show_remaining_percent": false,
  "usage_profiles": {
    "managed": [],
    "selected": "system",
    "next_sequence": 1
  }
}"#,
    )
    .unwrap();
    let store = SettingsStore::for_root(&root);
    let mut settings = store.load().unwrap();

    assert_eq!(settings.usage_profiles.system_label(), None);
    settings
        .usage_profiles
        .rename(UsageProfileId::System, "Main")
        .unwrap();
    store.save(&settings).unwrap();

    assert_eq!(
        store.load().unwrap().usage_profiles.system_label(),
        Some("Main")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn forecast_setting_defaults_to_enabled_when_omitted_from_v2() {
    let root = test_root("forecast-default-v2");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        br#"{
  "schema_version": 2,
  "refresh_interval_minutes": 5,
  "widget_visible": true,
  "taskbar_offset": 0,
  "start_with_windows": false,
  "startup_view": "widget",
  "auto_auth_refresh": true,
  "last_update_check_unix": null,
  "usage_profiles": {
    "managed": [],
    "selected": "system",
    "next_sequence": 1
  }
}"#,
    )
    .unwrap();

    let loaded = SettingsStore::for_root(&root).load().unwrap();
    assert!(loaded.usage_forecast_enabled);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_usage_profile_catalogs_are_backed_up_and_reset() {
    let cases = [
        (
            "duplicate-sequence",
            r#"{"managed":[{"sequence":1,"label":"Work"},{"sequence":1,"label":"Personal"}],"selected":"system","next_sequence":2}"#,
        ),
        (
            "missing-selected",
            r#"{"managed":[],"selected":{"managed":1},"next_sequence":2}"#,
        ),
        (
            "non-increasing-next-sequence",
            r#"{"managed":[{"sequence":1,"label":"Work"}],"selected":"system","next_sequence":1}"#,
        ),
        (
            "profile-overflow",
            r#"{"managed":[{"sequence":1,"label":"One"},{"sequence":2,"label":"Two"},{"sequence":3,"label":"Three"},{"sequence":4,"label":"Four"},{"sequence":5,"label":"Five"},{"sequence":6,"label":"Six"},{"sequence":7,"label":"Seven"},{"sequence":8,"label":"Eight"}],"selected":"system","next_sequence":9}"#,
        ),
    ];

    for (label, usage_profiles) in cases {
        let root = test_root(label);
        fs::create_dir_all(&root).unwrap();
        let mut settings = serde_json::to_value(Settings::default()).unwrap();
        settings["usage_profiles"] = serde_json::from_str(usage_profiles).unwrap();
        let bytes = serde_json::to_vec(&settings).unwrap();
        fs::write(root.join("settings.json"), &bytes).unwrap();
        let store = SettingsStore::for_root(&root);

        assert_eq!(store.load().unwrap(), Settings::default(), "{label}");
        let backup = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), bytes, "{label}");

        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn schema_v3_is_backed_up_and_reset_to_defaults() {
    let root = test_root("schema-v3");
    fs::create_dir_all(&root).unwrap();
    let mut settings = serde_json::to_value(Settings::default()).unwrap();
    settings["schema_version"] = serde_json::json!(3);
    let bytes = serde_json::to_vec(&settings).unwrap();
    fs::write(root.join("settings.json"), &bytes).unwrap();
    let store = SettingsStore::for_root(&root);

    assert_eq!(store.load().unwrap(), Settings::default());
    let backup = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.corrupt-")
        })
        .unwrap();
    assert_eq!(fs::read(backup).unwrap(), bytes);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn profile_settings_writer_preserves_preference_submission_order() {
    let root = test_root("async-ordered");
    let store = SettingsStore::for_root(&root);
    let writer = ProfileSettingsService::start(
        store.clone(),
        Settings::default(),
        NativeProfileFileSystem::default(),
    );
    for offset in [10, 20, 30] {
        writer
            .save_preferences(Settings {
                taskbar_offset: offset,
                ..Settings::default()
            })
            .unwrap();
    }
    writer.flush().unwrap();
    assert_eq!(store.load().unwrap().taskbar_offset, 30);
    writer.stop().unwrap();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn profile_settings_preference_write_preserves_newer_profile_catalog() {
    let root = test_root("profile-settings-preference-order");
    let store = SettingsStore::for_root(&root);
    let original = Settings::default();
    let service = ProfileSettingsService::start(
        store.clone(),
        original.clone(),
        NativeProfileFileSystem::default(),
    );
    service
        .submit(ProfileSettingsMutation::Add {
            label: "Work".to_owned(),
        })
        .unwrap();
    assert!(matches!(
        service.wait_for_event().unwrap(),
        ProfileSettingsEvent::Added { .. }
    ));

    service
        .save_preferences(Settings {
            taskbar_offset: 42,
            ..original
        })
        .unwrap();
    service.flush().unwrap();

    let saved = store.load().unwrap();
    assert_eq!(saved.taskbar_offset, 42);
    assert_eq!(saved.usage_profiles.managed().len(), 1);
    assert_eq!(saved.usage_profiles.managed()[0].label(), "Work");
    service.stop().unwrap();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn settings_defaults_match_product_policy() {
    let settings = Settings::default();
    assert_eq!(settings.schema_version, 2);
    assert_eq!(settings.usage_profiles.selected(), UsageProfileId::System);
    assert_eq!(settings.refresh_interval_minutes, 5);
    assert!(settings.widget_visible);
    assert_eq!(settings.taskbar_offset, 0);
    assert_eq!(settings.taskbar_display_mode, TaskbarDisplayMode::All);
    assert!(!settings.start_with_windows);
    assert_eq!(settings.startup_view, StartupView::Widget);
    assert!(settings.auto_auth_refresh);
    assert_eq!(settings.language, LanguagePreference::Auto);
    assert_eq!(settings.last_update_check_unix, None);
    assert!(settings.show_remaining_percent);
    assert_eq!(settings.dismissed_update_version, None);
    assert_eq!(settings.unofficial_build_warning_version, None);
}

#[test]
fn update_prompt_versions_default_for_existing_v2_and_round_trip() {
    let root = test_root("dismissed-update-version");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        br#"{
  "schema_version": 2,
  "refresh_interval_minutes": 5,
  "widget_visible": true,
  "taskbar_offset": 0,
  "taskbar_display_mode": "all",
  "start_with_windows": false,
  "startup_view": "widget",
  "auto_auth_refresh": true,
  "language": "auto",
  "last_update_check_unix": null,
  "show_remaining_percent": false,
  "usage_profiles": {
    "managed": [],
    "selected": "system",
    "next_sequence": 1
  }
}"#,
    )
    .unwrap();
    let store = SettingsStore::for_root(&root);
    let mut settings = store.load().unwrap();

    assert_eq!(settings.dismissed_update_version, None);
    assert_eq!(settings.unofficial_build_warning_version, None);
    settings.dismissed_update_version = Some("2.0.0".to_owned());
    settings.unofficial_build_warning_version = Some("1.9.0".to_owned());
    store.save(&settings).unwrap();

    assert_eq!(
        store.load().unwrap().dismissed_update_version.as_deref(),
        Some("2.0.0")
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(persisted["schema_version"], 2);
    assert_eq!(persisted["dismissed_update_version"], "2.0.0");
    assert_eq!(persisted["unofficial_build_warning_version"], "1.9.0");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn language_preferences_round_trip_all_persisted_variants() {
    let cases = [
        (LanguagePreference::Auto, "auto"),
        (LanguagePreference::Korean, "korean"),
        (LanguagePreference::English, "english"),
        (LanguagePreference::Spanish, "spanish"),
        (LanguagePreference::PortugueseBrazil, "portuguese_brazil"),
        (LanguagePreference::Indonesian, "indonesian"),
        (LanguagePreference::Japanese, "japanese"),
        (LanguagePreference::Hindi, "hindi"),
        (LanguagePreference::German, "german"),
        (LanguagePreference::French, "french"),
        (LanguagePreference::Vietnamese, "vietnamese"),
        (LanguagePreference::Turkish, "turkish"),
        (LanguagePreference::Arabic, "arabic"),
    ];
    for (preference, persisted) in cases {
        let root = test_root(persisted);
        let store = SettingsStore::for_root(&root);
        let settings = Settings {
            language: preference,
            ..Settings::default()
        };

        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap().language, preference);
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["language"], persisted);

        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn settings_without_show_remaining_field_loads_with_default() {
    let root = test_root("missing-show-remaining");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{
  "schema_version": 1,
  "refresh_interval_minutes": 5,
  "widget_visible": true,
  "taskbar_offset": 0,
  "start_with_windows": false,
  "startup_view": "widget",
  "auto_auth_refresh": true,
  "language": "auto",
  "last_update_check_unix": null
}"#,
    )
    .unwrap();
    let store = SettingsStore::for_root(&root);

    assert!(store.inspect_validity().unwrap());
    let settings = store.load().unwrap();
    assert!(settings.show_remaining_percent);
    assert_eq!(settings.taskbar_display_mode, TaskbarDisplayMode::All);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn settings_without_language_preserve_existing_preferences() {
    let root = test_root("missing-language");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{
  "schema_version": 1,
  "refresh_interval_minutes": 15,
  "widget_visible": false,
  "taskbar_offset": 24,
  "taskbar_display_mode": "primary",
  "start_with_windows": true,
  "startup_view": "tray_only",
  "auto_auth_refresh": false,
  "last_update_check_unix": 1234,
  "show_remaining_percent": true
}"#,
    )
    .unwrap();
    let store = SettingsStore::for_root(&root);

    assert!(store.inspect_validity().unwrap());
    let settings = store.load().unwrap();

    assert_eq!(settings.language, LanguagePreference::Auto);
    assert_eq!(settings.refresh_interval_minutes, 15);
    assert!(!settings.widget_visible);
    assert_eq!(settings.taskbar_offset, 24);
    assert_eq!(settings.taskbar_display_mode, TaskbarDisplayMode::Primary);
    assert!(settings.start_with_windows);
    assert_eq!(settings.startup_view, StartupView::TrayOnly);
    assert!(!settings.auto_auth_refresh);
    assert_eq!(settings.last_update_check_unix, Some(1234));
    assert!(settings.show_remaining_percent);
    assert!(store.path().exists());
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("settings.corrupt-")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_language_is_backed_up_and_resets_to_defaults() {
    let root = test_root("invalid-language");
    fs::create_dir_all(&root).unwrap();
    let invalid = r#"{
  "schema_version": 1,
  "refresh_interval_minutes": 5,
  "widget_visible": true,
  "taskbar_offset": 0,
  "taskbar_display_mode": "all",
  "start_with_windows": false,
  "startup_view": "widget",
  "auto_auth_refresh": true,
  "language": "unsupported",
  "last_update_check_unix": null,
  "show_remaining_percent": false
}"#;
    fs::write(root.join("settings.json"), invalid).unwrap();
    let store = SettingsStore::for_root(&root);

    assert_eq!(store.load().unwrap(), Settings::default());
    assert!(!store.path().exists());
    let backup = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.corrupt-")
        })
        .unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), invalid);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn diagnostic_inspection_treats_missing_settings_as_valid_defaults() {
    let root = test_root("inspect-missing");
    let store = SettingsStore::for_root(&root);

    assert!(store.inspect_validity().unwrap());
    assert!(!root.exists());
}

#[test]
fn diagnostic_inspection_reports_valid_and_invalid_settings_without_mutation() {
    let root = test_root("inspect-validity");
    let store = SettingsStore::for_root(&root);
    store.save(&Settings::default()).unwrap();
    assert!(store.inspect_validity().unwrap());

    let cases = [
        b"not json".to_vec(),
        serde_json::to_vec(&Settings {
            schema_version: 3,
            ..Settings::default()
        })
        .unwrap(),
        serde_json::to_vec(&Settings {
            refresh_interval_minutes: 2,
            ..Settings::default()
        })
        .unwrap(),
    ];
    for bytes in cases {
        fs::write(store.path(), &bytes).unwrap();

        assert!(!store.inspect_validity().unwrap());
        assert_eq!(fs::read(store.path()).unwrap(), bytes);
        assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("settings.corrupt-")));
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn diagnostic_inspection_propagates_settings_read_errors() {
    let root = test_root("inspect-read-error");
    let store = SettingsStore::for_root(&root);
    fs::create_dir_all(store.path()).unwrap();

    assert!(store.inspect_validity().is_err());
    assert!(store.path().is_dir());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn negative_taskbar_offset_is_rejected() {
    let root = test_root("negative-taskbar-offset");
    let store = SettingsStore::for_root(&root);
    let result = store.save(&Settings {
        taskbar_offset: -1,
        ..Settings::default()
    });

    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn taskbar_display_mode_round_trips() {
    let root = test_root("taskbar-display-mode");
    let store = SettingsStore::for_root(&root);
    let selected = Settings {
        taskbar_display_mode: TaskbarDisplayMode::Primary,
        ..Settings::default()
    };
    store.save(&selected).unwrap();
    assert_eq!(
        store.load().unwrap().taskbar_display_mode,
        TaskbarDisplayMode::Primary
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn settings_round_trip_and_no_temporary_file_remains() {
    let root = test_root("round-trip");
    let store = SettingsStore::for_root(&root);
    let settings = Settings {
        language: LanguagePreference::Korean,
        refresh_interval_minutes: 30,
        ..Settings::default()
    };

    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), settings);
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".tmp-")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_settings_are_backed_up_and_reset_to_defaults() {
    let root = test_root("corrupt");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("settings.json"),
        r#"{"schema_version":1,"refresh_interval_minutes":2}"#,
    )
    .unwrap();
    let store = SettingsStore::for_root(&root);

    assert_eq!(store.load().unwrap(), Settings::default());
    assert!(!store.path().exists());
    let backup = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.corrupt-")
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(backup).unwrap(),
        r#"{"schema_version":1,"refresh_interval_minutes":2}"#
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unsupported_schema_is_rejected() {
    let root = test_root("validation");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("settings.json"), r#"{"schema_version":2}"#).unwrap();
    let store = SettingsStore::for_root(&root);
    assert_eq!(store.load().unwrap(), Settings::default());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_floating_settings_are_ignored_when_loading() {
    let root = test_root("legacy-floating");
    fs::create_dir_all(&root).unwrap();
    let store = SettingsStore::for_root(&root);
    let mut legacy = serde_json::to_value(Settings {
        refresh_interval_minutes: 10,
        ..Settings::default()
    })
    .unwrap();
    legacy["display_mode"] = serde_json::json!("Floating");
    legacy["floating_position"] = serde_json::json!({"x": 123, "y": -456});
    legacy["always_on_top"] = serde_json::json!(true);
    legacy["monitor_device"] = serde_json::json!("display-1");
    fs::write(store.path(), serde_json::to_vec(&legacy).unwrap()).unwrap();

    let settings = store.load().unwrap();
    assert_eq!(settings.refresh_interval_minutes, 10);
    assert_eq!(settings.taskbar_offset, 0);
    store.save(&settings).unwrap();
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    for field in [
        "display_mode",
        "floating_position",
        "always_on_top",
        "monitor_device",
    ] {
        assert!(persisted.get(field).is_none(), "{field}");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn checked_load_preserves_each_corrupt_file_with_unique_backup() {
    let root = test_root("unique-corrupt");
    fs::create_dir_all(&root).unwrap();
    let store = SettingsStore::for_root(&root);
    fs::write(store.path(), b"first").unwrap();
    assert_eq!(store.load().unwrap(), Settings::default());
    fs::write(store.path(), b"second").unwrap();
    assert_eq!(store.load().unwrap(), Settings::default());
    let backups: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("settings.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 2);
    let pid_marker = format!("-{}-", std::process::id());
    assert!(backups.iter().all(|backup| {
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&pid_marker)
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_load_does_not_create_root_and_replacement_keeps_latest_settings() {
    let root = test_root("missing-replace");
    let store = SettingsStore::for_root(&root);
    assert_eq!(store.load().unwrap(), Settings::default());
    assert!(!root.exists());
    let first = Settings {
        taskbar_offset: 1,
        ..Settings::default()
    };
    let second = Settings {
        taskbar_offset: 2,
        ..Settings::default()
    };
    store.save(&first).unwrap();
    store.save(&second).unwrap();
    assert_eq!(store.load().unwrap(), second);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn concurrent_saves_leave_valid_final_json_without_temp_files() {
    let root = test_root("concurrent");
    let store = SettingsStore::for_root(&root);
    let mut joins = Vec::new();
    for offset in 0..8 {
        let store = store.clone();
        joins.push(std::thread::spawn(move || {
            store.save(&Settings {
                taskbar_offset: offset,
                ..Settings::default()
            })
        }));
    }
    for join in joins {
        join.join().unwrap().unwrap();
    }
    let _: Settings = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".settings.tmp-")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn complete_json_field_mutations_are_backed_up_exactly() {
    let cases = vec![
        ("schema", serde_json::json!(3)),
        ("interval", serde_json::json!(2)),
        ("offset", serde_json::json!(2_000_001)),
    ];
    for (name, value) in cases {
        let root = test_root(name);
        fs::create_dir_all(&root).unwrap();
        let store = SettingsStore::for_root(&root);
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        match name {
            "schema" => json["schema_version"] = value,
            "interval" => json["refresh_interval_minutes"] = value,
            _ => json["taskbar_offset"] = value,
        }
        let bytes = serde_json::to_vec(&json).unwrap();
        fs::write(store.path(), &bytes).unwrap();
        assert_eq!(store.load().unwrap(), Settings::default());
        let backup = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            })
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), bytes);
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn separately_constructed_stores_do_not_back_up_a_newly_saved_settings_file() {
    let root = test_root("load-save-race");
    fs::create_dir_all(&root).unwrap();
    let reader = SettingsStore::for_root(&root);
    let writer = SettingsStore::for_root(&root);
    let saved = Settings {
        taskbar_offset: 777,
        ..Settings::default()
    };

    for _ in 0..200 {
        fs::write(reader.path(), "{".repeat(256 * 1024)).unwrap();
        let reader_store = reader.clone();
        let reader_thread = std::thread::spawn(move || reader_store.load());
        writer.save(&saved).unwrap();
        let _ = reader_thread.join().unwrap();
        assert_eq!(writer.load().unwrap(), saved);
    }

    assert!(fs::read_dir(&root).unwrap().all(|entry| {
        let path = entry.unwrap().path();
        !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("settings.corrupt")
            || !fs::read_to_string(path).unwrap_or_default().contains("777")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stores_created_before_and_after_root_creation_share_load_save_gate() {
    let root = test_root("creation-order-gate");
    let first = SettingsStore::for_root(&root);
    let initial = Settings {
        taskbar_offset: 1,
        ..Settings::default()
    };
    first.save(&initial).unwrap();
    let second = SettingsStore::for_root(&root);
    let saved = Settings {
        taskbar_offset: 888,
        ..Settings::default()
    };

    for _ in 0..200 {
        fs::write(first.path(), "{".repeat(256 * 1024)).unwrap();
        let reader = first.clone();
        let reader_thread = std::thread::spawn(move || reader.load());
        second.save(&saved).unwrap();
        let _ = reader_thread.join().unwrap();
        assert_eq!(second.load().unwrap(), saved);
    }

    assert!(fs::read_dir(&root).unwrap().all(|entry| {
        let path = entry.unwrap().path();
        !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("settings.corrupt")
            || !fs::read_to_string(path).unwrap_or_default().contains("888")
    }));
    let _ = fs::remove_dir_all(root);
}
