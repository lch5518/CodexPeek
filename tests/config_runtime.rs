use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    AsyncSettingsWriter, DisplayMode, LanguagePreference, LogicalPosition, Settings, SettingsStore,
    StartupView,
};

fn test_root(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codex-usage-monitor-{label}-{nonce}"))
}

#[test]
fn asynchronous_settings_writer_preserves_submission_order() {
    let root = test_root("async-ordered");
    let store = SettingsStore::for_root(&root);
    let writer = AsyncSettingsWriter::start(store.clone());
    for offset in [10, 20, 30] {
        writer
            .save(Settings {
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
fn settings_defaults_match_product_policy() {
    let settings = Settings::default();
    assert_eq!(settings.schema_version, 1);
    assert_eq!(settings.refresh_interval_minutes, 5);
    assert_eq!(settings.display_mode, DisplayMode::Taskbar);
    assert!(settings.widget_visible);
    assert_eq!(settings.taskbar_offset, 0);
    assert_eq!(settings.monitor_device, None);
    assert_eq!(settings.floating_position, None);
    assert!(settings.always_on_top);
    assert!(!settings.start_with_windows);
    assert_eq!(settings.startup_view, StartupView::Widget);
    assert!(settings.auto_auth_refresh);
    assert_eq!(settings.language, LanguagePreference::Auto);
    assert_eq!(settings.last_update_check_unix, None);
}

#[test]
fn settings_round_trip_and_no_temporary_file_remains() {
    let root = test_root("round-trip");
    let store = SettingsStore::for_root(&root);
    let settings = Settings {
        display_mode: DisplayMode::Floating,
        floating_position: Some(LogicalPosition { x: 123, y: -456 }),
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
fn unsupported_schema_and_unreasonable_coordinates_are_rejected() {
    let root = test_root("validation");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("settings.json"), r#"{"schema_version":2}"#).unwrap();
    let store = SettingsStore::for_root(&root);
    assert_eq!(store.load().unwrap(), Settings::default());

    let invalid = Settings {
        floating_position: Some(LogicalPosition { x: 2_000_001, y: 0 }),
        ..Settings::default()
    };
    assert!(store.save(&invalid).is_err());
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
        ("schema", serde_json::json!(2)),
        ("interval", serde_json::json!(2)),
        ("monitor_empty", serde_json::json!("")),
        ("monitor_whitespace", serde_json::json!("   \t")),
        ("monitor_long", serde_json::json!("x".repeat(513))),
        ("monitor_control", serde_json::json!("a\nb")),
        ("offset", serde_json::json!(2_000_001)),
        ("position", serde_json::json!({"x": 2_000_001, "y": 0})),
    ];
    for (name, value) in cases {
        let root = test_root(name);
        fs::create_dir_all(&root).unwrap();
        let store = SettingsStore::for_root(&root);
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        match name {
            "schema" => json["schema_version"] = value,
            "interval" => json["refresh_interval_minutes"] = value,
            name if name.starts_with("monitor") => json["monitor_device"] = value,
            "position" => json["floating_position"] = value,
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
