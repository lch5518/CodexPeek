use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    UsageHistory, UsageHistoryError, UsageHistoryRecord, UsageHistoryStore, UsageProfileId,
    UsageSample, WindowKind,
};

static TEST_NONCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "codex-peek-usage-history-{label}-{}-{}",
            std::process::id(),
            TEST_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn sample(
    profile_id: UsageProfileId,
    window_kind: WindowKind,
    used_percent: f64,
    resets_at: Option<SystemTime>,
    observed_at: SystemTime,
    now: SystemTime,
) -> UsageSample {
    UsageSample::new(
        profile_id,
        window_kind,
        used_percent,
        resets_at,
        observed_at,
        now,
    )
    .unwrap()
}

#[test]
fn history_store_round_trips_only_validated_sample_fields() {
    let root = TestRoot::new("round-trip");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = at(2_000_000);
    let sample = UsageSample::new(
        UsageProfileId::Managed(7),
        WindowKind::Secondary,
        37.5,
        Some(at(2_000_600)),
        at(1_999_700),
        now,
    )
    .unwrap();
    let mut history = UsageHistory::default();

    assert!(history.record(sample.clone(), now).unwrap().is_added());
    store.save(&history, now).unwrap();

    assert_eq!(store.load(now).unwrap().samples(), &[sample]);
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(stored["schema_version"], 1);
    assert_eq!(stored["samples"][0]["profile_id"], "managed:7");
    assert_eq!(stored["samples"][0]["window_kind"], "secondary");
    assert!(stored["samples"][0].get("window_duration_mins").is_none());
    assert!(stored["samples"][0].get("resets_at").unwrap().is_u64());
    assert!(stored["samples"][0].get("observed_at").unwrap().is_u64());
}

#[test]
fn daily_usage_groups_samples_and_returns_daily_increase() {
    let now = at(3 * 86_400);
    let mut history = UsageHistory::default();
    let profile = UsageProfileId::System;

    for (used, observed_at) in [(10.0, at(3_600)), (25.0, at(7_200))] {
        assert!(history
            .record(
                sample(profile, WindowKind::Secondary, used, None, observed_at, now,),
                now,
            )
            .unwrap()
            .is_added());
    }
    for (used, observed_at) in [(30.0, at(86_400 + 3_600)), (42.0, at(86_400 + 7_200))] {
        assert!(history
            .record(
                sample(profile, WindowKind::Secondary, used, None, observed_at, now,),
                now,
            )
            .unwrap()
            .is_added());
    }

    let daily = history.daily_usage_for(profile, WindowKind::Secondary);
    assert_eq!(daily.len(), 2);
    assert_eq!(daily[0].day(), 0);
    assert_eq!(daily[0].increase_percent(), 15.0);
    assert_eq!(daily[1].day(), 1);
    assert_eq!(daily[1].increase_percent(), 12.0);
}

#[test]
fn daily_usage_keeps_single_sample_as_zero_increase() {
    let now = at(86_400);
    let mut history = UsageHistory::default();
    history
        .record(
            sample(
                UsageProfileId::System,
                WindowKind::Secondary,
                37.5,
                None,
                at(3_600),
                now,
            ),
            now,
        )
        .unwrap();

    let daily = history.daily_usage_for(UsageProfileId::System, WindowKind::Secondary);
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].increase_percent(), 0.0);
}

#[test]
fn daily_usage_is_limited_to_the_latest_fourteen_days() {
    let now = at(20 * 86_400);
    let mut history = UsageHistory::default();
    for day in 0..16 {
        history
            .record(
                sample(
                    UsageProfileId::System,
                    WindowKind::Secondary,
                    day as f64,
                    None,
                    at(day * 86_400 + 3_600),
                    now,
                ),
                now,
            )
            .unwrap();
    }

    let daily = history.daily_usage_for(UsageProfileId::System, WindowKind::Secondary);
    assert_eq!(daily.len(), 14);
    assert_eq!(daily.first().unwrap().day(), 2);
    assert_eq!(daily.last().unwrap().day(), 15);
}

