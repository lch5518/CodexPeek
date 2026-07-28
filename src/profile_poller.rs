use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc, Arc, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use crate::{
    codex::{LoginPageOpener, OperationCancellation, ProfileAccountProvider},
    PollSnapshot, PollState, PollTrigger, ProfileExecutionContext, UsageError, UsageProfileId,
};

const MANUAL_REFRESH_COOLDOWN: Duration = Duration::from_secs(10);

/// 프로필 계정 작업이 직렬 워커에서 완료되었음을 알리는 이벤트입니다.
///
/// 로그인과 로그아웃 결과에는 안전하게 분류된 오류만 포함됩니다. 프로필 정지 이벤트는 해당
/// 프로필에서 이미 진행 중이던 작업이 반환되고 자동 스케줄에서 제외된 뒤에 발생합니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfilePollEvent {
    /// 지정한 프로필의 브라우저 로그인이 완료되었습니다.
    LoginFinished {
        /// 작업 대상 프로필의 안정적인 식별자입니다.
        id: UsageProfileId,
        /// 로그인 완료 여부 또는 안전한 실패 분류입니다.
        result: Result<bool, UsageError>,
    },
    /// 지정한 프로필의 로그아웃이 완료되었습니다.
    LogoutFinished {
        /// 작업 대상 프로필의 안정적인 식별자입니다.
        id: UsageProfileId,
        /// 성공 여부 또는 안전한 실패 분류입니다.
        result: Result<(), UsageError>,
    },
    /// 프로필이 자동 조회 대상에서 제외되어 저장소 삭제를 진행할 수 있습니다.
    ProfileQuiesced(UsageProfileId),
}

enum ProfilePollCommand {
    Select(UsageProfileId),
    RefreshSelected(PollTrigger),
    Add(ProfileExecutionContext),
    Quiesce(UsageProfileId),
    Resume(UsageProfileId),
    Remove(UsageProfileId),
    Login(UsageProfileId, LoginPageOpener),
    Logout(UsageProfileId),
    SetRefreshInterval(u32),
    SetAutoAuthRefresh(bool),
    Stop,
}

struct SharedProfileState {
    contexts: Vec<ProfileExecutionContext>,
    states: HashMap<UsageProfileId, PollState>,
    quiesced: HashSet<UsageProfileId>,
    selected: UsageProfileId,
    events: Vec<ProfilePollEvent>,
}

struct WorkerLifecycle {
    stopping: bool,
    current: OperationCancellation,
}

/// 여러 사용량 프로필의 조회와 계정 작업을 하나의 백그라운드 워커에서 직렬화합니다.
///
/// 명령 메서드는 외부 I/O를 기다리지 않고 워커 채널에 요청만 전달합니다. 상태 조회는 짧은
/// 메모리 잠금만 사용하며, 실제 공급자 호출 중에도 차단되지 않습니다. 모든 프로필은 독립적인
/// `PollState`를 유지하지만 수동 새로 고침의 10초 제한은 서비스 전체에서 공유합니다.
pub struct ProfilePollingService {
    sender: mpsc::Sender<ProfilePollCommand>,
    shared: Arc<Mutex<SharedProfileState>>,
    lifecycle: Arc<Mutex<WorkerLifecycle>>,
    worker: Option<JoinHandle<()>>,
}

impl ProfilePollingService {
    /// 초기 프로필 목록과 선택 상태로 직렬 폴링 워커를 시작합니다.
    ///
    /// `contexts`의 순서는 동일 시각에 예약된 프로필의 안정적인 우선순위로 사용되고,
    /// `selected`는 같은 시각이면 항상 먼저 조회됩니다. 갱신 간격은 1, 5, 10, 15, 30분만
    /// 허용합니다. 빈 목록, 중복 식별자, 목록에 없는 선택 식별자는 워커를 만들지 않고
    /// 오류를 반환합니다.
    pub fn start(
        provider: Arc<dyn ProfileAccountProvider>,
        contexts: Vec<ProfileExecutionContext>,
        selected: UsageProfileId,
        refresh_interval_minutes: u32,
        auto_auth_refresh: bool,
    ) -> Result<Self, &'static str> {
        if contexts.is_empty() {
            return Err("at least one profile is required");
        }

