use std::{
    collections::VecDeque,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};

use codex_usage_monitor::{
    codex::UsageProvider, CodexUsage, PollState, PollTrigger, PollingService, UsageError,
    UsageWindow, WindowKind,
};

fn usage(
    fetched_at: SystemTime,
    primary_reset: Option<SystemTime>,
    secondary_reset: Option<SystemTime>,
) -> CodexUsage {
    CodexUsage {
        primary: Some(
            UsageWindow::new(WindowKind::Primary, 25.0, Some(300), primary_reset).unwrap(),
        ),
        secondary: Some(
            UsageWindow::new(WindowKind::Secondary, 50.0, Some(10_080), secondary_reset).unwrap(),
        ),
        fetched_at,
    }
}

#[test]
fn manual_refresh_has_a_ten_second_cooldown() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Manual, now), Some(false));
    state.finish(Ok(usage(now, None, None)), now);
    assert_eq!(
        state.begin(PollTrigger::Manual, now + Duration::from_secs(9)),
        None
    );
    assert_eq!(
        state.begin(PollTrigger::Manual, now + Duration::from_secs(10)),
        Some(false)
    );
}

#[test]
fn failures_preserve_last_good_and_exact_error_with_capped_backoff() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let good = usage(now, None, None);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Automatic, now), Some(false));
    state.finish(Ok(good.clone()), now);

    let mut due = now + Duration::from_secs(300);
    for (index, minutes) in [1_u64, 2, 4, 8, 15, 15].into_iter().enumerate() {
        assert_eq!(state.begin(PollTrigger::Automatic, due), Some(false));
        let error = if index % 2 == 0 {
            UsageError::RpcTimeout
        } else {
            UsageError::NotLoggedIn
        };
        state.finish(Err(error), due);

        let snapshot = state.snapshot(due);
        assert_eq!(snapshot.usage.as_ref(), Some(&good));
        assert_eq!(snapshot.last_success_at, Some(now));
        assert_eq!(snapshot.last_error, Some(error));
        assert_eq!(
            state.next_poll_at(),
            due + Duration::from_secs(minutes * 60)
        );
        due += Duration::from_secs(minutes * 60);
    }
}

#[test]
fn staleness_is_computed_at_the_requested_time() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Automatic, now), Some(false));
    state.finish(Ok(usage(now, None, None)), now);

    assert!(!state.snapshot(now + Duration::from_secs(599)).is_stale);
    assert!(state.snapshot(now + Duration::from_secs(600)).is_stale);
}

#[test]
fn both_window_resets_each_schedule_exactly_one_automatic_fetch() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let primary_reset = now + Duration::from_secs(60);
    let secondary_reset = now + Duration::from_secs(120);
    let repeated = usage(now, Some(primary_reset), Some(secondary_reset));
    let mut state = PollState::new(15, now).unwrap();

    assert_eq!(state.begin(PollTrigger::Automatic, now), Some(false));
    state.finish(Ok(repeated.clone()), now);
    assert_eq!(state.next_poll_at(), primary_reset);

    assert_eq!(
        state.begin(PollTrigger::Automatic, primary_reset),
        Some(false)
    );
    state.finish(Ok(repeated.clone()), primary_reset);
    assert_eq!(state.next_poll_at(), secondary_reset);

    assert_eq!(
        state.begin(PollTrigger::Automatic, secondary_reset),
        Some(false)
    );
    state.finish(Ok(repeated), secondary_reset);
    assert_eq!(
        state.next_poll_at(),
        secondary_reset + Duration::from_secs(15 * 60)
    );
    assert_eq!(state.begin(PollTrigger::Automatic, secondary_reset), None);
}

#[test]
fn a_new_elapsed_reset_schedules_one_immediate_automatic_fetch() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let elapsed_reset = now - Duration::from_secs(1);
    let repeated = usage(now, Some(elapsed_reset), None);
    let mut state = PollState::new(5, now).unwrap();

    assert_eq!(state.begin(PollTrigger::Manual, now), Some(false));
    state.finish(Ok(repeated.clone()), now);
    assert_eq!(state.next_poll_at(), now);
    assert_eq!(state.begin(PollTrigger::Automatic, now), Some(false));
    state.finish(Ok(repeated), now);
    assert_eq!(state.next_poll_at(), now + Duration::from_secs(300));
}

#[test]
fn forced_auth_requests_refresh_and_reset_trigger_advances_schedule() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(15, now).unwrap();
    assert_eq!(state.begin(PollTrigger::ForcedAuth, now), Some(true));
    state.finish(Ok(usage(now, None, None)), now);

    let reset = now + Duration::from_secs(60);
    assert_eq!(
        state.begin(PollTrigger::Reset(reset), now + Duration::from_secs(1)),
        None
    );
    assert_eq!(state.next_poll_at(), reset);
}

struct ProviderStep {
    result: Result<CodexUsage, UsageError>,
    waits_for_release: bool,
}

#[derive(Default)]
struct ProviderState {
    steps: VecDeque<ProviderStep>,
    calls: Vec<bool>,
    active: usize,
    max_active: usize,
    completed: usize,
    release_permits: usize,
}

#[derive(Clone, Default)]
struct ObservableProvider {
    shared: Arc<(Mutex<ProviderState>, Condvar)>,
}

impl ObservableProvider {
    fn with_steps(steps: impl IntoIterator<Item = ProviderStep>) -> Self {
        let provider = Self::default();
        provider.shared.0.lock().unwrap().steps.extend(steps);
        provider
    }

    fn wait_for_calls(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        while state.calls.len() < expected {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("provider call did not arrive in time");
            let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
            assert!(!timeout.timed_out(), "provider call did not arrive in time");
        }
    }

