use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use codex_usage_monitor::{
    codex::{LoginPageOpener, OperationCancellation, ProfileAccountProvider},
    CodexUsage, PollTrigger, ProfileExecutionContext, ProfilePollEvent, ProfilePollingService,
    UsageError, UsageProfileId, UsageProfileRoot, UsageSampleSink, UsageWindow, WindowKind,
};

#[derive(Clone)]
struct ProviderStep {
    operation: &'static str,
    result: ProviderResult,
    waits_for_release: bool,
}

#[derive(Clone)]
enum ProviderResult {
    Fetch(Result<CodexUsage, UsageError>),
    Login(Result<bool, UsageError>),
    Logout(Result<(), UsageError>),
}

#[derive(Default)]
struct ProviderState {
    steps: VecDeque<ProviderStep>,
    calls: Vec<(UsageProfileId, &'static str)>,
    completed: usize,
    release_permits: usize,
}

#[derive(Clone, Default)]
struct FakeProfileProvider {
    shared: Arc<(Mutex<ProviderState>, Condvar)>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl FakeProfileProvider {
    fn with_steps(steps: impl IntoIterator<Item = ProviderStep>) -> Self {
        let provider = Self::default();
        provider.shared.0.lock().unwrap().steps.extend(steps);
        provider
    }

    fn wait_for_calls(&self, expected: usize) {
        self.wait_until(|state| state.calls.len() >= expected, "provider call");
    }

    fn wait_for_completed(&self, expected: usize) {
        self.wait_until(|state| state.completed >= expected, "provider completion");
    }

    fn wait_until(&self, condition: impl Fn(&ProviderState) -> bool, description: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        while !condition(&state) {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("{description} did not arrive in time"));
            let (next, timeout) = changed.wait_timeout(state, remaining).unwrap();
            state = next;
            assert!(!timeout.timed_out(), "{description} did not arrive in time");
        }
    }

    fn release_one(&self) {
        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.release_permits += 1;
        changed.notify_all();
    }

    fn calls(&self) -> Vec<(UsageProfileId, &'static str)> {
        self.shared.0.lock().unwrap().calls.clone()
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }

    fn run_step(&self, id: UsageProfileId, operation: &'static str) -> ProviderResult {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);

        let (lock, changed) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.calls.push((id, operation));
        let step = state.steps.pop_front().expect("unexpected provider call");
        assert_eq!(step.operation, operation);
        changed.notify_all();
        while step.waits_for_release && state.release_permits == 0 {
            state = changed.wait(state).unwrap();
        }
        if step.waits_for_release {
            state.release_permits -= 1;
        }
        state.completed += 1;
        changed.notify_all();
        drop(state);

        self.active.fetch_sub(1, Ordering::SeqCst);
        step.result
    }
}

impl ProfileAccountProvider for FakeProfileProvider {
    fn fetch_profile(
        &self,
        profile: &ProfileExecutionContext,
        _allow_auth_refresh: bool,
        _cancellation: OperationCancellation,
    ) -> Result<CodexUsage, UsageError> {
        match self.run_step(profile.id(), "fetch") {
            ProviderResult::Fetch(result) => result,
            _ => panic!("wrong fake result for fetch"),
        }
    }

    fn login_profile(
        &self,
        profile: &ProfileExecutionContext,
        _open: LoginPageOpener,
        _cancellation: OperationCancellation,
    ) -> Result<bool, UsageError> {
        match self.run_step(profile.id(), "login") {
            ProviderResult::Login(result) => result,
            _ => panic!("wrong fake result for login"),
        }
    }

