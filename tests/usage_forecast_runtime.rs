use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use codex_usage_monitor::{
    CodexUsage, ConsumptionPaceAssessment, ConsumptionPaceLevel, ForecastPolicy, ForecastResult,
    SafeDiagnostic, UsageForecastService, UsageHistory, UsageHistoryOperation, UsageHistoryStore,
    UsageProfileId, UsageSample, UsageSampleSink, UsageWindow, WindowKind,
};

static TEST_NONCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "codex-peek-usage-forecast-{label}-{}-{}",
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

fn usage(kind: WindowKind, percent: f64, reset: SystemTime, observed_at: SystemTime) -> CodexUsage {
    let window = UsageWindow::new(kind, percent, Some(60), Some(reset)).unwrap();
    CodexUsage {
        primary: (kind == WindowKind::Primary).then_some(window.clone()),
        secondary: (kind == WindowKind::Secondary).then_some(window),
        reset_credits: None,
        fetched_at: observed_at,
    }
}

fn two_window_usage(reset: SystemTime, observed_at: SystemTime) -> CodexUsage {
    CodexUsage {
        primary: Some(UsageWindow::new(WindowKind::Primary, 12.0, Some(60), Some(reset)).unwrap()),
        secondary: Some(
            UsageWindow::new(WindowKind::Secondary, 34.0, Some(60), Some(reset)).unwrap(),
        ),
        reset_credits: None,
        fetched_at: observed_at,
    }
}

fn preload_history(store: &UsageHistoryStore, now: SystemTime) {
    let reset = now + Duration::from_secs(60 * 60);
    let mut history = UsageHistory::default();
    for index in 0..1_000_u64 {
        let observed_at = now - Duration::from_secs((1_000 - index) * 5 * 60);
        history
            .record(
                UsageSample::new(
                    UsageProfileId::System,
                    WindowKind::Primary,
                    index as f64,
                    Some(reset),
                    observed_at,
                    now,
                )
                .unwrap(),
                now,
            )
            .unwrap();
    }
    store.save(&history, now).unwrap();
}

fn wait_for_forecast(
    service: &UsageForecastService,
    id: UsageProfileId,
    kind: WindowKind,
    now: SystemTime,
) -> ForecastResult {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result) = service.forecast_at(id, kind, now) {
            if result != ForecastResult::Stale {
                return result;
            }
        }
        assert!(Instant::now() < deadline, "forecast was not cached in time");
        thread::yield_now();
    }
}

fn wait_for_pace(
    service: &UsageForecastService,
    id: UsageProfileId,
    kind: WindowKind,
    now: SystemTime,
) -> ConsumptionPaceAssessment {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result @ ConsumptionPaceAssessment::Ready(_)) = service.pace_at(id, kind, now) {
            return result;
        }
        assert!(Instant::now() < deadline, "pace was not cached in time");
        thread::yield_now();
    }
}

