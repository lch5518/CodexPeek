use std::{
    fs,
    sync::{Arc, Barrier},
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    aggregate_profile_diagnostics, inspect_settings_for_diagnostics, AsyncDiagnosticWriter,
    DiagnosticLogger, PollSnapshot, ProfileDiagnosticSnapshot, SafeDiagnostic, Settings,
    SettingsStore, UsageError, UsageProfileId, UsageProfileRoot,
};

fn temp_log() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "diagnostic-{}.log",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn diagnostics_mask_secrets_and_keep_one_line_records() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);
    logger
        .record_safe(SafeDiagnostic::Proxy { present: true })
        .unwrap();
    let line = fs::read_to_string(&path).unwrap();
    assert_eq!(line.lines().count(), 1);
    let _ = fs::remove_file(path);
}

#[test]
fn profile_diagnostics_are_aggregate_only_and_clamped_to_supported_count() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);

    logger
        .record_safe(SafeDiagnostic::Profiles {
            settings_valid: true,
            configured: u8::MAX,
            ok: 17,
            login_required: 9,
            request_failed: 11,
        })
        .unwrap();

    let line = fs::read_to_string(&path).unwrap();
    assert!(line.contains("settings_valid=true"));
    assert!(line.contains("configured=8"));
    assert!(line.contains("ok=8"));
    assert!(line.contains("login_required=8"));
    assert!(line.contains("request_failed=8"));
    let _ = fs::remove_file(path);
}

#[test]
fn profile_diagnostics_never_serialize_labels_or_managed_paths() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);
    let fixture_label = "SENTINEL_PRIVATE_PROFILE_LABEL";
    let root_path = std::env::temp_dir().join("SENTINEL_PRIVATE_MANAGED_PATH");
    let root = UsageProfileRoot::new(root_path.clone());
    let mut settings = Settings::default();
    let managed_id = settings.usage_profiles.add(fixture_label).unwrap().id();
    let managed_snapshot = PollSnapshot {
        last_error: Some(UsageError::NotLoggedIn),
        ..PollSnapshot::default()
    };
    let inputs = [
        ProfileDiagnosticSnapshot {
            id: UsageProfileId::System,
            snapshot: Some(PollSnapshot::default()),
            login_required: false,
        },
        ProfileDiagnosticSnapshot {
            id: managed_id,
            snapshot: Some(managed_snapshot),
            login_required: false,
        },
    ];
    let event = aggregate_profile_diagnostics(true, &settings, &root, &inputs);
    let writer = AsyncDiagnosticWriter::start(logger, 4);
    assert!(writer.enqueue(event));
    writer.stop().unwrap();

    let serialized = fs::read_to_string(&path).unwrap();
    assert!(serialized.contains("configured=2"));
    assert!(serialized.contains("login_required=1"));
    assert!(!serialized.contains(fixture_label));
    assert!(!serialized.contains(root_path.to_string_lossy().as_ref()));
    assert!(!serialized.to_ascii_lowercase().contains("profile-0001"));
    let _ = fs::remove_file(path);
}

#[test]
fn settings_diagnostics_report_invalid_without_repairing_the_file() {
    let root = std::env::temp_dir().join(format!(
        "diagnostic-settings-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let store = SettingsStore::for_root(&root);
    let bytes = b"{broken-json";
    fs::write(store.path(), bytes).unwrap();
    let log_path = temp_log();
    let logger = DiagnosticLogger::for_path(&log_path);

    assert!(!inspect_settings_for_diagnostics(&store, &logger).unwrap());
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
    assert!(fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("settings.corrupt-")));
    assert!(fs::read_to_string(&log_path)
        .unwrap()
        .contains("settings_invalid valid=false"));

    let _ = fs::remove_file(log_path);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn diagnostics_rotate_and_replace_old_backup() {
    let path = temp_log();
    fs::write(&path, "x".repeat(1024 * 1024)).unwrap();
    fs::write(path.with_extension("log.1"), "old").unwrap();
    let logger = DiagnosticLogger::for_path(&path);
    logger
        .record_safe(SafeDiagnostic::Settings { valid: true })
        .unwrap();
    assert_eq!(
        fs::read_to_string(path.with_extension("log.1"))
            .unwrap()
            .len(),
        1024 * 1024
    );
    assert!(fs::read_to_string(&path).unwrap().contains("valid=true"));
    let _ = fs::remove_file(path.with_extension("log.1"));
    let _ = fs::remove_file(path);
}

#[test]
fn first_oversized_typed_event_never_exceeds_active_log_limit() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);
    logger
        .record_safe(SafeDiagnostic::Cli {
            path: std::path::PathBuf::from("x".repeat(2 * 1024 * 1024)),
            exists: true,
        })
        .unwrap();
    assert!(fs::metadata(&path).unwrap().len() <= 1024 * 1024);
    let _ = fs::remove_file(path);
}

