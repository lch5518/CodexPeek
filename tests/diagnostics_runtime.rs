use std::{
    fs,
    sync::{Arc, Barrier},
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{DiagnosticLogger, SafeDiagnostic};

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
