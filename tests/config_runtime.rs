use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    DisplayMode, LanguagePreference, LogicalPosition, Settings, SettingsStore, StartupView,
};

fn test_root(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("codex-usage-monitor-{label}-{nonce}"))
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
    assert_eq!(store.load(), settings);
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

    assert_eq!(store.load(), Settings::default());
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
    assert_eq!(store.load(), Settings::default());

    let invalid = Settings {
        floating_position: Some(LogicalPosition { x: 2_000_001, y: 0 }),
        ..Settings::default()
    };
    assert!(store.save(&invalid).is_err());
    let _ = fs::remove_dir_all(root);
}