    fn logout_profile(
        &self,
        profile: &ProfileExecutionContext,
        _cancellation: OperationCancellation,
    ) -> Result<(), UsageError> {
        match self.run_step(profile.id(), "logout") {
            ProviderResult::Logout(result) => result,
            _ => panic!("wrong fake result for logout"),
        }
    }
}

fn usage_for(id: UsageProfileId) -> CodexUsage {
    let used_percent = match id {
        UsageProfileId::System => 20.0,
        UsageProfileId::Managed(sequence) => f64::from(sequence),
    };
    CodexUsage {
        primary: Some(
            UsageWindow::new(WindowKind::Primary, used_percent, Some(300), None).unwrap(),
        ),
        secondary: None,
        reset_credits: None,
        fetched_at: SystemTime::now(),
        daily_token_usage: Vec::new(),
    }
}

fn fetch_step(id: UsageProfileId) -> ProviderStep {
    ProviderStep {
        operation: "fetch",
        result: ProviderResult::Fetch(Ok(usage_for(id))),
        waits_for_release: false,
    }
}

fn managed_context(sequence: u32) -> ProfileExecutionContext {
    let root = UsageProfileRoot::new(PathBuf::from("profile-poller-test-root"));
    ProfileExecutionContext::managed(&root, UsageProfileId::Managed(sequence)).unwrap()
}

fn no_opener() -> LoginPageOpener {
    Arc::new(|_| Ok(()))
}

#[derive(Clone, Default)]
struct CapturingSampleSink {
    samples: Arc<Mutex<Vec<(UsageProfileId, CodexUsage)>>>,
}

impl CapturingSampleSink {
    fn samples(&self) -> Vec<(UsageProfileId, CodexUsage)> {
        self.samples.lock().unwrap().clone()
    }
}

impl UsageSampleSink for CapturingSampleSink {
    fn record_success(&self, id: UsageProfileId, usage: &CodexUsage, _observed_at: SystemTime) {
        self.samples.lock().unwrap().push((id, usage.clone()));
    }
}

#[test]
fn successful_fetches_are_forwarded_to_the_sample_sink_but_failures_are_not() {
    let provider = FakeProfileProvider::with_steps([
        ProviderStep {
            operation: "fetch",
            result: ProviderResult::Fetch(Ok(usage_for(UsageProfileId::System))),
            waits_for_release: false,
        },
        ProviderStep {
            operation: "fetch",
            result: ProviderResult::Fetch(Err(UsageError::RpcTimeout)),
            waits_for_release: false,
        },
    ]);
    let sink = CapturingSampleSink::default();
    let service = ProfilePollingService::start_with_sample_sink(
        Arc::new(provider.clone()),
        Arc::new(sink.clone()),
        vec![ProfileExecutionContext::system()],
        UsageProfileId::System,
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(1);

    service.refresh_selected(PollTrigger::Manual).unwrap();
    provider.wait_for_completed(2);

    let samples = sink.samples();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].0, UsageProfileId::System);
    assert_eq!(
        samples[0]
            .1
            .primary
            .as_ref()
            .map(|window| window.used_percent),
        Some(20.0)
    );
    service.stop();
}