#[test]
fn oversized_unicode_path_is_capped_without_splitting_utf8() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);

    logger
        .record_safe(SafeDiagnostic::Cli {
            path: std::path::PathBuf::from("한".repeat(1024 * 1024)),
            exists: true,
        })
        .unwrap();

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.len() <= 1024 * 1024);
    assert!(std::str::from_utf8(&bytes).is_ok());
    assert!(bytes.ends_with(b"\n"));
    let _ = fs::remove_file(path);
}

#[test]
fn cloned_concurrent_writers_serialize_rotation_and_complete_lines() {
    let path = temp_log();
    let logger = DiagnosticLogger::for_path(&path);
    let mut workers = Vec::new();
    for worker in 0..8 {
        let logger = logger.clone();
        workers.push(std::thread::spawn(move || {
            for event in 0..600 {
                logger
                    .record_safe(SafeDiagnostic::Cli {
                        path: std::path::PathBuf::from(format!(
                            "worker-{worker}-event-{event}-{}",
                            "x".repeat(240)
                        )),
                        exists: true,
                    })
                    .unwrap();
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(fs::metadata(&path).unwrap().len() <= 1024 * 1024);
    let backup = path.with_extension("log.1");
    assert!(backup.exists());
    for candidate in [&path, &backup] {
        for line in fs::read_to_string(candidate).unwrap().lines() {
            assert!(line.contains("cli_unavailable") && line.contains("exists=true"));
        }
    }
    let _ = fs::remove_file(backup);
    let _ = fs::remove_file(path);
}

#[test]
fn separately_constructed_loggers_share_rotation_lock_and_keep_complete_lines() {
    let path = temp_log();
    let start = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for worker in 0..8 {
        let path = path.clone();
        let start = Arc::clone(&start);
        workers.push(std::thread::spawn(move || {
            let logger = DiagnosticLogger::for_path(path);
            start.wait();
            for event in 0..600 {
                logger
                    .record_safe(SafeDiagnostic::Cli {
                        path: std::path::PathBuf::from(format!(
                            "independent-{worker}-{event}-{}",
                            "x".repeat(240)
                        )),
                        exists: true,
                    })
                    .unwrap();
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    let backup = path.with_extension("log.1");
    assert!(fs::metadata(&path).unwrap().len() <= 1024 * 1024);
    assert!(backup.exists());
    for candidate in [&path, &backup] {
        for line in fs::read_to_string(candidate).unwrap().lines() {
            assert!(line.contains("cli_unavailable") && line.contains("exists=true"));
        }
    }
    let _ = fs::remove_file(backup);
    let _ = fs::remove_file(path);
}

#[test]
fn loggers_created_before_and_after_parent_creation_share_the_rotation_gate() {
    let root = std::env::temp_dir().join(format!(
        "diagnostic-missing-parent-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = root.join("nested").join("diagnostic.log");
    let first = DiagnosticLogger::for_path(&path);
    first
        .record_safe(SafeDiagnostic::Settings { valid: true })
        .unwrap();
    let second = DiagnosticLogger::for_path(&path);
    let start = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for worker in 0..8 {
        let logger = if worker % 2 == 0 {
            first.clone()
        } else {
            second.clone()
        };
        let start = Arc::clone(&start);
        workers.push(std::thread::spawn(move || {
            start.wait();
            for event in 0..600 {
                logger
                    .record_safe(SafeDiagnostic::Cli {
                        path: std::path::PathBuf::from(format!(
                            "creation-order-{worker}-{event}-{}",
                            "x".repeat(240)
                        )),
                        exists: true,
                    })
                    .unwrap();
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    let backup = path.with_extension("log.1");
    assert!(fs::metadata(&path).unwrap().len() <= 1024 * 1024);
    assert!(backup.exists());
    for candidate in [&path, &backup] {
        for line in fs::read_to_string(candidate).unwrap().lines() {
            assert!(line.contains("cli_unavailable") || line.contains("settings_invalid"));
        }
    }
    let _ = fs::remove_dir_all(root);
}
