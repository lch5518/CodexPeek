use std::{
    collections::{HashMap, VecDeque},
    sync::{mpsc, Arc, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use crate::{
    CodexUsage, ForecastEngine, ForecastPolicy, ForecastResult, SafeDiagnostic, UsageHistory,
    UsageHistoryOperation, UsageHistoryStore, UsageProfileId, UsageSample, UsageSampleSink,
    WindowKind,
};

const COMMAND_CAPACITY: usize = 64;
const SAVE_DEBOUNCE: Duration = Duration::from_secs(10);
const CONTROL_WAKE_INTERVAL: Duration = Duration::from_millis(50);

/// 성공한 조회 표본을 비동기로 보관하고 창별 예측을 캐시하는 서비스입니다.
///
/// 표본 전달과 UI 조회는 파일 I/O를 수행하지 않습니다. 이력 읽기·저장은 전용 worker에서
/// 실행하며, 저장 실패는 다음 dirty 저장 시도까지 보존되고 폴링 또는 마지막 정상 표시에 영향을
/// 주지 않습니다. 종료 시 저장 완료와 worker join이 필요하면 반드시 [`Self::stop`]을 호출해야
/// 하며, `Drop`은 UI 종료를 막지 않도록 종료 요청만 전달합니다.
pub struct UsageForecastService {
    samples: mpsc::SyncSender<ForecastSample>,
    controls: mpsc::Sender<ForecastControl>,
    shared: Arc<Mutex<ForecastShared>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct ForecastShared {
    enabled: bool,
    policy: ForecastPolicy,
    active: HashMap<UsageProfileId, u64>,
    generations: HashMap<UsageProfileId, u64>,
    cache: HashMap<(UsageProfileId, WindowKind), CachedForecast>,
    diagnostics: Vec<SafeDiagnostic>,
}

#[derive(Clone)]
struct CachedForecast {
    observed_at: SystemTime,
    result: ForecastResult,
}

struct ForecastSample {
    profile_id: UsageProfileId,
    generation: u64,
    usage: CodexUsage,
    observed_at: SystemTime,
}

enum ForecastControl {
    Add {
        active: HashMap<UsageProfileId, u64>,
    },
    Remove {
        profile_id: UsageProfileId,
        active: HashMap<UsageProfileId, u64>,
    },
    Clear {
        active: HashMap<UsageProfileId, u64>,
    },
    SetEnabled {
        active: HashMap<UsageProfileId, u64>,
    },
    Stop,
}

/// `UsageForecastService`를 폴링 성공 표본 sink로 사용합니다.
impl UsageSampleSink for UsageForecastService {
    fn record_success(&self, id: UsageProfileId, usage: &CodexUsage, observed_at: SystemTime) {
        let generation = {
            let shared = lock(&self.shared);
            shared
                .enabled
                .then(|| shared.active.get(&id).copied())
                .flatten()
        };
        if let Some(generation) = generation {
            let _ = self.samples.try_send(ForecastSample {
                profile_id: id,
                generation,
                usage: usage.clone(),
                observed_at,
            });
        }
    }
}

impl UsageForecastService {
    /// 지정 저장소와 활성 프로필로 예측 worker를 시작합니다.
    ///
    /// 기존 이력은 worker가 시작 시 읽습니다. 로드 실패는 안전 진단으로만 남기고 빈 이력으로
    /// 계속 실행합니다. `policy`는 캐시 신선도와 순수 예측 계산에 사용됩니다.
    pub fn start(
        store: UsageHistoryStore,
        active_profiles: impl IntoIterator<Item = UsageProfileId>,
        policy: ForecastPolicy,
    ) -> Self {
        let active = active_profiles
            .into_iter()
            .map(|id| (id, 0_u64))
            .collect::<HashMap<_, _>>();
        let shared = Arc::new(Mutex::new(ForecastShared {
            enabled: true,
            policy,
            generations: active.clone(),
            active,
            cache: HashMap::new(),
            diagnostics: Vec::new(),
        }));
        let (samples, sample_receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (controls, control_receiver) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            worker_loop(
                store,
                policy,
                worker_shared,
                sample_receiver,
                control_receiver,
            )
        });
        Self {
            samples,
            controls,
            shared,
            worker: Mutex::new(Some(worker)),
        }
    }

    /// 새 프로필을 이력 수집·예측 대상으로 등록합니다.
    pub fn add_profile(&self, profile_id: UsageProfileId) {
        let active = {
            let mut shared = lock(&self.shared);
            let generation = *shared.generations.entry(profile_id).or_insert(0);
            shared.active.insert(profile_id, generation);
            shared.active.clone()
        };
        let _ = self.controls.send(ForecastControl::Add { active });
    }

    /// 지정 프로필의 표본과 캐시를 제거하고 늦게 도착한 표본을 무효화합니다.
    pub fn remove_profile(&self, profile_id: UsageProfileId) {
        let active = {
            let mut shared = lock(&self.shared);
            let _generation = *shared
                .generations
                .entry(profile_id)
                .and_modify(|generation| *generation = generation.saturating_add(1))
                .or_insert(1);
            shared.active.remove(&profile_id);
            shared.cache.retain(|(id, _), _| *id != profile_id);
            shared.active.clone()
        };
        let _ = self
            .controls
            .send(ForecastControl::Remove { profile_id, active });
    }

    /// 모든 이력과 캐시를 제거하고 이미 큐에 있던 표본을 무효화합니다.
    pub fn clear_all(&self) {
        let active = {
            let mut shared = lock(&self.shared);
            for generation in shared.active.values_mut() {
                *generation = generation.saturating_add(1);
            }
            for (profile_id, generation) in shared.active.clone() {
                shared.generations.insert(profile_id, generation);
            }
            shared.cache.clear();
            shared.active.clone()
        };
        let _ = self.controls.send(ForecastControl::Clear { active });
    }

    /// 예측 수집과 표시를 함께 켜거나 끕니다.
    ///
    /// 비활성화하면 기존 캐시를 즉시 숨기며, 다시 켜도 새 성공 표본이 도착하기 전에는 오래된
    /// 캐시를 재사용하지 않습니다.
    pub fn set_enabled(&self, enabled: bool) {
        let active = {
            let mut shared = lock(&self.shared);
            if shared.enabled == enabled {
                return;
            }
            shared.enabled = enabled;
            for generation in shared.active.values_mut() {
                *generation = generation.saturating_add(1);
            }
            for (profile_id, generation) in shared.active.clone() {
                shared.generations.insert(profile_id, generation);
            }
            shared.cache.clear();
            shared.active.clone()
        };
        let _ = self.controls.send(ForecastControl::SetEnabled { active });
    }

    /// 지정 시각에서 여전히 신선한 프로필·창 예측을 복사합니다.
    ///
    /// 캐시 읽기는 파일 I/O를 하지 않습니다. 마지막 성공 표본이 정책의 신선도 한계를 넘으면
    /// 오래된 계산 결과 대신 [`ForecastResult::Stale`]를 반환합니다.
    pub fn forecast_at(
        &self,
        profile_id: UsageProfileId,
        window_kind: WindowKind,
        now: SystemTime,
    ) -> Option<ForecastResult> {
        let shared = lock(&self.shared);
        if !shared.enabled || !shared.active.contains_key(&profile_id) {
            return None;
        }
        shared.cache.get(&(profile_id, window_kind)).map(|cached| {
            if now
                .duration_since(cached.observed_at)
                .map_or(true, |age| age > shared.policy.stale_after())
            {
                ForecastResult::Stale
            } else {
                cached.result.clone()
            }
        })
    }

    /// worker에서 발생한 민감정보 없는 이력 진단을 가져옵니다.
    pub fn take_diagnostics(&self) -> Vec<SafeDiagnostic> {
        std::mem::take(&mut lock(&self.shared).diagnostics)
    }

    /// 보류된 저장을 완료하고 worker 종료를 기다립니다.
    pub fn stop(&self) {
        let _ = self.controls.send(ForecastControl::Stop);
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
    }
}

impl Drop for UsageForecastService {
    fn drop(&mut self) {
        let _ = self.controls.send(ForecastControl::Stop);
        drop(lock(&self.worker).take());
    }
}

fn worker_loop(
    store: UsageHistoryStore,
    policy: ForecastPolicy,
    shared: Arc<Mutex<ForecastShared>>,
    sample_receiver: mpsc::Receiver<ForecastSample>,
    control_receiver: mpsc::Receiver<ForecastControl>,
) {
    let now = SystemTime::now();
    let mut history = match store.load(now) {
        Ok(history) => history,
        Err(_) => {
            diagnostic(&shared, UsageHistoryOperation::Load);
            UsageHistory::default()
        }
    };
    let mut active = lock(&shared).active.clone();
    let mut pending = VecDeque::with_capacity(COMMAND_CAPACITY);
    let mut dirty = false;
    let mut save_due: Option<SystemTime> = None;

    loop {
        let mut stop = false;
        while let Ok(control) = control_receiver.try_recv() {
            match control {
                ForecastControl::Add {
                    active: next_active,
                } => active = next_active,
                ForecastControl::Remove {
                    profile_id,
                    active: next_active,
                } => {
                    active = next_active;
                    history.remove_profile(profile_id);
                    dirty = true;
                    save_due = Some(SystemTime::now());
                    remove_cache(&shared, profile_id);
                }
                ForecastControl::Clear {
                    active: next_active,
                } => {
                    active = next_active;
                    history.clear();
                    dirty = true;
                    save_due = Some(SystemTime::now());
                    lock(&shared).cache.clear();
                }
                ForecastControl::SetEnabled {
                    active: next_active,
                } => active = next_active,
                ForecastControl::Stop => stop = true,
            }
        }
        drain_pending_samples(
            &mut pending,
            &active,
            &mut history,
            &mut dirty,
            &mut save_due,
            policy,
            &shared,
        );
        if stop {
            while let Ok(sample) = sample_receiver.try_recv() {
                defer_or_process_sample(
                    sample,
                    &active,
                    &mut pending,
                    &mut history,
                    &mut dirty,
                    &mut save_due,
                    policy,
                    &shared,
                );
            }
            drain_pending_samples(
                &mut pending,
                &active,
                &mut history,
                &mut dirty,
                &mut save_due,
                policy,
                &shared,
            );
            save_on_shutdown(&store, &history, dirty, &shared);
            break;
        }

        let timeout = save_due
            .and_then(|due| due.duration_since(SystemTime::now()).ok())
            .unwrap_or(CONTROL_WAKE_INTERVAL)
            .min(CONTROL_WAKE_INTERVAL);
        match sample_receiver.recv_timeout(timeout) {
            Ok(sample) => defer_or_process_sample(
                sample,
                &active,
                &mut pending,
                &mut history,
                &mut dirty,
                &mut save_due,
                policy,
                &shared,
            ),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if dirty && save_due.is_some_and(|due| due <= SystemTime::now()) {
                    if store.save(&history, SystemTime::now()).is_ok() {
                        dirty = false;
                        save_due = None;
                    } else {
                        diagnostic(&shared, UsageHistoryOperation::Save);
                        save_due = SystemTime::now().checked_add(SAVE_DEBOUNCE);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                save_on_shutdown(&store, &history, dirty, &shared);
                break;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn defer_or_process_sample(
    sample: ForecastSample,
    active: &HashMap<UsageProfileId, u64>,
    pending: &mut VecDeque<ForecastSample>,
    history: &mut UsageHistory,
    dirty: &mut bool,
    save_due: &mut Option<SystemTime>,
    policy: ForecastPolicy,
    shared: &Arc<Mutex<ForecastShared>>,
) {
    if !sample_is_current(shared, sample.profile_id, sample.generation) {
        return;
    }
    if active.get(&sample.profile_id) != Some(&sample.generation) {
        if pending.len() < COMMAND_CAPACITY {
            pending.push_back(sample);
        }
        return;
    }
    process_current_sample(sample, history, dirty, save_due, policy, shared);
}

#[allow(clippy::too_many_arguments)]
fn drain_pending_samples(
    pending: &mut VecDeque<ForecastSample>,
    active: &HashMap<UsageProfileId, u64>,
    history: &mut UsageHistory,
    dirty: &mut bool,
    save_due: &mut Option<SystemTime>,
    policy: ForecastPolicy,
    shared: &Arc<Mutex<ForecastShared>>,
) {
    let mut deferred = VecDeque::with_capacity(pending.len());
    while let Some(sample) = pending.pop_front() {
        if !sample_is_current(shared, sample.profile_id, sample.generation) {
            continue;
        }
        if active.get(&sample.profile_id) == Some(&sample.generation) {
            process_current_sample(sample, history, dirty, save_due, policy, shared);
        } else {
            deferred.push_back(sample);
        }
    }
    *pending = deferred;
}

fn process_current_sample(
    sample: ForecastSample,
    history: &mut UsageHistory,
    dirty: &mut bool,
    save_due: &mut Option<SystemTime>,
    policy: ForecastPolicy,
    shared: &Arc<Mutex<ForecastShared>>,
) {
    if record_usage(
        history,
        sample.profile_id,
        &sample.usage,
        sample.observed_at,
        shared,
    ) {
        *dirty = true;
        *save_due = SystemTime::now().checked_add(SAVE_DEBOUNCE);
    }
    cache_usage(
        history,
        sample.profile_id,
        sample.generation,
        &sample.usage,
        sample.observed_at,
        policy,
        shared,
    );
}

fn sample_is_current(
    shared: &Arc<Mutex<ForecastShared>>,
    profile_id: UsageProfileId,
    generation: u64,
) -> bool {
    let state = lock(shared);
    state.enabled && state.active.get(&profile_id) == Some(&generation)
}

fn save_on_shutdown(
    store: &UsageHistoryStore,
    history: &UsageHistory,
    dirty: bool,
    shared: &Arc<Mutex<ForecastShared>>,
) {
    if dirty && store.save(history, SystemTime::now()).is_err() {
        diagnostic(shared, UsageHistoryOperation::Save);
    }
}

fn record_usage(
    history: &mut UsageHistory,
    profile_id: UsageProfileId,
    usage: &CodexUsage,
    observed_at: SystemTime,
    shared: &Arc<Mutex<ForecastShared>>,
) -> bool {
    let mut added = false;
    for window in [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
    {
        let Ok(sample) = UsageSample::new(
            profile_id,
            window.kind,
            window.used_percent,
            window.resets_at,
            observed_at,
            observed_at,
        ) else {
            diagnostic(shared, UsageHistoryOperation::Record);
            continue;
        };
        match history.record(sample, observed_at) {
            Ok(record) => added |= record.is_added(),
            Err(_) => diagnostic(shared, UsageHistoryOperation::Record),
        }
    }
    added
}

fn cache_usage(
    history: &UsageHistory,
    profile_id: UsageProfileId,
    generation: u64,
    usage: &CodexUsage,
    observed_at: SystemTime,
    policy: ForecastPolicy,
    shared: &Arc<Mutex<ForecastShared>>,
) {
    for window in [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
    {
        let samples = history
            .samples_for(profile_id, window.kind)
            .cloned()
            .collect::<Vec<_>>();
        let result =
            ForecastEngine::calculate(&samples, window, observed_at, observed_at, false, &policy);
        let mut state = lock(shared);
        if state.enabled && state.active.get(&profile_id) == Some(&generation) {
            state.cache.insert(
                (profile_id, window.kind),
                CachedForecast {
                    observed_at,
                    result,
                },
            );
        }
    }
}

fn remove_cache(shared: &Arc<Mutex<ForecastShared>>, profile_id: UsageProfileId) {
    lock(shared).cache.retain(|(id, _), _| *id != profile_id);
}

fn diagnostic(shared: &Arc<Mutex<ForecastShared>>, operation: UsageHistoryOperation) {
    lock(shared)
        .diagnostics
        .push(SafeDiagnostic::UsageHistory { operation });
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