#[test]
fn startup_fetches_selected_first_then_stable_order_without_overlap() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::Managed(2)),
        fetch_step(UsageProfileId::System),
        fetch_step(UsageProfileId::Managed(1)),
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![
            ProfileExecutionContext::system(),
            managed_context(1),
            managed_context(2),
        ],
        UsageProfileId::Managed(2),
        5,
        false,
    )
    .unwrap();

    provider.wait_for_completed(3);
    assert_eq!(
        provider.calls(),
        vec![
            (UsageProfileId::Managed(2), "fetch"),
            (UsageProfileId::System, "fetch"),
            (UsageProfileId::Managed(1), "fetch"),
        ]
    );
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[test]
fn queued_login_runs_between_selected_and_remaining_initial_fetch() {
    let provider = FakeProfileProvider::with_steps([
        ProviderStep {
            waits_for_release: true,
            ..fetch_step(UsageProfileId::Managed(1))
        },
        ProviderStep {
            operation: "login",
            result: ProviderResult::Login(Ok(true)),
            waits_for_release: false,
        },
        fetch_step(UsageProfileId::System),
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system(), managed_context(1)],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_calls(1);

    service.login(UsageProfileId::System, no_opener()).unwrap();
    provider.release_one();
    provider.wait_for_completed(3);

    assert_eq!(
        provider.calls(),
        vec![
            (UsageProfileId::Managed(1), "fetch"),
            (UsageProfileId::System, "login"),
            (UsageProfileId::System, "fetch"),
        ]
    );
    assert_eq!(provider.max_active(), 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    let events = loop {
        let events = service.take_events();
        if !events.is_empty() {
            break events;
        }
        assert!(Instant::now() < deadline, "login event did not arrive");
        thread::yield_now();
    };
    assert_eq!(
        events,
        vec![ProfilePollEvent::LoginFinished {
            id: UsageProfileId::System,
            result: Ok(true),
        }]
    );
    service.stop();
}

#[test]
fn timeout_preserves_only_the_target_profiles_last_good_value() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::Managed(1)),
        fetch_step(UsageProfileId::System),
        ProviderStep {
            operation: "fetch",
            result: ProviderResult::Fetch(Err(UsageError::RpcTimeout)),
            waits_for_release: false,
        },
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system(), managed_context(1)],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(2);

    service.refresh_selected(PollTrigger::Manual).unwrap();
    provider.wait_for_completed(3);

    let deadline = Instant::now() + Duration::from_secs(2);
    let managed = loop {
        let snapshot = service.snapshot(UsageProfileId::Managed(1)).unwrap();
        if snapshot.last_error == Some(UsageError::RpcTimeout) {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "profile timeout state was not applied"
        );
        thread::yield_now();
    };
    let system = service.snapshot(UsageProfileId::System).unwrap();
    assert_eq!(
        managed
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .map(|window| window.used_percent),
        Some(1.0)
    );
    assert_eq!(managed.last_error, Some(UsageError::RpcTimeout));
    assert_eq!(
        system
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .map(|window| window.used_percent),
        Some(20.0)
    );
    assert_eq!(system.last_error, None);
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[test]
fn manual_cooldown_is_global_across_profile_selection() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::Managed(1)),
        fetch_step(UsageProfileId::System),
        fetch_step(UsageProfileId::Managed(1)),
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system(), managed_context(1)],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(2);

    service.refresh_selected(PollTrigger::Manual).unwrap();
    service.select(UsageProfileId::System).unwrap();
    service.refresh_selected(PollTrigger::Manual).unwrap();
    provider.wait_for_completed(3);
    thread::sleep(Duration::from_millis(100));

    assert_eq!(provider.calls().len(), 3);
    assert_eq!(service.selected_id(), UsageProfileId::System);
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[test]
fn quiesce_emits_after_current_profile_work_returns() {
    let provider = FakeProfileProvider::with_steps([ProviderStep {
        waits_for_release: true,
        ..fetch_step(UsageProfileId::Managed(1))
    }]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![managed_context(1)],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_calls(1);

    service.quiesce(UsageProfileId::Managed(1)).unwrap();
    assert!(service.take_events().is_empty());
    provider.release_one();
    provider.wait_for_completed(1);
    let deadline = Instant::now() + Duration::from_secs(2);
    let events = loop {
        let events = service.take_events();
        if !events.is_empty() {
            break events;
        }
        assert!(Instant::now() < deadline, "quiesce event did not arrive");
        thread::yield_now();
    };

    assert_eq!(
        events,
        vec![ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
            1
        ))]
    );
    assert!(service
        .snapshot(UsageProfileId::Managed(1))
        .is_some_and(|snapshot| !snapshot.is_fetching));
    service.stop();
}

