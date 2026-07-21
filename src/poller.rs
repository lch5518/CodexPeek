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
    pending_resets: Vec<SystemTime>,
    handled_resets: Vec<SystemTime>,
}

impl PollState {
    /// 유효한 분 단위 주기로 상태 기계를 만듭니다.
    ///
    /// `refresh_interval_minutes`는 1, 5, 10, 15, 30 중 하나여야 하며, `now`를 첫 자동 요청 시각으로
    /// 사용합니다. 다른 값은 상태를 만들지 않고 오류를 반환합니다.
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
            pending_resets: Vec::new(),
            handled_resets: Vec::new(),
        })
    }

    /// 요청을 시작할 수 있으면 강제 인증 갱신 여부를 반환합니다.
    ///
    /// `trigger`와 `now`에 따라 중복 실행 및 10초 이내 수동 요청을 거절하면 `None`을 반환합니다.
    /// `Some(true)`는 호출자가 자동 정책과 별개로 인증 갱신을 허용해야 함을 뜻합니다.
    pub fn begin(&mut self, trigger: PollTrigger, now: SystemTime) -> Option<bool> {
        if self.in_flight {
            return None;
        }
        let should_start = match trigger {
            PollTrigger::Automatic => {
                let due = now >= self.next_poll_at;
                if due {
                    self.mark_due_resets_handled(now);
                }
                due
            }
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
            PollTrigger::Reset(reset_at) => {
                self.register_reset(reset_at, now);
                false
            }
        };
        if !should_start {
            return None;
        }
        self.in_flight = true;
        Some(matches!(trigger, PollTrigger::ForcedAuth))
    }

    /// 완료 결과를 반영하고 다음 자동 요청 시각을 계산합니다.
    ///
    /// 성공한 `result`만 마지막 정상 사용량과 성공 시각을 갱신합니다. 실패는 마지막 정상 값을 보존하고
    /// 1/2/4/8/15분 백오프를 적용합니다.
    pub fn finish(&mut self, result: Result<CodexUsage, UsageError>, now: SystemTime) {
        self.in_flight = false;
        match result {
            Ok(usage) => {
                let reported_resets = [
                    usage.primary.as_ref().and_then(|window| window.resets_at),
                    usage.secondary.as_ref().and_then(|window| window.resets_at),
                ];
                self.last_success_at = Some(now);
                self.last_good = Some(usage);
                self.failure_count = 0;
                self.last_error = None;
                self.next_poll_at = now + self.interval;
                self.pending_resets.clear();
                for reset_at in reported_resets.into_iter().flatten() {
                    self.register_reset(reset_at, now);
                }
            }
            Err(error) => {
                self.failure_count = self.failure_count.saturating_add(1);
                self.last_error = Some(error);
                let minutes = [1_u64, 2, 4, 8, 15][self.failure_count.saturating_sub(1).min(4)];
                self.next_poll_at = now + Duration::from_secs(minutes * 60);
                self.schedule_pending_reset(now);
            }
        }
    }

    fn register_reset(&mut self, reset_at: SystemTime, now: SystemTime) {
        if self.handled_resets.contains(&reset_at) || self.pending_resets.contains(&reset_at) {
            return;
        }
        self.pending_resets.push(reset_at);
        self.schedule_pending_reset(now);
    }

    fn schedule_pending_reset(&mut self, now: SystemTime) {
        let Some(earliest) = self.pending_resets.iter().copied().min() else {
            return;
        };
        let reset_due = if earliest <= now { now } else { earliest };
        if reset_due < self.next_poll_at {
            self.next_poll_at = reset_due;
        }
    }

    fn mark_due_resets_handled(&mut self, now: SystemTime) {
        let mut still_pending = Vec::with_capacity(self.pending_resets.len());
        for reset_at in self.pending_resets.drain(..) {
            if reset_at <= now {
                if !self.handled_resets.contains(&reset_at) {
                    self.handled_resets.push(reset_at);
                }
            } else {
                still_pending.push(reset_at);
            }
        }
        self.pending_resets = still_pending;
    }

    /// 다음 자동 요청 예정 시각을 반환합니다.
    pub fn next_poll_at(&self) -> SystemTime {
        self.next_poll_at
    }

    /// 자동 갱신 간격을 바꾸고 다음 예약 시각을 새 간격 기준으로 다시 계산합니다.
    ///
    /// `refresh_interval_minutes`가 1, 5, 10, 15, 30이 아니면 상태를 변경하지 않고 오류를 반환합니다.
    pub fn set_refresh_interval(
        &mut self,
        refresh_interval_minutes: u32,
        now: SystemTime,
    ) -> Result<(), &'static str> {
        if !matches!(refresh_interval_minutes, 1 | 5 | 10 | 15 | 30) {
            return Err("invalid refresh interval");
        }
        self.interval = Duration::from_secs(u64::from(refresh_interval_minutes) * 60);
        self.next_poll_at = now + self.interval;
        self.schedule_pending_reset(now);
        Ok(())
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
    SetRefreshInterval(u32),
    SetAutoAuthRefresh(bool),
    Stop,
}

