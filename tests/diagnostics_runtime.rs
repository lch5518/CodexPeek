use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{DiagnosticCode, DiagnosticLogger};

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
        .record(
            DiagnosticCode::RpcFailed,
            "bearer abc\nemail=user@example.com proxy=http://secret token=xyz",
        )
        .unwrap();
    let line = fs::read_to_string(&path).unwrap();
    assert!(!line.contains("abc"));
    assert!(!line.contains("user@example.com"));
    assert!(!line.contains("secret"));
    assert!(!line.contains("\nemail"));
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
        .record(DiagnosticCode::SettingsInvalid, "safe")
        .unwrap();
    assert_eq!(
        fs::read_to_string(path.with_extension("log.1"))
            .unwrap()
            .len(),
        1024 * 1024
    );
    assert!(fs::read_to_string(&path).unwrap().contains("safe"));
    let _ = fs::remove_file(path.with_extension("log.1"));
    let _ = fs::remove_file(path);
}