#[test]
fn resume_reenables_automatic_and_manual_polling() {
    let provider = FakeProfileProvider::with_steps([
        ProviderStep {
            waits_for_release: true,
            ..fetch_step(UsageProfileId::System)
        },
        fetch_step(UsageProfileId::Managed(1)),
        fetch_step(UsageProfileId::Managed(1)),
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system(), managed_context(1)],
        UsageProfileId::System,
        5,
        false,
    )
    .unwrap();
    provider.wait_for_calls(1);

    service.quiesce(UsageProfileId::Managed(1)).unwrap();
    provider.release_one();
    provider.wait_for_completed(1);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !service
        .take_events()
        .contains(&ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
            1,
        )))
    {
        assert!(Instant::now() < deadline, "quiesce event did not arrive");
        thread::yield_now();
    }
    service.resume(UsageProfileId::Managed(1)).unwrap();
    provider.wait_for_completed(2);
    let deadline = Instant::now() + Duration::from_secs(2);
    let after_automatic = loop {
        let snapshot = service.snapshot(UsageProfileId::Managed(1)).unwrap();
        if snapshot
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .is_some_and(|window| window.used_percent == 1.0)
        {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "resumed profile usage was not applied"
        );
        thread::yield_now();
    };
    assert_eq!(
        after_automatic
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .map(|window| window.used_percent),
        Some(1.0)
    );

    service.select(UsageProfileId::Managed(1)).unwrap();
    service.refresh_selected(PollTrigger::Manual).unwrap();
    provider.wait_for_completed(3);
    assert_eq!(
        provider.calls(),
        vec![
            (UsageProfileId::System, "fetch"),
            (UsageProfileId::Managed(1), "fetch"),
            (UsageProfileId::Managed(1), "fetch"),
        ]
    );
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[test]
fn resume_preserves_last_good_and_backoff_state() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::Managed(1)),
        fetch_step(UsageProfileId::System),
        ProviderStep {
            operation: "fetch",
            result: ProviderResult::Fetch(Err(UsageError::RpcTimeout)),
            waits_for_release: false,
        },
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system(), managed_context(1)],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(2);
    let last_good_at = service
        .snapshot(UsageProfileId::Managed(1))
        .unwrap()
        .last_success_at;

    service.quiesce(UsageProfileId::Managed(1)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !service
        .take_events()
        .contains(&ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
            1,
        )))
    {
        assert!(Instant::now() < deadline, "quiesce event did not arrive");
        thread::yield_now();
    }
    service.resume(UsageProfileId::Managed(1)).unwrap();
    service.refresh_selected(PollTrigger::Manual).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let resumed = loop {
        let snapshot = service.snapshot(UsageProfileId::Managed(1)).unwrap();
        if snapshot.last_error == Some(UsageError::RpcTimeout) {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "resumed profile failure state was not applied"
        );
        thread::yield_now();
    };
    assert_eq!(resumed.last_success_at, last_good_at);
    assert_eq!(resumed.last_error, Some(UsageError::RpcTimeout));
    assert_eq!(
        resumed
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .map(|window| window.used_percent),
        Some(1.0)
    );
    service.stop();
}