#[test]
fn empty_corrupt_and_unsupported_history_files_are_quarantined_as_safe_empty_history() {
    let now = at(4_000_000);
    for (label, bytes) in [
        ("empty", b"".as_slice()),
        ("corrupt", b"not json".as_slice()),
        (
            "unsupported",
            br#"{"schema_version":2,"samples":[]}"#.as_slice(),
        ),
    ] {
        let root = TestRoot::new(label);
        let store = UsageHistoryStore::for_root(&root.0);
        fs::write(store.path(), bytes).unwrap();

        assert!(store.load(now).unwrap().samples().is_empty(), "{label}");
        assert!(!store.path().exists(), "{label}");
        let quarantined = fs::read_dir(&root.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("usage-history.corrupt-")
            })
            .unwrap();
        assert_eq!(fs::read(quarantined).unwrap(), bytes, "{label}");
    }
}

#[test]
fn oversized_history_file_is_quarantined_without_parsing_its_payload() {
    let root = TestRoot::new("oversized");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = at(4_500_000);
    let oversized = format!(
        "{{\"schema_version\":1,\"samples\":[],\"ignored\":\"{}\"}}",
        "x".repeat(4 * 1024 * 1024)
    );
    fs::write(store.path(), oversized.as_bytes()).unwrap();

    assert!(store.load(now).unwrap().samples().is_empty());
    assert!(!store.path().exists());
    assert!(fs::read_dir(&root.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("usage-history.corrupt-")
    }));
}

#[test]
fn interrupted_temporary_history_file_is_ignored_without_disturbing_saved_history() {
    let root = TestRoot::new("interrupted-temp");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = at(5_000_000);
    let mut history = UsageHistory::default();
    let original = sample(
        UsageProfileId::System,
        WindowKind::Primary,
        12.0,
        Some(at(5_000_600)),
        at(4_999_000),
        now,
    );
    history.record(original.clone(), now).unwrap();
    store.save(&history, now).unwrap();
    fs::write(root.0.join(".usage-history.tmp-interrupted"), b"{").unwrap();

    assert_eq!(store.load(now).unwrap().samples(), &[original]);
    assert!(root.0.join(".usage-history.tmp-interrupted").exists());
}

#[test]
fn retention_and_per_stream_cap_keep_the_most_recent_bounded_samples() {
    let now = at(100_000_000);
    let reset = Some(at(100_100_000));
    let mut history = UsageHistory::default();
    let expired = sample(
        UsageProfileId::System,
        WindowKind::Primary,
        1.0,
        reset,
        now - Duration::from_secs(30 * 24 * 60 * 60 + 1),
        now,
    );
    assert_eq!(
        history.record(expired, now).unwrap(),
        UsageHistoryRecord::SkippedExpired
    );
    for index in 0..1_001_u64 {
        let observed_at = now - Duration::from_secs((1_000 - index) * 5 * 60);
        history
            .record(
                sample(
                    UsageProfileId::System,
                    WindowKind::Primary,
                    index as f64,
                    reset,
                    observed_at,
                    now,
                ),
                now,
            )
            .unwrap();
    }

    let samples: Vec<_> = history
        .samples_for(UsageProfileId::System, WindowKind::Primary)
        .collect();
    assert_eq!(samples.len(), 1_000);
    assert_eq!(samples.first().unwrap().used_percent(), 1.0);
    assert_eq!(samples.last().unwrap().used_percent(), 1_000.0);
}