    fn release_one(&self) {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.release_permits += 1;
        changed.notify_all();
    }

    fn wait_for_completed(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        while state.completed < expected {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("provider call did not complete in time");
            let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
            assert!(
                !timeout.timed_out(),
                "provider call did not complete in time"
            );
        }
    }

    fn calls(&self) -> Vec<bool> {
        self.shared.0.lock().unwrap().calls.clone()
    }

    fn max_active(&self) -> usize {
        self.shared.0.lock().unwrap().max_active
    }
}

impl UsageProvider for ObservableProvider {
    fn fetch(&self, allow_auth_refresh: bool) -> Result<CodexUsage, UsageError> {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.calls.push(allow_auth_refresh);
        state.active += 1;
        state.max_active = state.max_active.max(state.active);
        let step = state.steps.pop_front().unwrap_or(ProviderStep {
            result: Err(UsageError::RequestFailed),
            waits_for_release: false,
        });
        changed.notify_all();

        while step.waits_for_release && state.release_permits == 0 {
            state = changed.wait(state).unwrap();
        }
        if step.waits_for_release {
            state.release_permits -= 1;
        }
        state.active -= 1;
        state.completed += 1;
        changed.notify_all();
        step.result
    }
}

fn success_step(value: CodexUsage) -> ProviderStep {
    ProviderStep {
        result: Ok(value),
        waits_for_release: false,
    }
}

fn blocked_success_step(value: CodexUsage) -> ProviderStep {
    ProviderStep {
        result: Ok(value),
        waits_for_release: true,
    }
}

#[test]
fn service_starts_immediately_and_forwards_automatic_or_forced_auth_policy() {
    let now = SystemTime::now();
    let provider = ObservableProvider::with_steps([
        success_step(usage(now, None, None)),
        success_step(usage(now, None, None)),
    ]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();

    provider.wait_for_calls(1);
    service.refresh_with_auth();
    provider.wait_for_calls(2);
    assert_eq!(provider.calls(), vec![false, true]);
    service.stop();

    let auto_provider = ObservableProvider::with_steps([success_step(usage(now, None, None))]);
    let auto_service = PollingService::start(Arc::new(auto_provider.clone()), 5, true).unwrap();
    auto_provider.wait_for_calls(1);
    assert_eq!(auto_provider.calls(), vec![true]);
    auto_service.stop();
}

#[test]
fn snapshot_lock_is_available_while_provider_is_blocked() {
    let now = SystemTime::now();
    let provider = ObservableProvider::with_steps([blocked_success_step(usage(now, None, None))]);
    let service = Arc::new(PollingService::start(Arc::new(provider.clone()), 5, false).unwrap());
    provider.wait_for_calls(1);

    let (sent, received) = mpsc::channel();
    let snapshot_service = Arc::clone(&service);
    let snapshot_thread = thread::spawn(move || {
        sent.send(snapshot_service.snapshot_at(now)).unwrap();
    });
    let snapshot = received
        .recv_timeout(Duration::from_millis(200))
        .expect("snapshot lock was held during provider I/O");
    assert!(snapshot.is_fetching);

    provider.release_one();
    snapshot_thread.join().unwrap();
    let service = Arc::try_unwrap(service).ok().unwrap();
    service.stop();
}

#[test]
fn service_preserves_last_good_and_exposes_exact_error() {
    let now = SystemTime::now();
    let good = usage(now, None, None);
    let provider = ObservableProvider::with_steps([
        success_step(good.clone()),
        ProviderStep {
            result: Err(UsageError::AuthenticationExpired),
            waits_for_release: false,
        },
    ]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();
    provider.wait_for_calls(1);

    service.refresh();
    provider.wait_for_completed(2);
    let snapshot = service.snapshot();
    assert_eq!(snapshot.usage.as_ref(), Some(&good));
    assert_eq!(snapshot.last_error, Some(UsageError::AuthenticationExpired));
    service.stop();
}

#[test]
fn service_repeated_future_reset_causes_one_extra_fetch_without_looping() {
    let now = SystemTime::now();
    let reset = now + Duration::from_millis(150);
    let repeated = usage(now, Some(reset), None);
    let provider =
        ObservableProvider::with_steps([success_step(repeated.clone()), success_step(repeated)]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();

    provider.wait_for_calls(2);
    provider.wait_for_completed(2);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(provider.calls().len(), 2);
    service.stop();
}

#[test]
fn service_snapshot_staleness_uses_the_requested_current_time() {
    let fetched_at = SystemTime::now();
    let provider = ObservableProvider::with_steps([success_step(usage(fetched_at, None, None))]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();
    provider.wait_for_completed(1);

    let last_success = service.snapshot().last_success_at.unwrap();
    assert!(
        !service
            .snapshot_at(last_success + Duration::from_secs(599))
            .is_stale
    );
    assert!(
        service
            .snapshot_at(last_success + Duration::from_secs(600))
            .is_stale
    );
    service.stop();
}

#[test]
fn queued_manual_refreshes_obey_cooldown_and_never_overlap() {
    let now = SystemTime::now();
    let provider = ObservableProvider::with_steps([
        blocked_success_step(usage(now, None, None)),
        blocked_success_step(usage(now, None, None)),
    ]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();
    provider.wait_for_calls(1);
    service.refresh();
    service.refresh();

    provider.release_one();
    provider.wait_for_calls(2);
    provider.release_one();
    provider.wait_for_completed(2);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(provider.calls().len(), 2);
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[test]
fn service_stops_normally_after_provider_returns() {
    let now = SystemTime::now();
    let provider = ObservableProvider::with_steps([success_step(usage(now, None, None))]);
    let service = PollingService::start(Arc::new(provider.clone()), 5, false).unwrap();
    provider.wait_for_calls(1);

    service.stop();
}