        let initial = SystemTime::now();
        let mut states = HashMap::with_capacity(contexts.len());
        for context in &contexts {
            if states
                .insert(
                    context.id(),
                    PollState::new(refresh_interval_minutes, initial)?,
                )
                .is_some()
            {
                return Err("duplicate profile id");
            }
        }
        if !states.contains_key(&selected) {
            return Err("selected profile is missing");
        }

        let shared = Arc::new(Mutex::new(SharedProfileState {
            contexts,
            states,
            quiesced: HashSet::new(),
            selected,
            events: Vec::new(),
        }));
        let lifecycle = Arc::new(Mutex::new(WorkerLifecycle {
            stopping: false,
            current: OperationCancellation::default(),
        }));
        let (sender, receiver) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker_lifecycle = Arc::clone(&lifecycle);
        let worker = thread::spawn(move || {
            worker_loop(
                provider,
                worker_shared,
                worker_lifecycle,
                refresh_interval_minutes,
                auto_auth_refresh,
                receiver,
            )
        });

        Ok(Self {
            sender,
            shared,
            lifecycle,
            worker: Some(worker),
        })
    }

    /// 선택 프로필 변경을 워커에 전달하며 외부 I/O 완료를 기다리지 않습니다.
    pub fn select(&self, id: UsageProfileId) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Select(id))
    }

    /// 현재 선택된 프로필의 조회 트리거를 워커에 전달합니다.
    ///
    /// 수동 트리거는 프로필별 `PollState`에 도달하기 전에 서비스 전체 10초 제한을 적용합니다.
    /// 이 메서드는 조회 결과를 기다리지 않습니다.
    pub fn refresh_selected(&self, trigger: PollTrigger) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::RefreshSelected(trigger))
    }

    /// 새 실행 컨텍스트를 안정적인 프로필 순서의 끝에 추가합니다.
    ///
    /// 워커가 명령을 처리하면 새 프로필은 즉시 첫 자동 조회 대상이 됩니다. 중복 식별자는 기존
    /// 상태를 보존하기 위해 무시됩니다.
    pub fn add(&self, context: ProfileExecutionContext) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Add(context))
    }

    /// 프로필을 자동 조회 스케줄에서 제외하도록 요청합니다.
    ///
    /// 같은 프로필 작업이 진행 중이면 공급자가 반환한 뒤 상태를 정지하고
    /// `ProfilePollEvent::ProfileQuiesced`를 내보냅니다.
    pub fn quiesce(&self, id: UsageProfileId) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Quiesce(id))
    }

    /// 정지된 프로필을 기존 폴링 상태 그대로 자동 조회 대상에 복귀시킵니다.
    ///
    /// `id`의 `PollState`, 마지막 정상 사용량, 오류 백오프와 다음 예약 시각은 변경하지 않고
    /// quiesce 표시만 제거합니다. 존재하지 않거나 정지되지 않은 프로필은 안전한 no-op입니다.
    pub fn resume(&self, id: UsageProfileId) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Resume(id))
    }

    /// 정지된 프로필의 실행 컨텍스트와 폴링 상태를 워커에서 제거합니다.
    pub fn remove(&self, id: UsageProfileId) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Remove(id))
    }

    /// 지정한 프로필의 브라우저 로그인을 직렬 워커에 예약합니다.
    ///
    /// `open`은 공급자가 검증한 로그인 주소를 여는 콜백이며, 결과는 이벤트 큐에 기록됩니다.
    /// 호출 스레드는 로그인 완료를 기다리지 않습니다.
    pub fn login(&self, id: UsageProfileId, open: LoginPageOpener) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Login(id, open))
    }

    /// 지정한 프로필의 로그아웃을 직렬 워커에 예약하고 즉시 반환합니다.
    pub fn logout(&self, id: UsageProfileId) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::Logout(id))
    }

    /// 모든 활성 프로필의 자동 갱신 간격을 변경하도록 요청합니다.
    ///
    /// 1, 5, 10, 15, 30분 이외 값은 명령을 보내지 않고 오류를 반환합니다.
    pub fn set_refresh_interval(&self, minutes: u32) -> Result<(), &'static str> {
        if !matches!(minutes, 1 | 5 | 10 | 15 | 30) {
            return Err("invalid refresh interval");
        }
        self.send(ProfilePollCommand::SetRefreshInterval(minutes))
    }

    /// 자동 조회에서 인증 갱신을 허용할지 워커에 전달하고 즉시 반환합니다.
    pub fn set_auto_auth_refresh(&self, enabled: bool) -> Result<(), &'static str> {
        self.send(ProfilePollCommand::SetAutoAuthRefresh(enabled))
    }

    /// 지정한 프로필의 현재 폴링 상태를 현재 시각 기준으로 복사합니다.
    ///
    /// 존재하지 않거나 이미 제거된 프로필이면 `None`을 반환합니다. 공급자 I/O 중에도 짧은 상태
    /// 잠금만 사용합니다.
    pub fn snapshot(&self, id: UsageProfileId) -> Option<PollSnapshot> {
        self.snapshot_at(id, SystemTime::now())
    }

    /// 지정한 시각을 기준으로 프로필의 현재 폴링 상태를 복사합니다.
    pub fn snapshot_at(&self, id: UsageProfileId, now: SystemTime) -> Option<PollSnapshot> {
        lock(&self.shared)
            .states
            .get(&id)
            .map(|state| state.snapshot(now))
    }

    /// 현재 선택된 프로필의 폴링 상태를 현재 시각 기준으로 복사합니다.
    pub fn selected_snapshot(&self) -> Option<PollSnapshot> {
        self.selected_snapshot_at(SystemTime::now())
    }

    /// 지정한 시각을 기준으로 현재 선택된 프로필의 폴링 상태를 복사합니다.
    pub fn selected_snapshot_at(&self, now: SystemTime) -> Option<PollSnapshot> {
        let shared = lock(&self.shared);
        shared
            .states
            .get(&shared.selected)
            .map(|state| state.snapshot(now))
    }

    /// 워커가 마지막으로 적용한 선택 프로필 식별자를 반환합니다.
    pub fn selected_id(&self) -> UsageProfileId {
        lock(&self.shared).selected
    }

    /// 지금까지 완료된 계정 작업 이벤트를 가져오고 내부 큐를 비웁니다.
    pub fn take_events(&self) -> Vec<ProfilePollEvent> {
        std::mem::take(&mut lock(&self.shared).events)
    }

    /// 현재 공급자 작업을 취소하고 직렬 워커가 종료될 때까지 기다립니다.
    ///
    /// 공급자 계약의 제한 시간 또는 취소 처리 안에서만 기다리며, 진행 중인 app-server 자식
    /// 프로세스 정리는 공급자가 소유합니다.
    pub fn stop(mut self) {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn send(&self, command: ProfilePollCommand) -> Result<(), &'static str> {
        self.sender
            .send(command)
            .map_err(|_| "profile polling worker stopped")
    }

    fn request_stop(&self) {
        let mut lifecycle = lock(&self.lifecycle);
        lifecycle.stopping = true;
        lifecycle.current.cancel();
        drop(lifecycle);
        let _ = self.sender.send(ProfilePollCommand::Stop);
    }
}