/// 별도 스레드에서 공급자를 호출하는 폴링 서비스입니다.
pub struct PollingService {
    sender: mpsc::Sender<PollCommand>,
    state: Arc<Mutex<PollState>>,
    worker: Option<JoinHandle<()>>,
}

impl PollingService {
    /// 공급자와 설정으로 폴링 워커를 시작합니다.
    ///
    /// `provider` 호출은 상태 잠금 밖에서 실행되며, `auto_auth_refresh`는 자동 요청의 인증 갱신 정책입니다.
    /// 유효하지 않은 갱신 간격이면 워커를 시작하지 않고 오류를 반환합니다.
    pub fn start(
        provider: Arc<dyn UsageProvider>,
        refresh_interval_minutes: u32,
        auto_auth_refresh: bool,
    ) -> Result<Self, &'static str> {
        let initial = SystemTime::now();
        let state = Arc::new(Mutex::new(PollState::new(
            refresh_interval_minutes,
            initial,
        )?));
        let worker_state = Arc::clone(&state);
        let (sender, receiver) = mpsc::channel();
        let worker =
            thread::spawn(move || worker_loop(provider, worker_state, auto_auth_refresh, receiver));
        Ok(Self {
            sender,
            state,
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

    /// 작업자를 중단하거나 기다리지 않고 새 자동 갱신 간격을 전달합니다.
    pub fn set_refresh_interval(&self, minutes: u32) -> Result<(), &'static str> {
        if !matches!(minutes, 1 | 5 | 10 | 15 | 30) {
            return Err("invalid refresh interval");
        }
        self.sender
            .send(PollCommand::SetRefreshInterval(minutes))
            .map_err(|_| "polling worker stopped")
    }

    /// 작업자를 중단하거나 기다리지 않고 자동 인증 갱신 정책을 전달합니다.
    pub fn set_auto_auth_refresh(&self, enabled: bool) -> Result<(), &'static str> {
        self.sender
            .send(PollCommand::SetAutoAuthRefresh(enabled))
            .map_err(|_| "polling worker stopped")
    }

    /// UI가 읽을 수 있는 최신 상태를 반환합니다.
    pub fn snapshot(&self) -> PollSnapshot {
        self.snapshot_at(SystemTime::now())
    }

    /// 지정한 현재 시각을 기준으로 최신 폴링 상태를 계산합니다.
    pub fn snapshot_at(&self, now: SystemTime) -> PollSnapshot {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot(now)
    }

    /// 워커를 종료하고 리소스를 정리합니다.
    ///
    /// 공급자 호출이 진행 중이면 공급자의 제한된 호출 시간이 끝날 때까지 기다릴 수 있습니다.
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
        // 공급자 호출은 자체 제한 시간과 자식 프로세스 정리를 소유합니다. UI 종료에서는 작업자를
        // 분리해 즉시 반환하고, 작업자는 진행 중 호출이 끝난 뒤 대기 중인 Stop을 처리해 종료합니다.
        drop(self.worker.take());
    }
}

fn worker_loop(
    provider: Arc<dyn UsageProvider>,
    state: Arc<Mutex<PollState>>,
    mut auto_auth_refresh: bool,
    receiver: mpsc::Receiver<PollCommand>,
) {
    loop {
        let now = SystemTime::now();
        let timeout = state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .next_poll_at()
            .duration_since(now)
            .unwrap_or_default();
        let trigger = match receiver.recv_timeout(timeout) {
            Ok(PollCommand::Trigger(trigger)) => trigger,
            Ok(PollCommand::SetRefreshInterval(minutes)) => {
                let _ = state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .set_refresh_interval(minutes, SystemTime::now());
                continue;
            }
            Ok(PollCommand::SetAutoAuthRefresh(enabled)) => {
                auto_auth_refresh = enabled;
                continue;
            }
            Ok(PollCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => PollTrigger::Automatic,
        };
        let now = SystemTime::now();
        let forced_auth = state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .begin(trigger, now);
        let Some(forced_auth) = forced_auth else {
            continue;
        };
        let allow_auth_refresh = auto_auth_refresh || forced_auth;
        let result = provider.fetch(allow_auth_refresh);
        let now = SystemTime::now();
        state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finish(result, now);
    }
}
