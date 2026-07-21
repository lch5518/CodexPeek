use std::{
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use crate::{codex::UsageProvider, CodexUsage, UsageError};

/// 폴링을 시작하게 하는 요청 종류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollTrigger {
    /// 정해진 주기에 따른 자동 요청입니다.
    Automatic,
    /// 사용자가 요청한 수동 새로 고침입니다.
    Manual,
    /// 사용량 창의 초기화 시각 변화입니다.
    Reset(SystemTime),
    /// 인증 갱신을 명시적으로 허용하는 요청입니다.
    ForcedAuth,
}

/// UI가 안전하게 읽을 수 있는 폴링 상태 복사본입니다.
#[derive(Clone, Debug, Default)]
pub struct PollSnapshot {
    /// 마지막 정상 사용량입니다.
    pub usage: Option<CodexUsage>,
    /// 마지막 성공 시각입니다.
    pub last_success_at: Option<SystemTime>,
    /// 마지막 오류의 안정적인 분류입니다.
    pub last_error: Option<UsageError>,
    /// 표시 중인 사용량이 오래되었는지 여부입니다.
    pub is_stale: bool,
    /// 현재 요청 진행 여부입니다.
    pub is_fetching: bool,
}

/// 잠금이나 I/O 없이 폴링 규칙을 계산하는 상태 기계입니다.
#[derive(Clone, Debug)]
pub struct PollState {
    interval: Duration,
    next_poll_at: SystemTime,
    last_manual_at: Option<SystemTime>,
    last_success_at: Option<SystemTime>,
    last_good: Option<CodexUsage>,
    last_error: Option<UsageError>,
    failure_count: usize,
    in_flight: bool,
    observed_elapsed_resets: Vec<SystemTime>,
}

impl PollState {
    /// 유효한 분 단위 주기로 상태 기계를 만듭니다.
    pub fn new(refresh_interval_minutes: u32, now: SystemTime) -> Result<Self, &'static str> {
        if !matches!(refresh_interval_minutes, 1 | 5 | 10 | 15 | 30) {
            return Err("invalid refresh interval");
        }
        Ok(Self {
            interval: Duration::from_secs(u64::from(refresh_interval_minutes) * 60),
            next_poll_at: now,
            last_manual_at: None,
            last_success_at: None,
            last_good: None,
            last_error: None,
            failure_count: 0,
            in_flight: false,
            observed_elapsed_resets: Vec::new(),
        })
    }

    /// 요청을 시작할 수 있으면 인증 갱신 허용 여부를 반환합니다.
    pub fn begin(&mut self, trigger: PollTrigger, now: SystemTime) -> Option<bool> {
        if self.in_flight {
            return None;
        }
        let should_start = match trigger {
            PollTrigger::Automatic => now >= self.next_poll_at,
            PollTrigger::Manual => {
                let permitted = self.last_manual_at.is_none_or(|previous| {
                    elapsed_at_least(now, previous, Duration::from_secs(10))
                });
                if permitted {
                    self.last_manual_at = Some(now);
                }
                permitted
            }
            PollTrigger::ForcedAuth => true,
            PollTrigger::Reset(reset_at) if reset_at > now => {
                if reset_at < self.next_poll_at {
                    self.next_poll_at = reset_at;
                }
                false
            }
            PollTrigger::Reset(reset_at) => {
                if self.observed_elapsed_resets.contains(&reset_at) {
                    false
                } else {
                    self.observed_elapsed_resets.push(reset_at);
                    true
                }
            }
        };
        if !should_start {
            return None;
        }
        self.in_flight = true;
        Some(matches!(trigger, PollTrigger::ForcedAuth))
    }

    /// 완료 결과를 반영하고 다음 자동 요청 시각을 계산합니다.
    pub fn finish(&mut self, result: Result<Option<CodexUsage>, ()>, now: SystemTime) {
        self.in_flight = false;
        match result {
            Ok(usage) => {
                self.last_success_at = Some(now);
                if let Some(usage) = usage {
                    self.last_good = Some(usage);
                }
                self.failure_count = 0;
                self.last_error = None;
                self.next_poll_at = now + self.interval;
            }
            Err(()) => {
                self.failure_count = self.failure_count.saturating_add(1);
                let minutes = [1_u64, 2, 4, 8, 15][self.failure_count.saturating_sub(1).min(4)];
                self.next_poll_at = now + Duration::from_secs(minutes * 60);
            }
        }
    }

    /// 다음 자동 요청 예정 시각을 반환합니다.
    pub fn next_poll_at(&self) -> SystemTime {
        self.next_poll_at
    }

    /// 현재 시각 기준의 표시용 상태를 반환합니다.
    pub fn snapshot(&self, now: SystemTime) -> PollSnapshot {
        let stale_after = self
            .interval
            .saturating_mul(2)
            .max(Duration::from_secs(600));
        PollSnapshot {
            usage: self.last_good.clone(),
            last_success_at: self.last_success_at,
            last_error: self.last_error,
            is_stale: self
                .last_success_at
                .is_some_and(|at| elapsed_at_least(now, at, stale_after)),
            is_fetching: self.in_flight,
        }
    }
}