#[test]
fn low_activity_pace_is_cached_even_without_an_exhaustion_forecast() {
    let root = TestRoot::new("low-activity-pace");
    let service = UsageForecastService::start(
        UsageHistoryStore::for_root(&root.0),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(10 * 60 * 60);
    for (minutes_ago, percent) in [(120, 10.0), (60, 10.5), (0, 10.9)] {
        let observed_at = now - Duration::from_secs(minutes_ago * 60);
        service.record_success(
            UsageProfileId::System,
            &usage(WindowKind::Secondary, percent, reset, observed_at),
            observed_at,
        );
    }

    assert_eq!(
        wait_for_forecast(&service, UsageProfileId::System, WindowKind::Secondary, now,),
        ForecastResult::InsufficientActivity
    );
    let ConsumptionPaceAssessment::Ready(metrics) =
        wait_for_pace(&service, UsageProfileId::System, WindowKind::Secondary, now)
    else {
        unreachable!();
    };
    assert_eq!(metrics.level, ConsumptionPaceLevel::Comfortable);
    service.stop();
}

#[test]
fn successful_samples_are_cached_per_profile_and_window() {
    let root = TestRoot::new("streams");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = UsageForecastService::start(
        store,
        [UsageProfileId::System, UsageProfileId::Managed(7)],
        ForecastPolicy::new(Duration::from_secs(60)),
    );
    let now = SystemTime::now();
    let reset = std::time::UNIX_EPOCH
        + Duration::from_secs(
            now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3 * 60 * 60,
        );
    for (id, kind, base) in [
        (UsageProfileId::System, WindowKind::Primary, 10.0),
        (UsageProfileId::System, WindowKind::Secondary, 40.0),
        (UsageProfileId::Managed(7), WindowKind::Primary, 70.0),
    ] {
        for (minutes_ago, increase) in [(40, 0.0), (20, 10.0), (0, 20.0)] {
            let observed_at = now - Duration::from_secs(minutes_ago * 60);
            service.record_success(
                id,
                &usage(kind, base + increase, reset, observed_at),
                observed_at,
            );
        }
    }

    let lookup_now = SystemTime::now();
    let primary = wait_for_forecast(
        &service,
        UsageProfileId::System,
        WindowKind::Primary,
        lookup_now,
    );
    assert!(
        matches!(primary, ForecastResult::ForecastAvailable(_)),
        "{primary:?}"
    );
    assert!(matches!(
        wait_for_forecast(
            &service,
            UsageProfileId::System,
            WindowKind::Secondary,
            lookup_now
        ),
        ForecastResult::ForecastAvailable(_)
    ));
    assert!(matches!(
        wait_for_forecast(
            &service,
            UsageProfileId::Managed(7),
            WindowKind::Primary,
            lookup_now
        ),
        ForecastResult::ForecastAvailable(_)
    ));
    assert_eq!(
        service.forecast_at(
            UsageProfileId::Managed(7),
            WindowKind::Secondary,
            lookup_now
        ),
        None
    );
    service.stop();
}

#[test]
fn one_successful_two_window_response_persists_both_streams() {
    let root = TestRoot::new("two-windows");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(60 * 60);

    service.record_success(UsageProfileId::System, &two_window_usage(reset, now), now);
    service.stop();

    let history = store.load(SystemTime::now()).unwrap();
    assert_eq!(
        history
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
    assert_eq!(
        history
            .samples_for(UsageProfileId::System, WindowKind::Secondary)
            .count(),
        1
    );
}

#[test]
fn disabled_cleared_and_removed_profiles_do_not_expose_cached_forecasts() {
    let root = TestRoot::new("lifecycle");
    let service = UsageForecastService::start(
        UsageHistoryStore::for_root(&root.0),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(3 * 60 * 60);
    for (minutes_ago, percent) in [(40, 10.0), (20, 20.0), (0, 30.0)] {
        let observed_at = now - Duration::from_secs(minutes_ago * 60);
        service.record_success(
            UsageProfileId::System,
            &usage(WindowKind::Primary, percent, reset, observed_at),
            observed_at,
        );
    }
    let _ = wait_for_forecast(&service, UsageProfileId::System, WindowKind::Primary, now);

    service.set_enabled(false);
    assert_eq!(
        service.forecast_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    assert_eq!(
        service.pace_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    service.set_enabled(true);
    service.clear_all();
    assert_eq!(
        service.forecast_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    assert_eq!(
        service.pace_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    service.remove_profile(UsageProfileId::System);
    assert_eq!(
        service.forecast_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    assert_eq!(
        service.pace_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    service.stop();
}

#[test]
fn stale_pace_does_not_expose_an_old_consumption_speed() {
    let root = TestRoot::new("stale-pace");
    let service = UsageForecastService::start(
        UsageHistoryStore::for_root(&root.0),
        [UsageProfileId::System],
        ForecastPolicy::new(Duration::from_secs(60)),
    );
    let now = SystemTime::now();
    service.record_success(
        UsageProfileId::System,
        &usage(
            WindowKind::Primary,
            10.0,
            now + Duration::from_secs(60 * 60),
            now,
        ),
        now,
    );
    let _ = wait_for_forecast(&service, UsageProfileId::System, WindowKind::Primary, now);

    assert!(matches!(
        service.pace_at(UsageProfileId::System, WindowKind::Primary, now),
        Some(ConsumptionPaceAssessment::Measuring { .. })
    ));
    assert_eq!(
        service.pace_at(
            UsageProfileId::System,
            WindowKind::Primary,
            now + Duration::from_secs(10 * 60 + 1),
        ),
        Some(ConsumptionPaceAssessment::Unavailable)
    );
    service.stop();
}

#[test]
fn shutdown_flushes_history_for_a_restart() {
    let root = TestRoot::new("restart");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = SystemTime::now();
    let reset = std::time::UNIX_EPOCH
        + Duration::from_secs(
            now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3 * 60 * 60,
        );
    let first = UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    for (minutes_ago, percent) in [(40, 10.0), (20, 20.0), (5, 30.0)] {
        let observed_at = now - Duration::from_secs(minutes_ago * 60);
        first.record_success(
            UsageProfileId::System,
            &usage(WindowKind::Primary, percent, reset, observed_at),
            observed_at,
        );
    }
    first.stop();
    assert_eq!(store.load(SystemTime::now()).unwrap().samples().len(), 3);

    let second = UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    second.record_success(
        UsageProfileId::System,
        &usage(WindowKind::Primary, 40.0, reset, now),
        now,
    );
    let restored = wait_for_forecast(
        &second,
        UsageProfileId::System,
        WindowKind::Primary,
        SystemTime::now(),
    );
    assert!(
        matches!(restored, ForecastResult::ForecastAvailable(_)),
        "{restored:?}"
    );
    second.stop();
    assert_eq!(store.load(SystemTime::now()).unwrap().samples().len(), 4);
}

#[test]
fn storage_failures_are_reported_without_exposing_storage_details() {
    let root = std::env::temp_dir().join(format!(
        "codex-peek-usage-forecast-file-root-{}-{}",
        std::process::id(),
        TEST_NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&root, b"not a directory").unwrap();
    let service = UsageForecastService::start(
        UsageHistoryStore::for_root(&root),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    service.clear_all();
    service.stop();

    assert!(service.take_diagnostics().iter().any(|event| {
        matches!(
            event,
            SafeDiagnostic::UsageHistory {
                operation: UsageHistoryOperation::Load | UsageHistoryOperation::Save
            }
        )
    }));
    let _ = fs::remove_file(root);
}

#[test]
fn clear_is_not_lost_when_the_sample_queue_is_saturated() {
    let root = TestRoot::new("saturated-clear");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = SystemTime::now();
    preload_history(&store, now);
    let service = Arc::new(UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    ));
    let reset = now + Duration::from_secs(60 * 60);
    let sample = usage(WindowKind::Primary, 10.0, reset, now);
    let mut senders = Vec::new();
    for _ in 0..8 {
        let service = Arc::clone(&service);
        let sample = sample.clone();
        senders.push(thread::spawn(move || {
            for _ in 0..256 {
                service.record_success(UsageProfileId::System, &sample, now);
            }
        }));
    }
    for sender in senders {
        sender.join().unwrap();
    }

    service.clear_all();
    service.stop();
    assert!(store.load(SystemTime::now()).unwrap().samples().is_empty());
}

#[test]
fn remove_is_not_lost_when_the_sample_queue_is_saturated() {
    let root = TestRoot::new("saturated-remove");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = SystemTime::now();
    preload_history(&store, now);
    let service = Arc::new(UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    ));
    let reset = now + Duration::from_secs(60 * 60);
    let sample = usage(WindowKind::Primary, 10.0, reset, now);
    let mut senders = Vec::new();
    for _ in 0..8 {
        let service = Arc::clone(&service);
        let sample = sample.clone();
        senders.push(thread::spawn(move || {
            for _ in 0..256 {
                service.record_success(UsageProfileId::System, &sample, now);
            }
        }));
    }
    for sender in senders {
        sender.join().unwrap();
    }

    service.remove_profile(UsageProfileId::System);
    service.stop();
    assert!(store.load(SystemTime::now()).unwrap().samples().is_empty());
}

#[test]
fn queued_samples_before_disable_do_not_reappear_after_reenable() {
    let root = TestRoot::new("disable-generation");
    let store = UsageHistoryStore::for_root(&root.0);
    let now = SystemTime::now();
    preload_history(&store, now);
    let service = Arc::new(UsageForecastService::start(
        store,
        [UsageProfileId::System],
        ForecastPolicy::default(),
    ));
    let reset = now + Duration::from_secs(3 * 60 * 60);
    let sample = usage(WindowKind::Primary, 10.0, reset, now);
    let mut senders = Vec::new();
    for _ in 0..8 {
        let service = Arc::clone(&service);
        let sample = sample.clone();
        senders.push(thread::spawn(move || {
            for _ in 0..256 {
                service.record_success(UsageProfileId::System, &sample, now);
            }
        }));
    }
    for sender in senders {
        sender.join().unwrap();
    }

    service.set_enabled(false);
    service.set_enabled(true);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        assert_eq!(
            service.forecast_at(
                UsageProfileId::System,
                WindowKind::Primary,
                SystemTime::now()
            ),
            None
        );
        thread::yield_now();
    }
    service.stop();
}

#[test]
fn first_sample_after_profile_registration_is_not_dropped_by_channel_ordering() {
    let root = TestRoot::new("add-sample-ordering");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = UsageForecastService::start(store.clone(), [], ForecastPolicy::default());
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(60 * 60);

    thread::sleep(Duration::from_millis(75));
    service.add_profile(UsageProfileId::System);
    service.record_success(
        UsageProfileId::System,
        &usage(WindowKind::Primary, 10.0, reset, now),
        now,
    );
    service.stop();

    assert_eq!(
        store
            .load(SystemTime::now())
            .unwrap()
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
}

#[test]
fn sample_after_clear_waits_until_the_clear_lifecycle_boundary_is_applied() {
    let root = TestRoot::new("clear-sample-ordering");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(60 * 60);

    thread::sleep(Duration::from_millis(75));
    service.clear_all();
    service.record_success(
        UsageProfileId::System,
        &usage(WindowKind::Primary, 10.0, reset, now),
        now,
    );
    service.stop();

    assert_eq!(
        store
            .load(SystemTime::now())
            .unwrap()
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
}

#[test]
fn sample_after_remove_and_reregistration_waits_for_both_lifecycle_controls() {
    let root = TestRoot::new("remove-add-sample-ordering");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(60 * 60);

    thread::sleep(Duration::from_millis(75));
    service.remove_profile(UsageProfileId::System);
    service.add_profile(UsageProfileId::System);
    service.record_success(
        UsageProfileId::System,
        &usage(WindowKind::Primary, 10.0, reset, now),
        now,
    );
    service.stop();

    assert_eq!(
        store
            .load(SystemTime::now())
            .unwrap()
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
}

#[test]
fn concurrent_lifecycle_controls_do_not_strand_the_current_generation_sample() {
    let root = TestRoot::new("concurrent-lifecycle-ordering");
    let store = UsageHistoryStore::for_root(&root.0);
    let service = Arc::new(UsageForecastService::start(
        store.clone(),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    ));
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(60 * 60);

    for _ in 0..64 {
        let barrier = Arc::new(Barrier::new(3));
        let first_service = Arc::clone(&service);
        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            first_barrier.wait();
            first_service.clear_all();
        });
        let second_service = Arc::clone(&service);
        let second_barrier = Arc::clone(&barrier);
        let second = thread::spawn(move || {
            second_barrier.wait();
            second_service.clear_all();
        });
        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
    }

    service.record_success(
        UsageProfileId::System,
        &usage(WindowKind::Primary, 10.0, reset, now),
        now,
    );
    service.stop();

    assert_eq!(
        store
            .load(SystemTime::now())
            .unwrap()
            .samples_for(UsageProfileId::System, WindowKind::Primary)
            .count(),
        1
    );
}