impl Drop for ProfilePollingService {
    fn drop(&mut self) {
        self.request_stop();
        // 공급자 작업은 자체 제한 시간과 취소 시 자식 프로세스 정리를 보장합니다. UI 종료는 워커
        // 조인을 기다리지 않고, 취소된 작업이 반환되면 대기 중인 Stop을 처리해 스스로 종료합니다.
        drop(self.worker.take());
    }
}

fn worker_loop(
    provider: Arc<dyn ProfileAccountProvider>,
    shared: Arc<Mutex<SharedProfileState>>,
    lifecycle: Arc<Mutex<WorkerLifecycle>>,
    mut refresh_interval_minutes: u32,
    mut auto_auth_refresh: bool,
    receiver: mpsc::Receiver<ProfilePollCommand>,
) {
    let mut last_manual_at = None;
    loop {
        match receiver.try_recv() {
            Ok(command) => {
                if !handle_command(
                    command,
                    provider.as_ref(),
                    &shared,
                    &lifecycle,
                    &mut refresh_interval_minutes,
                    &mut auto_auth_refresh,
                    &mut last_manual_at,
                ) {
                    break;
                }
                continue;
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        let Some((id, due_at)) = next_scheduled_profile(&shared) else {
            match receiver.recv() {
                Ok(command) => {
                    if !handle_command(
                        command,
                        provider.as_ref(),
                        &shared,
                        &lifecycle,
                        &mut refresh_interval_minutes,
                        &mut auto_auth_refresh,
                        &mut last_manual_at,
                    ) {
                        break;
                    }
                }
                Err(_) => break,
            }
            continue;
        };

        let now = SystemTime::now();
        if due_at <= now {
            execute_fetch(
                id,
                PollTrigger::Automatic,
                provider.as_ref(),
                &shared,
                &lifecycle,
                auto_auth_refresh,
                &mut last_manual_at,
            );
            continue;
        }

        match receiver.recv_timeout(due_at.duration_since(now).unwrap_or_default()) {
            Ok(command) => {
                if !handle_command(
                    command,
                    provider.as_ref(),
                    &shared,
                    &lifecycle,
                    &mut refresh_interval_minutes,
                    &mut auto_auth_refresh,
                    &mut last_manual_at,
                ) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_command(
    command: ProfilePollCommand,
    provider: &dyn ProfileAccountProvider,
    shared: &Arc<Mutex<SharedProfileState>>,
    lifecycle: &Arc<Mutex<WorkerLifecycle>>,
    refresh_interval_minutes: &mut u32,
    auto_auth_refresh: &mut bool,
    last_manual_at: &mut Option<SystemTime>,
) -> bool {
    match command {
        ProfilePollCommand::Select(id) => {
            let mut state = lock(shared);
            if state.states.contains_key(&id) && !state.quiesced.contains(&id) {
                state.selected = id;
            }
        }
        ProfilePollCommand::RefreshSelected(trigger) => {
            let id = lock(shared).selected;
            execute_fetch(
                id,
                trigger,
                provider,
                shared,
                lifecycle,
                *auto_auth_refresh,
                last_manual_at,
            );
        }
        ProfilePollCommand::Add(context) => {
            let mut state = lock(shared);
            let inserted = match state.states.entry(context.id()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    if let Ok(poll_state) =
                        PollState::new(*refresh_interval_minutes, SystemTime::now())
                    {
                        entry.insert(poll_state);
                        true
                    } else {
                        false
                    }
                }
                std::collections::hash_map::Entry::Occupied(_) => false,
            };
            if inserted {
                state.contexts.push(context);
            }
        }
        ProfilePollCommand::Quiesce(id) => {
            let mut state = lock(shared);
            if state.states.contains_key(&id) {
                state.quiesced.insert(id);
                state.events.push(ProfilePollEvent::ProfileQuiesced(id));
            }
        }
        ProfilePollCommand::Resume(id) => {
            lock(shared).quiesced.remove(&id);
        }
        ProfilePollCommand::Remove(id) => {
            let mut state = lock(shared);
            state.contexts.retain(|context| context.id() != id);
            state.states.remove(&id);
            state.quiesced.remove(&id);
            if state.selected == id {
                if let Some(next) = state
                    .contexts
                    .iter()
                    .map(ProfileExecutionContext::id)
                    .find(|candidate| !state.quiesced.contains(candidate))
                {
                    state.selected = next;
                }
            }
        }
        ProfilePollCommand::Login(id, open) => {
            let result = match context_for_operation(shared, id) {
                Some(context) => match begin_operation(lifecycle) {
                    Some(cancellation) => provider.login_profile(&context, open, cancellation),
                    None => Err(UsageError::RequestFailed),
                },
                None => Err(UsageError::RequestFailed),
            };
            lock(shared)
                .events
                .push(ProfilePollEvent::LoginFinished { id, result });
        }
        ProfilePollCommand::Logout(id) => {
            let result = match context_for_operation(shared, id) {
                Some(context) => match begin_operation(lifecycle) {
                    Some(cancellation) => provider.logout_profile(&context, cancellation),
                    None => Err(UsageError::RequestFailed),
                },
                None => Err(UsageError::RequestFailed),
            };
            lock(shared)
                .events
                .push(ProfilePollEvent::LogoutFinished { id, result });
        }
        ProfilePollCommand::SetRefreshInterval(minutes) => {
            let now = SystemTime::now();
            let mut state = lock(shared);
            for poll_state in state.states.values_mut() {
                let _ = poll_state.set_refresh_interval(minutes, now);
            }
            *refresh_interval_minutes = minutes;
        }
        ProfilePollCommand::SetAutoAuthRefresh(enabled) => *auto_auth_refresh = enabled,
        ProfilePollCommand::Stop => return false,
    }
    true
}

fn execute_fetch(
    id: UsageProfileId,
    trigger: PollTrigger,
    provider: &dyn ProfileAccountProvider,
    shared: &Arc<Mutex<SharedProfileState>>,
    lifecycle: &Arc<Mutex<WorkerLifecycle>>,
    auto_auth_refresh: bool,
    last_manual_at: &mut Option<SystemTime>,
) {
    let now = SystemTime::now();
    let operation = {
        let mut state = lock(shared);
        if state.quiesced.contains(&id) {
            return;
        }
        let Some(context) = state
            .contexts
            .iter()
            .find(|context| context.id() == id)
            .cloned()
        else {
            return;
        };
        if matches!(trigger, PollTrigger::Manual) {
            if last_manual_at
                .is_some_and(|previous| !elapsed_at_least(now, previous, MANUAL_REFRESH_COOLDOWN))
            {
                return;
            }
            *last_manual_at = Some(now);
        }
        let Some(forced_auth) = state
            .states
            .get_mut(&id)
            .and_then(|poll| poll.begin(trigger, now))
        else {
            return;
        };
        (context, forced_auth)
    };

    let Some(cancellation) = begin_operation(lifecycle) else {
        return;
    };
    let result =
        provider.fetch_profile(&operation.0, auto_auth_refresh || operation.1, cancellation);
    if let Some(poll_state) = lock(shared).states.get_mut(&id) {
        poll_state.finish(result, SystemTime::now());
    }
}

fn context_for_operation(
    shared: &Arc<Mutex<SharedProfileState>>,
    id: UsageProfileId,
) -> Option<ProfileExecutionContext> {
    let state = lock(shared);
    if state.quiesced.contains(&id) {
        return None;
    }
    state
        .contexts
        .iter()
        .find(|context| context.id() == id)
        .cloned()
}

fn begin_operation(lifecycle: &Arc<Mutex<WorkerLifecycle>>) -> Option<OperationCancellation> {
    let mut lifecycle = lock(lifecycle);
    if lifecycle.stopping {
        return None;
    }
    let cancellation = OperationCancellation::default();
    lifecycle.current = cancellation.clone();
    Some(cancellation)
}

fn next_scheduled_profile(
    shared: &Arc<Mutex<SharedProfileState>>,
) -> Option<(UsageProfileId, SystemTime)> {
    let state = lock(shared);
    state
        .contexts
        .iter()
        .enumerate()
        .filter_map(|(index, context)| {
            let id = context.id();
            if state.quiesced.contains(&id) {
                return None;
            }
            state.states.get(&id).map(|poll_state| {
                (
                    id,
                    poll_state.next_poll_at(),
                    usize::from(id != state.selected),
                    index,
                )
            })
        })
        .min_by_key(|(_, due_at, selected_priority, index)| (*due_at, *selected_priority, *index))
        .map(|(id, due_at, _, _)| (id, due_at))
}

fn elapsed_at_least(now: SystemTime, previous: SystemTime, duration: Duration) -> bool {
    now.duration_since(previous)
        .is_ok_and(|elapsed| elapsed >= duration)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