#[test]
fn profile_and_window_streams_are_independent() {
    let now = at(6_000_000);
    let mut history = UsageHistory::default();
    for (profile_id, window_kind) in [
        (UsageProfileId::System, WindowKind::Primary),
        (UsageProfileId::System, WindowKind::Secondary),
        (UsageProfileId::Managed(4), WindowKind::Primary),
    ] {
        history
            .record(
                sample(
                    profile_id,
                    window_kind,
                    10.0,
                    Some(at(6_000_600)),
                    at(5_999_000),
                    now,
                ),
                now,
            )
            .unwrap();
    }

    assert_eq!(history.samples().len(), 3);
    assert_eq!(
        history
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
    assert_eq!(
        history
            .samples_for(UsageProfileId::Managed(4), WindowKind::Secondary)
            .count(),
        0
    );
}

#[test]
fn profile_removal_and_clear_remove_only_requested_history() {
    let now = at(7_000_000);
    let mut history = UsageHistory::default();
    for profile_id in [UsageProfileId::System, UsageProfileId::Managed(9)] {
        history
            .record(
                sample(
                    profile_id,
                    WindowKind::Primary,
                    4.0,
                    Some(at(7_000_600)),
                    at(6_999_000),
                    now,
                ),
                now,
            )
            .unwrap();
    }

    assert_eq!(history.remove_profile(UsageProfileId::Managed(9)), 1);
    assert_eq!(history.samples().len(), 1);
    history.clear();
    assert!(history.samples().is_empty());
}

#[test]
fn exact_duplicates_and_short_intervals_are_skipped_unless_reset_changes() {
    let now = at(8_000_000);
    let observed_at = at(7_999_000);
    let mut history = UsageHistory::default();
    let first = sample(
        UsageProfileId::System,
        WindowKind::Primary,
        10.0,
        Some(at(8_000_600)),
        observed_at,
        now,
    );
    assert_eq!(
        history.record(first.clone(), now).unwrap(),
        UsageHistoryRecord::Added
    );
    assert_eq!(
        history.record(first, now).unwrap(),
        UsageHistoryRecord::SkippedDuplicate
    );
    assert_eq!(
        history
            .record(
                sample(
                    UsageProfileId::System,
                    WindowKind::Primary,
                    11.0,
                    Some(at(8_000_600)),
                    observed_at + Duration::from_secs(60),
                    now,
                ),
                now,
            )
            .unwrap(),
        UsageHistoryRecord::SkippedMinimumInterval
    );
    assert_eq!(
        history
            .record(
                sample(
                    UsageProfileId::System,
                    WindowKind::Primary,
                    11.0,
                    Some(at(8_001_200)),
                    observed_at + Duration::from_secs(120),
                    now,
                ),
                now,
            )
            .unwrap(),
        UsageHistoryRecord::Added
    );
    assert_eq!(history.samples().len(), 2);
}

#[test]
fn invalid_numbers_and_timestamp_orders_are_rejected_and_quarantined() {
    let now = at(9_000_000);
    assert_eq!(
        UsageSample::new(
            UsageProfileId::Managed(0),
            WindowKind::Primary,
            1.0,
            None,
            now,
            now,
        ),
        Err(UsageHistoryError::InvalidProfile)
    );
    for invalid_usage in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1] {
        assert_eq!(
            UsageSample::new(
                UsageProfileId::System,
                WindowKind::Primary,
                invalid_usage,
                None,
                now,
                now,
            ),
            Err(UsageHistoryError::InvalidUsage)
        );
    }
    assert_eq!(
        UsageSample::new(
            UsageProfileId::System,
            WindowKind::Primary,
            1.0,
            None,
            UNIX_EPOCH - Duration::from_secs(1),
            now,
        ),
        Err(UsageHistoryError::PreEpochTimestamp)
    );
    assert_eq!(
        UsageSample::new(
            UsageProfileId::System,
            WindowKind::Primary,
            1.0,
            None,
            now + Duration::from_secs(1),
            now,
        ),
        Err(UsageHistoryError::FutureObservation)
    );
    assert_eq!(
        UsageSample::new(
            UsageProfileId::System,
            WindowKind::Primary,
            1.0,
            Some(now - Duration::from_secs(1)),
            now,
            now,
        ),
        Err(UsageHistoryError::ReversedTimestamps)
    );

    let root = TestRoot::new("invalid-json-values");
    let store = UsageHistoryStore::for_root(&root.0);
    fs::write(
        store.path(),
        br#"{"schema_version":1,"samples":[{"profile_id":"system","window_kind":"primary","used_percent":-1,"resets_at":null,"observed_at":9000000}]}"#,
    )
    .unwrap();
    assert!(store.load(now).unwrap().samples().is_empty());
    assert!(!store.path().exists());
}

#[test]
fn negative_zero_usage_is_rejected() {
    let now = at(9_100_000);

    assert_eq!(
        UsageSample::new(
            UsageProfileId::System,
            WindowKind::Primary,
            -0.0,
            None,
            now,
            now,
        ),
        Err(UsageHistoryError::InvalidUsage)
    );
}

#[test]
fn failed_atomic_replacement_leaves_the_existing_history_file_unchanged() {
    let root = TestRoot::new("atomic-failure");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = at(10_000_000);
    let original = br#"{"schema_version":1,"samples":[]}"#;
    fs::write(store.path(), original).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.path())
        .unwrap();

    let result = store.save(&UsageHistory::default(), now);

    drop(lock);
    assert!(result.is_err());
    assert_eq!(fs::read(store.path()).unwrap(), original);
}