fn elapsed_at_least(now: SystemTime, previous: SystemTime, duration: Duration) -> bool {
    now.duration_since(previous)
        .is_ok_and(|elapsed| elapsed >= duration)
}

enum PollCommand {
    Trigger(PollTrigger),
    Stop,
}

/// 별도 스레드에서 공급자를 호출하는 폴링 서비스입니다.
pub struct PollingService {
    sender: mpsc::Sender<PollCommand>,
    snapshot: Arc<Mutex<PollSnapshot>>,
    worker: Option<JoinHandle<()>>,
}

impl PollingService {
    /// 공급자와 설정으로 폴링 워커를 시작합니다.
    pub fn start(
        provider: Arc<dyn UsageProvider>,
        refresh_interval_minutes: u32,
        auto_auth_refresh: bool,
    ) -> Result<Self, &'static str> {
        let initial = SystemTime::now();
        let state = PollState::new(refresh_interval_minutes, initial)?;
        let snapshot = Arc::new(Mutex::new(state.snapshot(initial)));
        let shared_snapshot = Arc::clone(&snapshot);
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            worker_loop(
                provider,
                state,
                auto_auth_refresh,
                receiver,
                shared_snapshot,
            )
        });
        Ok(Self {
            sender,
            snapshot,
            worker: Some(worker),
        })
    }

    /// 수동 새로 고침을 요청합니다.
    pub fn refresh(&self) {
        let _ = self.sender.send(PollCommand::Trigger(PollTrigger::Manual));
    }

    /// 인증 갱신을 허용하는 새로 고침을 요청합니다.
    pub fn refresh_with_auth(&self) {
        let _ = self
            .sender
            .send(PollCommand::Trigger(PollTrigger::ForcedAuth));
    }

    /// UI가 읽을 수 있는 최신 상태를 반환합니다.
    pub fn snapshot(&self) -> PollSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    /// 워커를 종료하고 리소스를 정리합니다.
    pub fn stop(mut self) {
        let _ = self.sender.send(PollCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for PollingService {
    fn drop(&mut self) {
        let _ = self.sender.send(PollCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    provider: Arc<dyn UsageProvider>,
    mut state: PollState,
    auto_auth_refresh: bool,
    receiver: mpsc::Receiver<PollCommand>,
    snapshot: Arc<Mutex<PollSnapshot>>,
) {
    loop {
        let now = SystemTime::now();
        let timeout = state.next_poll_at().duration_since(now).unwrap_or_default();
        let trigger = match receiver.recv_timeout(timeout) {
            Ok(PollCommand::Trigger(trigger)) => trigger,
            Ok(PollCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => PollTrigger::Automatic,
        };
        let now = SystemTime::now();
        let Some(forced_auth) = state.begin(trigger, now) else {
            continue;
        };
        let allow_auth_refresh = auto_auth_refresh || forced_auth;
        *snapshot.lock().unwrap_or_else(|error| error.into_inner()) = state.snapshot(now);
        let result = provider.fetch(allow_auth_refresh);
        let now = SystemTime::now();
        let result_for_state = result.clone().map(Some).map_err(|_| ());
        state.last_error = result.err();
        state.finish(result_for_state, now);
        *snapshot.lock().unwrap_or_else(|error| error.into_inner()) = state.snapshot(now);
    }
}
