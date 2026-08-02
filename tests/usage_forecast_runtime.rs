use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime},
};

use codex_usage_monitor::{
    CodexUsage, ForecastPolicy, ForecastResult, SafeDiagnostic, UsageForecastService,
    UsageHistoryOperation, UsageHistoryStore, UsageProfileId, UsageSampleSink, UsageWindow,
    WindowKind,
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
    service.set_enabled(true);
    service.clear_all();
    assert_eq!(
        service.forecast_at(UsageProfileId::System, WindowKind::Primary, now),
        None
    );
    service.remove_profile(UsageProfileId::System);
    assert_eq!(
        service.forecast_at(UsageProfileId::System, WindowKind::Primary, now),
        None
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