#[test]
fn deleting_unselected_profile_keeps_refresh_on_authoritative_selection() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::Managed(1)),
        fetch_step(UsageProfileId::System),
        fetch_step(UsageProfileId::Managed(2)),
        fetch_step(UsageProfileId::Managed(1)),
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![
            ProfileExecutionContext::system(),
            managed_context(1),
            managed_context(2),
        ],
        UsageProfileId::Managed(1),
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(3);

    service.quiesce(UsageProfileId::Managed(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !service
        .take_events()
        .contains(&ProfilePollEvent::ProfileQuiesced(UsageProfileId::Managed(
            2,
        )))
    {
        assert!(Instant::now() < deadline, "quiesce event did not arrive");
        thread::yield_now();
    }
    service.remove(UsageProfileId::Managed(2)).unwrap();
    service.select(UsageProfileId::Managed(1)).unwrap();
    service.refresh_selected(PollTrigger::Manual).unwrap();
    provider.wait_for_completed(4);

    assert_eq!(service.selected_id(), UsageProfileId::Managed(1));
    assert_eq!(
        provider.calls().last(),
        Some(&(UsageProfileId::Managed(1), "fetch"))
    );
    service.stop();
}

#[test]
fn logout_is_serialized_and_reports_an_event() {
    let provider = FakeProfileProvider::with_steps([
        fetch_step(UsageProfileId::System),
        ProviderStep {
            operation: "logout",
            result: ProviderResult::Logout(Ok(())),
            waits_for_release: false,
        },
    ]);
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system()],
        UsageProfileId::System,
        5,
        false,
    )
    .unwrap();
    provider.wait_for_completed(1);

    service.logout(UsageProfileId::System).unwrap();
    provider.wait_for_completed(2);
    let deadline = Instant::now() + Duration::from_secs(2);
    let events = loop {
        let events = service.take_events();
        if !events.is_empty() {
            break events;
        }
        assert!(Instant::now() < deadline, "logout event did not arrive");
        thread::yield_now();
    };
    assert_eq!(
        events,
        vec![ProfilePollEvent::LogoutFinished {
            id: UsageProfileId::System,
            result: Ok(()),
        }]
    );
    assert_eq!(provider.max_active(), 1);
    service.stop();
}

#[derive(Clone, Default)]
struct CancellationProvider {
    started: Arc<(Mutex<bool>, Condvar)>,
    completed: Arc<(Mutex<bool>, Condvar)>,
}

impl CancellationProvider {
    fn wait(&self, signal: &Arc<(Mutex<bool>, Condvar)>) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (lock, changed) = &**signal;
        let mut ready = lock.lock().unwrap();
        while !*ready {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("cancellation provider signal timed out");
            let (next, timeout) = changed.wait_timeout(ready, remaining).unwrap();
            ready = next;
            assert!(
                !timeout.timed_out(),
                "cancellation provider signal timed out"
            );
        }
    }

    fn wait_started(&self) {
        self.wait(&self.started);
    }

    fn wait_completed(&self) {
        self.wait(&self.completed);
    }
}

impl ProfileAccountProvider for CancellationProvider {
    fn fetch_profile(
        &self,
        _profile: &ProfileExecutionContext,
        _allow_auth_refresh: bool,
        cancellation: OperationCancellation,
    ) -> Result<CodexUsage, UsageError> {
        let (lock, changed) = &*self.started;
        *lock.lock().unwrap() = true;
        changed.notify_all();
        while !cancellation.is_cancelled() {
            thread::yield_now();
        }
        let (lock, changed) = &*self.completed;
        *lock.lock().unwrap() = true;
        changed.notify_all();
        Err(UsageError::RequestFailed)
    }

    fn login_profile(
        &self,
        _profile: &ProfileExecutionContext,
        _open: LoginPageOpener,
        _cancellation: OperationCancellation,
    ) -> Result<bool, UsageError> {
        unreachable!()
    }

    fn logout_profile(
        &self,
        _profile: &ProfileExecutionContext,
        _cancellation: OperationCancellation,
    ) -> Result<(), UsageError> {
        unreachable!()
    }
}

#[test]
fn explicit_stop_cancels_current_operation_and_joins_worker() {
    let provider = CancellationProvider::default();
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system()],
        UsageProfileId::System,
        5,
        false,
    )
    .unwrap();
    provider.wait_started();

    service.stop();
    provider.wait_completed();
}

#[test]
fn drop_cancels_current_operation_without_waiting_for_worker_join() {
    let provider = CancellationProvider::default();
    let service = ProfilePollingService::start(
        Arc::new(provider.clone()),
        vec![ProfileExecutionContext::system()],
        UsageProfileId::System,
        5,
        false,
    )
    .unwrap();
    provider.wait_started();

    let started = Instant::now();
    drop(service);
    assert!(started.elapsed() < Duration::from_millis(100));
    provider.wait_completed();
}
