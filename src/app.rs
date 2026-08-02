//! 런타임 서비스와 Windows UI를 조합하는 애플리케이션 계층입니다.

use std::{
    collections::HashSet,
    ffi::OsString,
    io,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    aggregate_profile_diagnostics,
    codex::{locate_supported_cli, AppServerUsageProvider},
    diagnose_profile_contexts,
    domain::{reset_credits_label, reset_unavailable_label, ResetDateTime},
    inspect_settings_for_diagnostics, localized_text,
    windows::{
        autostart::{set_autostart, WindowsRegistry},
        initial_widget_visible, native, profile_taskbar_tooltip, resolve_windows_language, taskbar,
        time::local_reset_time,
        ForecastView, LaunchMode, ProfileUsageStatus, UiAction, UiBackend, UiSettings,
        UsageProfileView, UsageRowView, WidgetDataState, WidgetViewModel,
    },
    AsyncDiagnosticWriter, CorrelatedProfileSettingsEvent, DiagnosticCode, DiagnosticLogger,
    ForecastResult, Language, LanguagePreference, LocalizationKey, NativeProfileFileSystem,
    PollSnapshot, PollTrigger, ProfileDiagnosticRun, ProfileDiagnosticSnapshot,
    ProfileExecutionContext, ProfilePollEvent, ProfilePollingService, ProfileSettingsMutation,
    ProfileSettingsOperation, ProfileSettingsRequestId, ProfileSettingsService,
    ProfileValidationError, ResetCredits, SafeDiagnostic, Settings, SettingsStore,
    UpdateCheckIntent, UpdateCheckNotice, UpdateCheckStart, UpdateChecker, UpdatePresentation,
    UpdatePresentationStatus, UpdateUserAction, UreqHttpClient, UsageError, UsageForecastService,
    UsageHistoryStore, UsageProfileId, UsageProfileRoot, UsageWindow, WindowKind,
};

/// 명령줄 모드에 따라 진단 또는 네이티브 애플리케이션을 실행합니다.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> io::Result<()> {
    let arguments = arguments
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mode = LaunchMode::parse(arguments.iter())
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    if mode == LaunchMode::Diagnose {
        native::attach_parent_console();
        return run_safe_diagnostics(true).map(|_| ());
    }

    let _instance_guard = native::acquire_single_instance()?;
    let store = SettingsStore::new();
    let settings = store.load()?;
    let startup_hidden =
        !initial_widget_visible(mode, settings.startup_view, settings.widget_visible)
            && settings.widget_visible;
    let mut runtime = AppRuntime::new(store, settings, startup_hidden)?;
    runtime.start_automatic_update_check();
    let result = native::run(&mut runtime);
    runtime.shutdown();
    result
}

/// 프로필 설정의 내구성 이벤트와 계정 워커 이벤트 사이의 순서를 조정하는 명령입니다.
///
/// 명령에는 검증된 숫자 식별자와 실행 컨텍스트만 포함됩니다. 프로필 이름은 설정 저장 명령에만
/// 일시적으로 전달되며 진단이나 계정 상태에는 보관되지 않습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileRuntimeCommand {
    /// 순서 보장 설정 워커에 제출할 변경입니다.
    Settings(ProfileSettingsMutation),
    /// 내구성 있게 추가된 프로필 컨텍스트를 폴링 워커에 등록합니다.
    AddPollContext(ProfileExecutionContext),
    /// 폴링 워커의 표시 대상을 변경합니다.
    SelectPoll(UsageProfileId),
    /// 현재 선택 프로필을 지정한 정책으로 갱신합니다.
    RefreshSelected(PollTrigger),
    /// 지정한 프로필 로그인을 폴링 워커에 제출합니다.
    Login(UsageProfileId),
    /// 지정한 프로필 로그아웃을 폴링 워커에 제출합니다.
    Logout(UsageProfileId),
    /// 삭제 전에 지정한 프로필의 진행 중 작업을 정지합니다.
    Quiesce(UsageProfileId),
    /// 삭제 저장 실패 뒤 기존 프로필 폴링 상태를 다시 활성화합니다.
    Resume(UsageProfileId),
    /// 내구성 삭제가 끝난 프로필 상태를 폴링 워커에서 제거합니다.
    Remove(UsageProfileId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingProfileOperation {
    Settings {
        operation: ProfileSettingsOperation,
        request_id: Option<ProfileSettingsRequestId>,
        login_after_add: bool,
    },
    Login(UsageProfileId),
    Logout(UsageProfileId),
    DeleteQuiesce(UsageProfileId),
    DeleteSettings {
        id: UsageProfileId,
        request_id: Option<ProfileSettingsRequestId>,
    },
}

/// 프로필 UI 요청과 두 직렬 워커의 완료 이벤트를 순수 상태 전이로 조정합니다.
///
/// 입력은 검증된 `Settings`, 앱 전용 `UsageProfileRoot`, 형식화된 완료 이벤트입니다. 반환 명령은
/// 호출자가 비동기 worker에 제출해야 하며 이 타입 자체는 파일·네트워크·Codex 인증 I/O를 하지
/// 않습니다. 설정 성공 이벤트 전에는 렌더링 대상 설정을 바꾸지 않습니다.
pub struct ProfileRuntimeState {
    settings: Settings,
    root: UsageProfileRoot,
    pending: Option<PendingProfileOperation>,
    login_required: HashSet<UsageProfileId>,
}

impl ProfileRuntimeState {
    /// 저장된 설정과 앱 전용 프로필 루트에서 런타임 조정 상태를 생성합니다.
    ///
    /// 디스크나 인증 파일을 읽지 않으며, 선택 상태는 전달된 설정이 내구성 있게 저장된 값이라고
    /// 가정합니다.
    pub fn new(settings: Settings, root: UsageProfileRoot) -> Self {
        Self {
            settings,
            root,
            pending: None,
            login_required: HashSet::new(),
        }
    }

    /// 마지막으로 내구성 성공 이벤트가 반영된 설정을 반환합니다.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// 프로필 변경 또는 계정 작업의 완료를 기다리는지 반환합니다.
    pub fn mutation_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// 지정한 프로필에서 최근 로그인 취소 또는 인증 실패가 있었는지 반환합니다.
    pub fn login_required(&self, id: UsageProfileId) -> bool {
        self.login_required.contains(&id)
    }

    /// 새 프로필 이름을 검증하고 설정 worker의 add 명령만 생성합니다.
    ///
    /// 프로필 컨텍스트는 `Added` 성공 이벤트 이후에만 생성되며, 브라우저 로그인은 예약하지
    /// 않습니다. 확인된 로그인까지 요청하려면 `request_add_with_login_confirmation`을 사용합니다.
    pub fn request_add(
        &mut self,
        label: String,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.request_add_with_login_confirmation(label, false)
    }

    /// 새 프로필을 추가하고 명시적으로 확인된 경우에만 저장 성공 뒤 로그인을 예약합니다.
    ///
    /// `login_confirmed`가 거짓이어도 프로필 추가는 계속되며 로그인 필요 상태로 남습니다. 참인 경우
    /// 에도 설정 저장 성공 전에는 실행 컨텍스트나 로그인 명령을 만들지 않습니다.
    pub fn request_add_with_login_confirmation(
        &mut self,
        label: String,
        login_confirmed: bool,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        let mut catalog = self.settings.usage_profiles.clone();
        let normalized = catalog.add(&label)?.label().to_owned();
        self.pending = Some(PendingProfileOperation::Settings {
            operation: ProfileSettingsOperation::Add,
            request_id: None,
            login_after_add: login_confirmed,
        });
        Ok(vec![ProfileRuntimeCommand::Settings(
            ProfileSettingsMutation::Add { label: normalized },
        )])
    }

    /// 시스템 또는 관리 프로필 이름 변경을 검증해 설정 worker 명령을 생성합니다.
    ///
    /// 검증된 이름만 내구성 설정 worker에 전달하며 성공 이벤트 전에는 로컬 표시 이름, 선택 ID,
    /// 실행 컨텍스트 또는 CLI·IDE 계정 상태를 변경하지 않습니다.
    pub fn request_rename(
        &mut self,
        id: UsageProfileId,
        label: String,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        let mut catalog = self.settings.usage_profiles.clone();
        catalog.rename(id, &label)?;
        let normalized = match id {
            UsageProfileId::System => catalog
                .system_label()
                .ok_or(ProfileValidationError::InvalidId)?
                .to_owned(),
            UsageProfileId::Managed(_) => catalog
                .managed()
                .iter()
                .find(|profile| profile.id() == id)
                .ok_or(ProfileValidationError::InvalidId)?
                .label()
                .to_owned(),
        };
        self.pending = Some(PendingProfileOperation::Settings {
            operation: ProfileSettingsOperation::Rename,
            request_id: None,
            login_after_add: false,
        });
        Ok(vec![ProfileRuntimeCommand::Settings(
            ProfileSettingsMutation::Rename {
                id,
                label: normalized,
            },
        )])
    }

    /// 표시 대상 변경을 검증해 내구성 설정 명령을 생성합니다.
    ///
    /// 성공 이벤트 전에는 현재 렌더링 선택을 유지합니다.
    pub fn request_select(
        &mut self,
        id: UsageProfileId,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        let mut catalog = self.settings.usage_profiles.clone();
        catalog.select(id)?;
        self.pending = Some(PendingProfileOperation::Settings {
            operation: ProfileSettingsOperation::Select,
            request_id: None,
            login_after_add: false,
        });
        Ok(vec![ProfileRuntimeCommand::Settings(
            ProfileSettingsMutation::Select { id },
        )])
    }

    /// 삭제 대상을 검증하고 먼저 폴링 정지 명령만 생성합니다.
    ///
    /// 설정 삭제 명령은 같은 식별자의 `ProfileQuiesced` 이벤트 이후에 생성됩니다.
    pub fn request_delete(
        &mut self,
        id: UsageProfileId,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        let mut catalog = self.settings.usage_profiles.clone();
        catalog.remove(id)?;
        self.pending = Some(PendingProfileOperation::DeleteQuiesce(id));
        Ok(vec![ProfileRuntimeCommand::Quiesce(id)])
    }

    /// 확인되지 않은 기존 프로필 로그인 요청을 검증하되 작업은 생성하지 않습니다.
    ///
    /// 브라우저 계정 확인을 완료한 UI 경계는 `request_login_with_confirmation`에 `true`를 전달해야
    /// 합니다. 이 호환 메서드는 확인 우회를 막기 위해 항상 로그인 명령 없이 반환합니다.
    pub fn request_login(
        &mut self,
        id: UsageProfileId,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.request_login_with_confirmation(id, false)
    }

    /// 존재하는 프로필의 확인된 로그인을 직렬 계정 worker에 제출하도록 명령을 생성합니다.
    ///
    /// `confirmed`가 거짓이면 프로필 ID만 검증하고 상태나 명령을 바꾸지 않습니다. 참일 때만 로그인
    /// 완료를 기다리는 상태로 전이하고 `Login` 명령 하나를 반환합니다.
    pub fn request_login_with_confirmation(
        &mut self,
        id: UsageProfileId,
        confirmed: bool,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        self.validate_id(id)?;
        if !confirmed {
            return Ok(Vec::new());
        }
        self.pending = Some(PendingProfileOperation::Login(id));
        Ok(vec![ProfileRuntimeCommand::Login(id)])
    }

    /// 존재하는 프로필의 로그아웃을 직렬 계정 worker에 제출하도록 명령을 생성합니다.
    pub fn request_logout(
        &mut self,
        id: UsageProfileId,
    ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError> {
        self.require_idle()?;
        self.validate_id(id)?;
        self.pending = Some(PendingProfileOperation::Logout(id));
        Ok(vec![ProfileRuntimeCommand::Logout(id)])
    }

    /// 설정 서비스가 발급한 요청 ID를 현재 대기 중인 같은 종류의 설정 작업에 연결합니다.
    ///
    /// `operation`이 현재 작업과 다르거나 이미 요청 ID가 연결됐으면 상태를 바꾸지 않고 `false`를
    /// 반환합니다. 성공 이벤트와 실패 이벤트는 이 ID가 일치할 때만 상태를 전이할 수 있습니다.
    pub fn bind_settings_request(
        &mut self,
        request_id: ProfileSettingsRequestId,
        operation: ProfileSettingsOperation,
    ) -> bool {
        match &mut self.pending {
            Some(PendingProfileOperation::Settings {
                operation: expected,
                request_id: pending_id,
                ..
            }) if *expected == operation && pending_id.is_none() => {
                *pending_id = Some(request_id);
                true
            }
            Some(PendingProfileOperation::DeleteSettings {
                request_id: pending_id,
                ..
            }) if operation == ProfileSettingsOperation::Delete && pending_id.is_none() => {
                *pending_id = Some(request_id);
                true
            }
            _ => false,
        }
    }

    /// 설정 worker의 완료 이벤트를 반영하고 다음 폴링 명령을 반환합니다.
    ///
    /// 실패 이벤트는 기존 설정과 선택을 유지합니다. 성공한 로그인 뒤의 `Selected` 이벤트를
    /// 포함해 선택 성공은 폴링 선택과 강제 인증 갱신을 순서대로 생성합니다.
    pub fn apply_settings_event(
        &mut self,
        event: CorrelatedProfileSettingsEvent,
    ) -> Vec<ProfileRuntimeCommand> {
        match event {
            CorrelatedProfileSettingsEvent::Added {
                request_id,
                settings,
                id,
            } if self.matches_settings_request(request_id, ProfileSettingsOperation::Add) => {
                let login_after_add = matches!(
                    self.pending,
                    Some(PendingProfileOperation::Settings {
                        operation: ProfileSettingsOperation::Add,
                        request_id: Some(expected_id),
                        login_after_add: true,
                    }) if expected_id == request_id
                );
                self.settings.usage_profiles = settings.usage_profiles;
                self.login_required.insert(id);
                ProfileExecutionContext::managed(&self.root, id)
                    .map(|context| {
                        let mut commands = vec![ProfileRuntimeCommand::AddPollContext(context)];
                        if login_after_add {
                            self.pending = Some(PendingProfileOperation::Login(id));
                            commands.push(ProfileRuntimeCommand::Login(id));
                        } else {
                            self.pending = None;
                        }
                        commands
                    })
                    .unwrap_or_else(|_| {
                        self.pending = None;
                        Vec::new()
                    })
            }
            CorrelatedProfileSettingsEvent::Renamed {
                request_id,
                settings,
                ..
            } if self.matches_settings_request(request_id, ProfileSettingsOperation::Rename) => {
                self.settings.usage_profiles = settings.usage_profiles;
                self.pending = None;
                Vec::new()
            }
            CorrelatedProfileSettingsEvent::Selected {
                request_id,
                settings,
                id,
            } if self.matches_settings_request(request_id, ProfileSettingsOperation::Select) => {
                self.settings.usage_profiles = settings.usage_profiles;
                self.pending = None;
                self.login_required.remove(&id);
                vec![
                    ProfileRuntimeCommand::SelectPoll(id),
                    ProfileRuntimeCommand::RefreshSelected(PollTrigger::ForcedAuth),
                ]
            }
            CorrelatedProfileSettingsEvent::Deleted {
                request_id,
                settings,
                id,
            } if self.matches_settings_request(request_id, ProfileSettingsOperation::Delete) => {
                let selected = settings.usage_profiles.selected();
                self.settings.usage_profiles = settings.usage_profiles;
                self.pending = None;
                self.login_required.remove(&id);
                vec![
                    ProfileRuntimeCommand::Remove(id),
                    ProfileRuntimeCommand::SelectPoll(selected),
                ]
            }
            CorrelatedProfileSettingsEvent::Failed {
                request_id: Some(request_id),
                operation,
                ..
            } if self.matches_settings_request(request_id, operation) => {
                let resume = match self.pending {
                    Some(PendingProfileOperation::DeleteSettings { id, .. })
                        if operation == ProfileSettingsOperation::Delete =>
                    {
                        Some(id)
                    }
                    _ => None,
                };
                self.pending = None;
                resume
                    .map(ProfileRuntimeCommand::Resume)
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// 폴링·계정 worker 완료 이벤트를 반영하고 다음 설정 명령을 반환합니다.
    ///
    /// 로그인 취소와 오류는 해당 프로필만 로그인 필요 상태로 남깁니다. 삭제는 동일 프로필의
    /// quiesce 완료가 확인된 경우에만 설정 worker로 진행됩니다.
    pub fn apply_poll_event(&mut self, event: ProfilePollEvent) -> Vec<ProfileRuntimeCommand> {
        match event {
            ProfilePollEvent::ProfileQuiesced(id)
                if self.pending == Some(PendingProfileOperation::DeleteQuiesce(id)) =>
            {
                self.pending = Some(PendingProfileOperation::DeleteSettings {
                    id,
                    request_id: None,
                });
                vec![ProfileRuntimeCommand::Settings(
                    ProfileSettingsMutation::Delete { id },
                )]
            }
            ProfilePollEvent::LoginFinished { id, result } => {
                if result == Ok(true) {
                    self.pending = Some(PendingProfileOperation::Settings {
                        operation: ProfileSettingsOperation::Select,
                        request_id: None,
                        login_after_add: false,
                    });
                    self.login_required.remove(&id);
                    vec![ProfileRuntimeCommand::Settings(
                        ProfileSettingsMutation::Select { id },
                    )]
                } else {
                    self.pending = None;
                    self.login_required.insert(id);
                    Vec::new()
                }
            }
            ProfilePollEvent::LogoutFinished { id, result } => {
                self.pending = None;
                if result.is_ok()
                    || matches!(
                        result,
                        Err(UsageError::NotLoggedIn | UsageError::AuthenticationExpired)
                    )
                {
                    self.login_required.insert(id);
                }
                Vec::new()
            }
            ProfilePollEvent::ProfileQuiesced(_) => Vec::new(),
        }
    }

    fn require_idle(&self) -> Result<(), ProfileValidationError> {
        if self.pending.is_some() {
            Err(ProfileValidationError::InvalidId)
        } else {
            Ok(())
        }
    }

    fn matches_settings_request(
        &self,
        request_id: ProfileSettingsRequestId,
        operation: ProfileSettingsOperation,
    ) -> bool {
        match self.pending {
            Some(PendingProfileOperation::Settings {
                operation: expected,
                request_id: Some(expected_id),
                ..
            }) => expected == operation && expected_id == request_id,
            Some(PendingProfileOperation::DeleteSettings {
                request_id: Some(expected_id),
                ..
            }) => operation == ProfileSettingsOperation::Delete && expected_id == request_id,
            _ => false,
        }
    }

    fn validate_id(&self, id: UsageProfileId) -> Result<(), ProfileValidationError> {
        let mut catalog = self.settings.usage_profiles.clone();
        catalog.select(id)
    }
}

struct AppRuntime {
    profile_settings: Option<ProfileSettingsService>,
    profile_poller: Option<ProfilePollingService>,
    usage_forecast: Option<Arc<UsageForecastService>>,
    profile_state: std::sync::Mutex<ProfileRuntimeState>,
    diagnostics: Option<AsyncDiagnosticWriter>,
    startup_hidden: bool,
    update_presentation: UpdatePresentation,
}

impl AppRuntime {
    fn new(store: SettingsStore, settings: Settings, startup_hidden: bool) -> io::Result<Self> {
        let usage_provider = Arc::new(AppServerUsageProvider::new());
        let root = UsageProfileRoot::new(store.root().to_path_buf());
        let history_store = UsageHistoryStore::for_root(store.root().to_path_buf());
        let (profile_settings, startup) = ProfileSettingsService::start_with_recovery(
            store,
            settings.clone(),
            NativeProfileFileSystem::default(),
        );
        let (contexts, startup_report) = startup.into_parts();
        let mut runtime_settings = settings;
        if !contexts
            .iter()
            .any(|context| context.id() == runtime_settings.usage_profiles.selected())
        {
            let _ = runtime_settings
                .usage_profiles
                .select(UsageProfileId::System);
        }
        let profile_state = ProfileRuntimeState::new(runtime_settings, root);
        let selected = profile_state.settings().usage_profiles.selected();
        let provider: Arc<dyn crate::codex::ProfileAccountProvider> = usage_provider;
        let usage_forecast = Arc::new(UsageForecastService::start(
            history_store,
            contexts.iter().map(ProfileExecutionContext::id),
            crate::ForecastPolicy::new(std::time::Duration::from_secs(
                u64::from(profile_state.settings().refresh_interval_minutes) * 60,
            )),
        ));
        usage_forecast.set_enabled(profile_state.settings().usage_forecast_enabled);
        let profile_poller = ProfilePollingService::start_with_sample_sink(
            provider,
            usage_forecast.clone(),
            contexts,
            selected,
            profile_state.settings().refresh_interval_minutes,
            profile_state.settings().auto_auth_refresh,
        )
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
        let diagnostics = AsyncDiagnosticWriter::start(DiagnosticLogger::new(), 64);
        if startup_report.recovery_failed || startup_report.validation_failed != 0 {
            let _ = diagnostics.enqueue(SafeDiagnostic::Settings { valid: false });
        }
        Ok(Self {
            profile_settings: Some(profile_settings),
            profile_poller: Some(profile_poller),
            usage_forecast: Some(usage_forecast),
            profile_state: std::sync::Mutex::new(profile_state),
            diagnostics: Some(diagnostics),
            startup_hidden,
            update_presentation: UpdatePresentation::default(),
        })
    }

    fn save_settings(&self) {
        let settings = self.settings_snapshot();
        if self.profile_settings().save_preferences(settings).is_err() {
            self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false });
        }
    }

    fn start_automatic_update_check(&mut self) {
        let Some(checker) = update_checker() else {
            return;
        };
        let now = SystemTime::now();
        let last_check = self
            .settings_snapshot()
            .last_update_check_unix
            .map(|seconds| UNIX_EPOCH + std::time::Duration::from_secs(seconds));
        if last_check.is_some_and(|checked| {
            now.duration_since(checked)
                .is_ok_and(|elapsed| elapsed < std::time::Duration::from_secs(24 * 60 * 60))
        }) {
            return;
        }
        if self
            .update_presentation
            .begin_check(UpdateCheckIntent::Automatic)
            == UpdateCheckStart::AlreadyRunning
        {
            return;
        }
        self.spawn_update_worker(checker, last_check, now);
    }

    fn handle_user_update_action(&mut self) {
        let Some(checker) = update_checker() else {
            self.update_presentation
                .queue_user_notice(UpdateCheckNotice::Failed);
            return;
        };
        match self.update_presentation.begin_user_action() {
            UpdateUserAction::Open(update) => self
                .update_presentation
                .queue_user_notice(UpdateCheckNotice::Available(update)),
            UpdateUserAction::StartCheck => {
                self.spawn_update_worker(checker, None, SystemTime::now());
            }
            UpdateUserAction::WaitForRunning => {}
        }
    }

    fn spawn_update_worker(
        &mut self,
        checker: UpdateChecker,
        last_check: Option<SystemTime>,
        now: SystemTime,
    ) {
        self.with_settings_mut(|settings| {
            settings.last_update_check_unix = now
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_secs());
        });
        self.save_settings();
        let presentation = self.update_presentation.clone();
        thread::spawn(move || {
            let result = checker.check_if_due(&UreqHttpClient, last_check, now);
            presentation.record_result(result);
        });
    }

    fn snapshot_inner(&self) -> PollSnapshot {
        let selected = self.settings_snapshot().usage_profiles.selected();
        self.profile_poller().snapshot(selected).unwrap_or_default()
    }

    fn profile_settings(&self) -> &ProfileSettingsService {
        self.profile_settings
            .as_ref()
            .expect("profile settings service is available")
    }

    fn profile_poller(&self) -> &ProfilePollingService {
        self.profile_poller
            .as_ref()
            .expect("profile polling service is available")
    }

    fn usage_forecast(&self) -> &UsageForecastService {
        self.usage_forecast
            .as_deref()
            .expect("usage forecast service is available")
    }

    fn enqueue_diagnostic(&self, event: SafeDiagnostic) {
        if let Some(writer) = self.diagnostics.as_ref() {
            let _ = writer.enqueue(event);
        }
    }

    fn settings_snapshot(&self) -> Settings {
        self.profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .settings()
            .clone()
    }

    fn with_settings_mut(&self, update: impl FnOnce(&mut Settings)) {
        let mut state = self
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        update(&mut state.settings);
    }

    fn login_required(&self) -> bool {
        let selected = self.settings_snapshot().usage_profiles.selected();
        let state_required = self
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .login_required(selected);
        state_required
            || matches!(
                self.profile_poller()
                    .snapshot(selected)
                    .and_then(|snapshot| snapshot.last_error),
                Some(UsageError::NotLoggedIn | UsageError::AuthenticationExpired)
            )
    }

    fn drain_profile_events(&self) {
        for event in self.usage_forecast().take_diagnostics() {
            self.enqueue_diagnostic(event);
        }
        for event in self.profile_settings().take_correlated_events() {
            let failed = matches!(event, CorrelatedProfileSettingsEvent::Failed { .. });
            let commands = self
                .profile_state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .apply_settings_event(event);
            if failed {
                self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false });
            }
            self.execute_profile_commands(commands);
        }
        for event in self.profile_poller().take_events() {
            let account_failed = matches!(
                &event,
                ProfilePollEvent::LoginFinished { result: Err(_), .. }
                    | ProfilePollEvent::LogoutFinished { result: Err(_), .. }
            );
            let commands = self
                .profile_state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .apply_poll_event(event);
            if account_failed {
                self.enqueue_diagnostic(SafeDiagnostic::Rpc {
                    code: DiagnosticCode::RpcFailed,
                });
            }
            self.execute_profile_commands(commands);
        }
    }

    fn execute_profile_commands(&self, commands: Vec<ProfileRuntimeCommand>) {
        for command in commands {
            let result = match command {
                ProfileRuntimeCommand::Settings(mutation) => {
                    let operation = mutation.operation();
                    self.profile_settings()
                        .submit_correlated(mutation)
                        .map_err(|_| ())
                        .and_then(|request_id| {
                            self.profile_state
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .bind_settings_request(request_id, operation)
                                .then_some(())
                                .ok_or(())
                        })
                }
                ProfileRuntimeCommand::AddPollContext(context) => {
                    let id = context.id();
                    self.profile_poller()
                        .add(context)
                        .map(|_| self.usage_forecast().add_profile(id))
                        .map_err(|_| ())
                }
                ProfileRuntimeCommand::SelectPoll(id) => {
                    self.profile_poller().select(id).map_err(|_| ())
                }
                ProfileRuntimeCommand::RefreshSelected(trigger) => self
                    .profile_poller()
                    .refresh_selected(trigger)
                    .map_err(|_| ()),
                ProfileRuntimeCommand::Login(id) => self
                    .profile_poller()
                    .login(id, Arc::new(native::open_validated_login_page))
                    .map_err(|_| ()),
                ProfileRuntimeCommand::Logout(id) => {
                    self.profile_poller().logout(id).map_err(|_| ())
                }
                ProfileRuntimeCommand::Quiesce(id) => {
                    self.profile_poller().quiesce(id).map_err(|_| ())
                }
                ProfileRuntimeCommand::Resume(id) => {
                    self.profile_poller().resume(id).map_err(|_| ())
                }
                ProfileRuntimeCommand::Remove(id) => self
                    .profile_poller()
                    .remove(id)
                    .map(|_| self.usage_forecast().remove_profile(id))
                    .map_err(|_| ()),
            };
            if result.is_err() {
                self.profile_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .pending = None;
                self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false });
                break;
            }
        }
    }

    fn submit_profile_request(
        &self,
        request: impl FnOnce(
            &mut ProfileRuntimeState,
        ) -> Result<Vec<ProfileRuntimeCommand>, ProfileValidationError>,
    ) {
        let commands = request(
            &mut self
                .profile_state
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        match commands {
            Ok(commands) => self.execute_profile_commands(commands),
            Err(_) => {
                self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false });
            }
        }
    }

    fn record_profile_diagnostics(&self) {
        let settings = self.settings_snapshot();
        let state = self
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = state.root.clone();
        let profiles = std::iter::once(UsageProfileId::System)
            .chain(
                settings
                    .usage_profiles
                    .managed()
                    .iter()
                    .map(|profile| profile.id()),
            )
            .map(|id| ProfileDiagnosticSnapshot {
                id,
                snapshot: self.profile_poller().snapshot(id),
                login_required: state.login_required(id),
            })
            .collect::<Vec<_>>();
        drop(state);
        self.enqueue_diagnostic(aggregate_profile_diagnostics(
            true, &settings, &root, &profiles,
        ));
    }

    fn shutdown(&mut self) {
        if let Some(poller) = self.profile_poller.take() {
            poller.stop();
        }
        if let Some(forecast) = self.usage_forecast.take() {
            forecast.stop();
        }
        if let Some(settings) = self.profile_settings.take() {
            let _ = settings.stop();
        }
        if let Some(writer) = self.diagnostics.take() {
            let _ = writer.stop();
        }
    }
}

impl UiBackend for AppRuntime {
    fn snapshot(&self) -> WidgetViewModel {
        self.drain_profile_events();
        let settings = self.settings_snapshot();
        let snapshot = self.snapshot_inner();
        let language = effective_language(settings.language);
        let now = SystemTime::now();
        let selected = settings.usage_profiles.selected();
        let runtime_login_required = self
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .login_required(selected);
        let status = if runtime_login_required {
            localized_text(LocalizationKey::UsageProfileLoginRequired, language).to_owned()
        } else if snapshot.is_fetching {
            localized_text(LocalizationKey::Refreshing, language).to_owned()
        } else if let Some(error) = snapshot.last_error {
            error.user_message(language).to_owned()
        } else if snapshot.is_stale {
            localized_text(LocalizationKey::Stale, language).to_owned()
        } else {
            localized_text(LocalizationKey::Polling, language).to_owned()
        };
        let status = status_with_update(status, self.update_presentation.status(), language);
        let last_success = snapshot
            .last_success_at
            .and_then(|time| now.duration_since(time).ok())
            .map(|duration| last_success_text(duration.as_secs(), language))
            .unwrap_or_default();
        let primary_forecast = forecast_view(
            self.usage_forecast()
                .forecast_at(selected, WindowKind::Primary, now),
            settings.usage_forecast_enabled,
            now,
            language,
        );
        let secondary_forecast = forecast_view(
            self.usage_forecast()
                .forecast_at(selected, WindowKind::Secondary, now),
            settings.usage_forecast_enabled,
            now,
            language,
        );
        let primary = snapshot
            .usage
            .as_ref()
            .and_then(|usage| usage.primary.as_ref())
            .map(|window| {
                row_view_with_forecast(
                    window,
                    language,
                    settings.show_remaining_percent,
                    primary_forecast.clone(),
                )
            });
        let secondary = snapshot
            .usage
            .as_ref()
            .and_then(|usage| usage.secondary.as_ref())
            .map(|window| {
                row_view_with_forecast(
                    window,
                    language,
                    settings.show_remaining_percent,
                    secondary_forecast.clone(),
                )
            });
        let weekly = secondary.as_ref().or(primary.as_ref());
        let reset_credits_text = snapshot
            .usage
            .as_ref()
            .and_then(|usage| reset_credits_text(usage.reset_credits.as_ref(), language));
        let taskbar = taskbar_copy(
            weekly,
            language,
            &status,
            settings.show_remaining_percent,
            reset_credits_text.as_deref(),
        );
        let usage_profile_label = selected_usage_profile_label(&settings, language);
        let taskbar_tooltip = profile_taskbar_tooltip(
            &usage_profile_label,
            &append_forecast_tooltip(
                &taskbar.tooltip,
                primary.as_ref(),
                secondary.as_ref(),
                language,
            ),
            language,
        );
        let data_state = data_state_for_snapshot(&snapshot, runtime_login_required);
        WidgetViewModel {
            usage_profile_label,
            primary,
            secondary,
            status,
            last_success,
            is_stale: snapshot.is_stale,
            taskbar_label: taskbar.label,
            taskbar_tooltip,
            reset_credits_text,
            data_state,
        }
    }

    fn settings(&self) -> UiSettings {
        self.drain_profile_events();
        let settings = self.settings_snapshot();
        ui_settings(
            &settings,
            self.startup_hidden,
            self.update_presentation.status(),
            self.login_required(),
            self.profile_poller(),
            &self.profile_state,
        )
    }

    fn take_update_notice(&self) -> Option<UpdateCheckNotice> {
        self.update_presentation.take_user_notice()
    }

    fn dispatch(&mut self, action: UiAction) -> UiSettings {
        self.drain_profile_events();
        let mut save_preferences = true;
        match action {
            UiAction::Refresh => {
                let _ = self.profile_poller().refresh_selected(PollTrigger::Manual);
            }
            UiAction::SetRefreshInterval(minutes) if matches!(minutes, 1 | 5 | 10 | 15 | 30) => {
                if self.settings_snapshot().refresh_interval_minutes != minutes {
                    self.with_settings_mut(|settings| {
                        settings.refresh_interval_minutes = minutes;
                    });
                    let _ = self.profile_poller().set_refresh_interval(minutes);
                }
            }
            UiAction::SetRefreshInterval(_) => {}
            UiAction::ToggleAutostart => {
                let enabled = !self.settings_snapshot().start_with_windows;
                if std::env::current_exe()
                    .and_then(|path| set_autostart(&WindowsRegistry, enabled, &path))
                    .is_ok()
                {
                    self.with_settings_mut(|settings| settings.start_with_windows = enabled);
                } else {
                    self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false });
                }
            }
            UiAction::SetStartupView(view) => {
                self.with_settings_mut(|settings| settings.startup_view = view)
            }
            UiAction::RefreshWithAuth => {
                let _ = self
                    .profile_poller()
                    .refresh_selected(PollTrigger::ForcedAuth);
            }
            UiAction::Login => {
                save_preferences = false;
            }
            UiAction::ToggleAutoAuthRefresh => {
                let enabled = !self.settings_snapshot().auto_auth_refresh;
                self.with_settings_mut(|settings| settings.auto_auth_refresh = enabled);
                let _ = self.profile_poller().set_auto_auth_refresh(enabled);
            }
            UiAction::SetLanguage(language) => {
                self.with_settings_mut(|settings| settings.language = language)
            }
            UiAction::RunDiagnostics => {
                self.record_profile_diagnostics();
                let language = effective_language(self.settings_snapshot().language);
                thread::spawn(move || {
                    if let Ok(summary) = run_safe_diagnostics(false) {
                        let (title, text) = summary.localized(language);
                        let _ = native::show_diagnostic_summary(title, &text);
                    }
                });
            }
            UiAction::CheckForUpdates => self.handle_user_update_action(),
            UiAction::ToggleWidget => {
                if self.startup_hidden {
                    self.startup_hidden = false;
                } else {
                    self.with_settings_mut(|settings| {
                        settings.widget_visible = !settings.widget_visible;
                    });
                }
            }
            UiAction::Exit => {}
            UiAction::ToggleShowRemaining => {
                self.with_settings_mut(|settings| {
                    settings.show_remaining_percent = !settings.show_remaining_percent;
                });
            }
            UiAction::ToggleUsageForecast => {
                let enabled = !self.settings_snapshot().usage_forecast_enabled;
                self.with_settings_mut(|settings| settings.usage_forecast_enabled = enabled);
                self.usage_forecast().set_enabled(enabled);
            }
            UiAction::ClearUsageHistory => {
                self.usage_forecast().clear_all();
                save_preferences = false;
            }
            UiAction::SetTaskbarDisplayMode(mode) => {
                self.with_settings_mut(|settings| settings.taskbar_display_mode = mode)
            }
            UiAction::SelectUsageProfile(id) => {
                self.submit_profile_request(|state| state.request_select(id));
                save_preferences = false;
            }
            UiAction::AddUsageProfile(label) => {
                self.submit_profile_request(|state| state.request_add(label));
                save_preferences = false;
            }
            UiAction::RenameUsageProfile(id, label) => {
                self.submit_profile_request(|state| state.request_rename(id, label));
                save_preferences = false;
            }
            UiAction::LoginUsageProfile(id) => {
                let _ = id;
                save_preferences = false;
            }
            UiAction::LogoutUsageProfile(id) => {
                self.submit_profile_request(|state| state.request_logout(id));
                save_preferences = false;
            }
            UiAction::DeleteUsageProfile(id) => {
                self.submit_profile_request(|state| state.request_delete(id));
                save_preferences = false;
            }
            UiAction::OpenAddUsageProfile | UiAction::OpenManageUsageProfiles => {
                save_preferences = false;
            }
        }
        if save_preferences {
            self.save_settings();
        }
        let settings = self.settings_snapshot();
        ui_settings(
            &settings,
            self.startup_hidden,
            self.update_presentation.status(),
            self.login_required(),
            self.profile_poller(),
            &self.profile_state,
        )
    }

    fn dispatch_confirmed_profile_login(&mut self, action: UiAction) -> UiSettings {
        self.drain_profile_events();
        match action {
            UiAction::AddUsageProfile(label) => self.submit_profile_request(|state| {
                state.request_add_with_login_confirmation(label, true)
            }),
            UiAction::Login => {
                let selected = self.settings_snapshot().usage_profiles.selected();
                self.submit_profile_request(|state| {
                    state.request_login_with_confirmation(selected, true)
                });
            }
            UiAction::LoginUsageProfile(id) => {
                self.submit_profile_request(|state| {
                    state.request_login_with_confirmation(id, true)
                });
            }
            _ => self.enqueue_diagnostic(SafeDiagnostic::Settings { valid: false }),
        }
        self.settings()
    }
}

fn data_state_for_snapshot(
    snapshot: &PollSnapshot,
    runtime_login_required: bool,
) -> WidgetDataState {
    if runtime_login_required || snapshot.last_error.is_some() {
        WidgetDataState::Error
    } else if snapshot.usage.is_none() {
        WidgetDataState::Loading
    } else {
        WidgetDataState::Ready
    }
}

fn ui_settings(
    settings: &Settings,
    startup_hidden: bool,
    update_status: UpdatePresentationStatus,
    login_required: bool,
    profile_poller: &ProfilePollingService,
    profile_state: &std::sync::Mutex<ProfileRuntimeState>,
) -> UiSettings {
    let resolved_language = effective_language(settings.language);
    let profile_state = profile_state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    UiSettings {
        widget_visible: settings.widget_visible && !startup_hidden,
        refresh_interval_minutes: settings.refresh_interval_minutes,
        start_with_windows: settings.start_with_windows,
        startup_view: settings.startup_view,
        auto_auth_refresh: settings.auto_auth_refresh,
        language: settings.language,
        resolved_language,
        taskbar_offset: settings.taskbar_offset,
        taskbar_display_mode: settings.taskbar_display_mode,
        update_status,
        show_remaining_percent: settings.show_remaining_percent,
        usage_forecast_enabled: settings.usage_forecast_enabled,
        login_required,
        usage_profiles: usage_profile_views(
            settings,
            resolved_language,
            profile_poller,
            &profile_state,
        ),
        usage_profile_mutation_pending: profile_state.mutation_pending(),
    }
}

/// 프로필 행의 안전한 사용량 요약과 진행 표시 값을 함께 전달합니다.
///
/// 이 값은 폴링 스냅샷의 표시 가능한 필드만 보관하며 계정 식별자나 인증 정보는 포함하지 않습니다.
struct ProfileUsagePresentation {
    summary: String,
    details: String,
    login_required: bool,
    used_percent: Option<u8>,
    usage_status: Option<ProfileUsageStatus>,
}

/// 사용량 창을 프로필 행의 타입 지정 진행 표시 값으로 변환합니다.
///
/// 창이 없으면 진행 값을 만들지 않으며, 유효한 창의 막대 비율은 반올림해 그리기 값으로 사용하고
/// 원본 사용률은 상태 색상 분류에 사용합니다.
fn profile_usage_presentation_for_window(window: Option<&UsageWindow>) -> ProfileUsagePresentation {
    let Some(window) = window else {
        return ProfileUsagePresentation {
            summary: String::new(),
            details: String::new(),
            login_required: false,
            used_percent: None,
            usage_status: None,
        };
    };

    ProfileUsagePresentation {
        summary: String::new(),
        details: String::new(),
        login_required: false,
        used_percent: Some(window.bar_percent().round() as u8),
        usage_status: Some(ProfileUsageStatus::from_used_percent(window.used_percent)),
    }
}

/// 한 프로필의 폴링 스냅샷을 안전한 요약과 타입 지정 진행 표시 값으로 변환합니다.
///
/// 로그인 필요 상태는 보존된 사용량이 있어도 진행 값을 숨깁니다. 일시 오류가 마지막 정상 사용량을
/// 보존한 경우에는 기존 요약 규칙과 함께 그 진행 값을 그대로 반환합니다.
fn profile_usage_presentation_for_snapshot(
    snapshot: Option<&PollSnapshot>,
    login_required: bool,
    language: Language,
) -> ProfileUsagePresentation {
    if login_required {
        return ProfileUsagePresentation {
            summary: localized_text(LocalizationKey::UsageProfileLoginRequired, language)
                .to_string(),
            details: String::new(),
            login_required: true,
            used_percent: None,
            usage_status: None,
        };
    }

    let usage = snapshot.and_then(|snapshot| snapshot.usage.as_ref());
    let window = usage.and_then(|usage| usage.secondary.as_ref().or(usage.primary.as_ref()));
    let mut presentation = profile_usage_presentation_for_window(window);
    presentation.summary = if snapshot.is_some_and(|snapshot| snapshot.is_fetching) {
        localized_text(LocalizationKey::Refreshing, language).to_string()
    } else if let Some(usage) = usage {
        profile_reset_credits_summary(usage.reset_credits.as_ref(), language)
    } else {
        localized_text(LocalizationKey::Unavailable, language).to_string()
    };
    presentation.details = if snapshot.is_some_and(|snapshot| snapshot.is_fetching) {
        String::new()
    } else if let Some(usage) = usage {
        profile_usage_details(usage, language)
    } else {
        String::new()
    };
    presentation
}

/// 리셋권 정보를 프로필 관리 행의 안전한 한 줄 요약으로 변환합니다.
///
/// 서버가 리셋권 정보를 제공하지 않으면 사용 가능 여부를 추정하지 않고 정보 없음 상태를 표시합니다.
fn profile_reset_credits_summary(credits: Option<&ResetCredits>, language: Language) -> String {
    let label = localized_text(LocalizationKey::UsageProfileResetCredits, language);
    let Some(credits) = credits else {
        return format!(
            "{label}: {}",
            localized_text(LocalizationKey::Unavailable, language)
        );
    };
    let expiry = credits
        .nearest_expiry
        .and_then(|value| local_reset_time(value).ok())
        .map(|datetime| datetime.localized_label(language));
    let count = credits.available_count;
    match expiry {
        Some(expiry) => format!(
            "{label}: {count} · {} {expiry}",
            localized_text(LocalizationKey::UsageProfileEnds, language)
        ),
        None => format!("{label}: {count}"),
    }
}

/// 단기·주간 사용량 창을 프로필 관리 행의 안전한 한 줄 요약으로 변환합니다.
///
/// 각 창은 서버가 제공한 사용률과 초기화 시각만 사용하며, 누락된 창은 표시하지 않습니다.
fn profile_usage_details(usage: &crate::CodexUsage, language: Language) -> String {
    [usage.primary.as_ref(), usage.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|window| {
            let label = match window.kind {
                WindowKind::Primary => {
                    localized_text(LocalizationKey::PrimaryWindowLabel, language)
                }
                WindowKind::Secondary => {
                    localized_text(LocalizationKey::SecondaryWindowLabel, language)
                }
            };
            let reset = window
                .resets_at
                .and_then(|value| local_reset_time(value).ok())
                .map(|datetime| datetime.localized_label(language))
                .unwrap_or_else(|| reset_unavailable_label(language).to_owned());
            format!(
                "{label} ({}): {:.0}% {} · {} {reset}",
                window.period_label(language),
                window.used_percent,
                localized_text(LocalizationKey::UsageProfileUsed, language),
                localized_text(LocalizationKey::UsageProfileEnds, language),
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn usage_profile_views(
    settings: &Settings,
    language: Language,
    profile_poller: &ProfilePollingService,
    profile_state: &ProfileRuntimeState,
) -> Vec<UsageProfileView> {
    let selected = settings.usage_profiles.selected();
    let presentation_for = |id| {
        let snapshot = profile_poller.snapshot(id);
        let login_required = profile_state.login_required(id)
            || snapshot.as_ref().is_some_and(|snapshot| {
                matches!(
                    snapshot.last_error,
                    Some(UsageError::NotLoggedIn | UsageError::AuthenticationExpired)
                )
            });
        profile_usage_presentation_for_snapshot(snapshot.as_ref(), login_required, language)
    };
    let system_presentation = presentation_for(UsageProfileId::System);
    let mut profiles = vec![UsageProfileView {
        id: UsageProfileId::System,
        label: system_profile_display_label(settings, language),
        summary: system_presentation.summary,
        details: system_presentation.details,
        selected: selected == UsageProfileId::System,
        login_required: system_presentation.login_required,
        used_percent: system_presentation.used_percent,
        usage_status: system_presentation.usage_status,
        managed: false,
    }];
    profiles.extend(settings.usage_profiles.managed().iter().map(|profile| {
        let id = profile.id();
        let presentation = presentation_for(id);
        UsageProfileView {
            id,
            label: profile.label().to_string(),
            summary: presentation.summary,
            details: presentation.details,
            selected: id == selected,
            login_required: presentation.login_required,
            used_percent: presentation.used_percent,
            usage_status: presentation.usage_status,
            managed: true,
        }
    }));
    profiles
}

fn selected_usage_profile_label(settings: &Settings, language: Language) -> String {
    match settings.usage_profiles.selected() {
        UsageProfileId::System => system_profile_display_label(settings, language),
        selected => settings
            .usage_profiles
            .managed()
            .iter()
            .find(|profile| profile.id() == selected)
            .map(|profile| profile.label().to_string())
            .unwrap_or_else(|| system_profile_display_label(settings, language)),
    }
}

/// 시스템 프로필의 사용자 지정 이름 또는 현재 언어의 기본 표시 이름을 반환합니다.
///
/// `settings`의 선택 상태나 프로필 실행 문맥은 변경하지 않으며, 사용자 지정 이름이 없을 때만
/// 지역화된 기본 이름을 사용합니다.
fn system_profile_display_label(settings: &Settings, language: Language) -> String {
    settings
        .usage_profiles
        .system_label()
        .map(str::to_owned)
        .unwrap_or_else(|| localized_text(LocalizationKey::UsageProfileSystem, language).to_owned())
}

fn update_status_key(status: UpdatePresentationStatus) -> Option<LocalizationKey> {
    match status {
        UpdatePresentationStatus::Idle => None,
        UpdatePresentationStatus::Checking => Some(LocalizationKey::UpdateChecking),
        UpdatePresentationStatus::Available => Some(LocalizationKey::UpdateAvailable),
        UpdatePresentationStatus::Current => Some(LocalizationKey::UpdateCurrent),
        UpdatePresentationStatus::Failed => Some(LocalizationKey::UpdateFailed),
    }
}

fn status_with_update(
    mut usage_status: String,
    update_status: UpdatePresentationStatus,
    language: Language,
) -> String {
    if let Some(key) = update_status_key(update_status) {
        usage_status.push_str(" · ");
        usage_status.push_str(localized_text(key, language));
    }
    usage_status
}

#[allow(dead_code)]
fn row_view(window: &UsageWindow, language: Language, show_remaining: bool) -> UsageRowView {
    let reset_time = window
        .resets_at
        .and_then(|reset_at| local_reset_time(reset_at).ok());
    row_view_with_reset_time(window, language, show_remaining, reset_time)
}

#[allow(dead_code)]
fn row_view_with_reset_time(
    window: &UsageWindow,
    language: Language,
    show_remaining: bool,
    reset_time: Option<ResetDateTime>,
) -> UsageRowView {
    row_view_with_reset_time_and_forecast(
        window,
        language,
        show_remaining,
        reset_time,
        ForecastView::Hidden,
    )
}

fn row_view_with_forecast(
    window: &UsageWindow,
    language: Language,
    show_remaining: bool,
    forecast: ForecastView,
) -> UsageRowView {
    let reset_time = window
        .resets_at
        .and_then(|reset_at| local_reset_time(reset_at).ok());
    row_view_with_reset_time_and_forecast(window, language, show_remaining, reset_time, forecast)
}

fn row_view_with_reset_time_and_forecast(
    window: &UsageWindow,
    language: Language,
    show_remaining: bool,
    reset_time: Option<ResetDateTime>,
    forecast: ForecastView,
) -> UsageRowView {
    let display_percent = if show_remaining {
        (100.0 - window.used_percent).max(0.0)
    } else {
        window.used_percent
    };
    UsageRowView {
        label: window.period_label(language),
        used_percent: window.used_percent,
        display_percent,
        percent_text: format!("{display_percent:.0}%"),
        reset_text: reset_time
            .map(|value| value.localized_label(language))
            .unwrap_or_else(|| reset_unavailable_label(language).to_owned()),
        level: window.level(),
        forecast,
    }
}

/// 순수 예측 결과를 UI가 그대로 렌더링할 수 있는 지역화 상태로 변환합니다.
///
/// `enabled`가 거짓이면 저장된 결과와 무관하게 숨김 상태를 반환합니다. 시간은 호출자가
/// 주입한 `now`와 계산된 소진 시각의 차이만 사용하며, 소수 단위는 사용자에게 노출하지
/// 않습니다.
fn forecast_view(
    result: Option<ForecastResult>,
    enabled: bool,
    now: SystemTime,
    language: Language,
) -> ForecastView {
    if !enabled {
        return ForecastView::Hidden;
    }
    match result {
        None => collecting_forecast_view(0, language),
        Some(ForecastResult::Collecting { sample_count, .. }) => {
            collecting_forecast_view(sample_count, language)
        }
        Some(ForecastResult::InsufficientActivity) => ForecastView::InsufficientActivity {
            line: localized_text(LocalizationKey::UsageForecastInsufficientActivity, language)
                .to_owned(),
        },
        Some(ForecastResult::AlreadyExhausted) => ForecastView::AlreadyExhausted {
            line: localized_text(LocalizationKey::UsageForecastExhausted, language).to_owned(),
        },
        Some(ForecastResult::Stale) => ForecastView::Stale {
            line: localized_text(LocalizationKey::UsageForecastStale, language).to_owned(),
        },
        Some(ForecastResult::Invalid) => ForecastView::Invalid {
            line: localized_text(LocalizationKey::UsageForecastInvalid, language).to_owned(),
        },
        Some(ForecastResult::ForecastAvailable(forecast)) => {
            available_forecast_view(&forecast, now, language)
        }
    }
}

fn collecting_forecast_view(sample_count: usize, language: Language) -> ForecastView {
    let line = localized_text(LocalizationKey::UsageForecastCollecting, language)
        .replace("{count}", &sample_count.to_string())
        .replace(
            "{required}",
            &crate::ForecastPolicy::MINIMUM_SAMPLES.to_string(),
        );
    ForecastView::Collecting { line }
}

fn available_forecast_view(
    forecast: &crate::Forecast,
    now: SystemTime,
    language: Language,
) -> ForecastView {
    let Some(remaining) = forecast.exhaustion_at.duration_since(now).ok() else {
        return ForecastView::Stale {
            line: localized_text(LocalizationKey::UsageForecastStale, language).to_owned(),
        };
    };
    let duration = forecast_duration_text(remaining, language);
    let Some(duration) = duration else {
        let long_term = localized_text(LocalizationKey::UsageForecastLongTerm, language).to_owned();
        let line = if forecast.exhausts_before_reset == Some(false) {
            forecast
                .expected_remaining_percent_at_reset
                .filter(|value| value.is_finite())
                .map(|value| {
                    let reset = localized_text(LocalizationKey::UsageForecastAtReset, language)
                        .replace("{percent}", &format_percent(value));
                    format!("{long_term} · {reset}")
                })
                .unwrap_or(long_term)
        } else {
            long_term
        };
        return ForecastView::ForecastAvailable { line };
    };
    let estimate = match forecast.exhausts_before_reset {
        Some(true) => localized_text(LocalizationKey::UsageForecastBeforeReset, language)
            .replace("{duration}", &duration),
        _ => localized_text(LocalizationKey::UsageForecastEstimate, language)
            .replace("{duration}", &duration),
    };
    let line = if forecast.exhausts_before_reset == Some(false) {
        forecast
            .expected_remaining_percent_at_reset
            .filter(|value| value.is_finite())
            .map(|value| {
                let reset = localized_text(LocalizationKey::UsageForecastAtReset, language)
                    .replace("{percent}", &format_percent(value));
                format!("{estimate} · {reset}")
            })
            .unwrap_or(estimate)
    } else {
        estimate
    };
    ForecastView::ForecastAvailable { line }
}

fn format_percent(value: f64) -> String {
    format!("{:.0}", value.clamp(0.0, 100.0))
}

/// 사용자에게 의미 있는 정밀도로 남은 기간을 반올림합니다.
///
/// 7일 이상은 숫자 대신 장기 예측 상태를 반환합니다. 그 외 단위는 가장 가까운 정수로
/// 반올림하고 0보다 큰 기간이 0으로 보이지 않도록 최소 1을 적용합니다.
fn forecast_duration_text(duration: Duration, language: Language) -> Option<String> {
    let seconds = duration.as_secs();
    if seconds >= 7 * 24 * 60 * 60 {
        return None;
    }
    let (value, key) = if seconds < 60 * 60 {
        (
            ((seconds + 30) / 60).max(1),
            if ((seconds + 30) / 60).max(1) == 1 {
                LocalizationKey::UsageForecastMinuteOne
            } else {
                LocalizationKey::UsageForecastMinuteOther
            },
        )
    } else if seconds < 24 * 60 * 60 {
        (
            ((seconds + 1_800) / 3_600).max(1),
            if ((seconds + 1_800) / 3_600).max(1) == 1 {
                LocalizationKey::UsageForecastHourOne
            } else {
                LocalizationKey::UsageForecastHourOther
            },
        )
    } else {
        (
            ((seconds + 43_200) / 86_400).max(1),
            if ((seconds + 43_200) / 86_400).max(1) == 1 {
                LocalizationKey::UsageForecastDayOne
            } else {
                LocalizationKey::UsageForecastDayOther
            },
        )
    };
    let unit = localized_text(key, language);
    let duration = match language {
        Language::Korean | Language::Japanese => format!("{value}{unit}"),
        _ => format!("{value} {unit}"),
    };
    Some(duration)
}

fn append_forecast_tooltip(
    tooltip: &str,
    primary: Option<&UsageRowView>,
    secondary: Option<&UsageRowView>,
    language: Language,
) -> String {
    let mut result = tooltip.to_owned();
    let mut lines = Vec::new();
    if let Some(line) = primary.and_then(|row| row.forecast.line()) {
        lines.push(format!(
            "{}: {line}",
            localized_text(LocalizationKey::PrimaryWindowLabel, language)
        ));
    }
    if let Some(line) = secondary.and_then(|row| row.forecast.line()) {
        lines.push(format!(
            "{}: {line}",
            localized_text(LocalizationKey::SecondaryWindowLabel, language)
        ));
    }
    if !lines.is_empty() {
        result.push('\n');
        result.push_str(&lines.join("\n"));
    }
    result
}

struct TaskbarCopy {
    label: String,
    tooltip: String,
}

/// 리셋권 정보를 현지화된 표시 문구로 변환합니다.
///
/// 만료 시각은 Windows 현지 시간대로 변환하여 표시하며, 변환에 실패하면 개수만 표시합니다.
/// 개수가 0이거나 정보가 없으면 `None`을 반환합니다.
fn reset_credits_text(credits: Option<&ResetCredits>, language: Language) -> Option<String> {
    let credits = credits?;
    let expiry_text = credits
        .nearest_expiry
        .and_then(|value| local_reset_time(value).ok())
        .map(|datetime| datetime.localized_label(language));
    reset_credits_label(credits.available_count, expiry_text.as_deref(), language)
}

fn taskbar_copy(
    row: Option<&UsageRowView>,
    language: Language,
    status: &str,
    show_remaining: bool,
    reset_credits: Option<&str>,
) -> TaskbarCopy {
    let label = taskbar_label(show_remaining, language).to_owned();
    let reset_line = reset_credits
        .map(|text| format!("\n{text}"))
        .unwrap_or_default();
    let tooltip = match (row, language) {
        (Some(row), Language::Korean) => format!(
            "Codex {} 사용량\n현재 사용량: {:.0}%\n남은 사용량: {:.0}%\n초기화 시각: {}{reset_line}\n상태: {} · {status}",
            row.label,
            row.used_percent,
            (100.0 - row.used_percent).max(0.0),
            row.reset_text,
            taskbar_risk_text(row.used_percent, language),
        ),
        (Some(row), Language::English) => format!(
            "Codex {} usage\nCurrent usage: {:.0}%\nRemaining: {:.0}%\nReset at: {}{reset_line}\nStatus: {} · {status}",
            row.label,
            row.used_percent,
            (100.0 - row.used_percent).max(0.0),
            row.reset_text,
            taskbar_risk_text(row.used_percent, language),
        ),
        (None, Language::Korean) => format!("Codex 사용량{reset_line}\n상태: {status}"),
        (None, Language::English) => format!("Codex usage{reset_line}\nStatus: {status}"),
        (Some(row), language) => format!(
            "{} {}\n{}: {:.0}%\n{}: {:.0}%\n{}: {}{reset_line}\n{}: {} · {status}",
            taskbar_usage_title_prefix(language),
            row.label,
            current_usage_label(language),
            row.used_percent,
            remaining_usage_label(language),
            (100.0 - row.used_percent).max(0.0),
            reset_at_label(language),
            row.reset_text,
            status_label(language),
            taskbar_risk_text(row.used_percent, language),
        ),
        (None, language) => format!(
            "{}{reset_line}\n{}: {status}",
            codex_usage_title(language),
            status_label(language)
        ),
    };
    TaskbarCopy { label, tooltip }
}

fn last_success_text(seconds: u64, language: Language) -> String {
    match language {
        Language::Korean => format!("마지막 성공 {seconds}초 전"),
        Language::English => format!("Last success {seconds}s ago"),
        Language::Spanish => format!("Último éxito hace {seconds} s"),
        Language::PortugueseBrazil => format!("Último sucesso há {seconds} s"),
        Language::Indonesian => format!("Berhasil terakhir {seconds} dtk lalu"),
        Language::Japanese => format!("最後の成功は{seconds}秒前"),
        Language::Hindi => format!("अंतिम सफलता {seconds} सेकंड पहले"),
        Language::German => format!("Letzter Erfolg vor {seconds} s"),
        Language::French => format!("Dernier succès il y a {seconds} s"),
        Language::Vietnamese => format!("Thành công lần cuối {seconds} giây trước"),
        Language::Turkish => format!("Son başarılı deneme {seconds} sn önce"),
        Language::Arabic => format!("آخر نجاح قبل {seconds} ثانية"),
    }
}

const fn taskbar_label(show_remaining: bool, language: Language) -> &'static str {
    match (show_remaining, language) {
        (true, Language::Korean) => "남은 사용량",
        (true, Language::English) => "Remaining usage",
        (true, Language::Spanish) => "Uso restante",
        (true, Language::PortugueseBrazil) => "Uso restante",
        (true, Language::Indonesian) => "Sisa penggunaan",
        (true, Language::Japanese) => "残り使用量",
        (true, Language::Hindi) => "शेष उपयोग",
        (true, Language::German) => "Verbleibende Nutzung",
        (true, Language::French) => "Utilisation restante",
        (true, Language::Vietnamese) => "Mức dùng còn lại",
        (true, Language::Turkish) => "Kalan kullanım",
        (true, Language::Arabic) => "الاستخدام المتبقي",
        (false, Language::Korean) => "주간 사용량",
        (false, Language::English) => "Weekly usage",
        (false, Language::Spanish) => "Uso semanal",
        (false, Language::PortugueseBrazil) => "Uso semanal",
        (false, Language::Indonesian) => "Penggunaan mingguan",
        (false, Language::Japanese) => "週間使用量",
        (false, Language::Hindi) => "साप्ताहिक उपयोग",
        (false, Language::German) => "Wöchentliche Nutzung",
        (false, Language::French) => "Utilisation hebdomadaire",
        (false, Language::Vietnamese) => "Mức dùng hằng tuần",
        (false, Language::Turkish) => "Haftalık kullanım",
        (false, Language::Arabic) => "الاستخدام الأسبوعي",
    }
}

const fn codex_usage_title(language: Language) -> &'static str {
    match language {
        Language::Korean => "Codex 사용량",
        Language::English => "Codex usage",
        Language::Spanish => "Uso de Codex",
        Language::PortugueseBrazil => "Uso do Codex",
        Language::Indonesian => "Penggunaan Codex",
        Language::Japanese => "Codex 使用量",
        Language::Hindi => "Codex उपयोग",
        Language::German => "Codex-Nutzung",
        Language::French => "Utilisation de Codex",
        Language::Vietnamese => "Mức dùng Codex",
        Language::Turkish => "Codex kullanımı",
        Language::Arabic => "استخدام Codex",
    }
}

const fn taskbar_usage_title_prefix(language: Language) -> &'static str {
    match language {
        Language::Spanish => "Uso de Codex",
        Language::PortugueseBrazil => "Uso do Codex",
        Language::Indonesian => "Penggunaan Codex",
        Language::Japanese => "Codex 使用量",
        Language::Hindi => "Codex उपयोग",
        Language::German => "Codex-Nutzung",
        Language::French => "Utilisation de Codex",
        Language::Vietnamese => "Mức dùng Codex",
        Language::Turkish => "Codex kullanımı",
        Language::Arabic => "استخدام Codex",
        Language::Korean | Language::English => codex_usage_title(language),
    }
}

const fn current_usage_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "현재 사용량",
        Language::English => "Current usage",
        Language::Spanish => "Uso actual",
        Language::PortugueseBrazil => "Uso atual",
        Language::Indonesian => "Penggunaan saat ini",
        Language::Japanese => "現在の使用量",
        Language::Hindi => "वर्तमान उपयोग",
        Language::German => "Aktuelle Nutzung",
        Language::French => "Utilisation actuelle",
        Language::Vietnamese => "Mức dùng hiện tại",
        Language::Turkish => "Geçerli kullanım",
        Language::Arabic => "الاستخدام الحالي",
    }
}

const fn remaining_usage_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "남은 사용량",
        Language::English => "Remaining",
        Language::Spanish => "Restante",
        Language::PortugueseBrazil => "Restante",
        Language::Indonesian => "Tersisa",
        Language::Japanese => "残り",
        Language::Hindi => "शेष",
        Language::German => "Verbleibend",
        Language::French => "Restant",
        Language::Vietnamese => "Còn lại",
        Language::Turkish => "Kalan",
        Language::Arabic => "المتبقي",
    }
}

const fn reset_at_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "초기화 시각",
        Language::English => "Reset at",
        Language::Spanish => "Se restablece",
        Language::PortugueseBrazil => "Redefine em",
        Language::Indonesian => "Direset pada",
        Language::Japanese => "リセット時刻",
        Language::Hindi => "रीसेट समय",
        Language::German => "Zurücksetzen um",
        Language::French => "Réinitialisation à",
        Language::Vietnamese => "Đặt lại lúc",
        Language::Turkish => "Sıfırlanma zamanı",
        Language::Arabic => "إعادة التعيين عند",
    }
}

const fn status_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "상태",
        Language::English => "Status",
        Language::Spanish => "Estado",
        Language::PortugueseBrazil => "Status",
        Language::Indonesian => "Status",
        Language::Japanese => "状態",
        Language::Hindi => "स्थिति",
        Language::German => "Status",
        Language::French => "État",
        Language::Vietnamese => "Trạng thái",
        Language::Turkish => "Durum",
        Language::Arabic => "الحالة",
    }
}

fn taskbar_risk_text(percent: f64, language: Language) -> &'static str {
    match (percent, language) {
        (value, Language::Korean) if value >= 90.0 => "위험",
        (value, Language::English) if value >= 90.0 => "Critical",
        (value, Language::Spanish) if value >= 90.0 => "Crítico",
        (value, Language::PortugueseBrazil) if value >= 90.0 => "Crítico",
        (value, Language::Indonesian) if value >= 90.0 => "Kritis",
        (value, Language::Japanese) if value >= 90.0 => "危険",
        (value, Language::Hindi) if value >= 90.0 => "गंभीर",
        (value, Language::German) if value >= 90.0 => "Kritisch",
        (value, Language::French) if value >= 90.0 => "Critique",
        (value, Language::Vietnamese) if value >= 90.0 => "Nghiêm trọng",
        (value, Language::Turkish) if value >= 90.0 => "Kritik",
        (value, Language::Arabic) if value >= 90.0 => "حرج",
        (value, Language::Korean) if value >= 70.0 => "주의",
        (value, Language::English) if value >= 70.0 => "Warning",
        (value, Language::Spanish) if value >= 70.0 => "Advertencia",
        (value, Language::PortugueseBrazil) if value >= 70.0 => "Atenção",
        (value, Language::Indonesian) if value >= 70.0 => "Peringatan",
        (value, Language::Japanese) if value >= 70.0 => "注意",
        (value, Language::Hindi) if value >= 70.0 => "चेतावनी",
        (value, Language::German) if value >= 70.0 => "Warnung",
        (value, Language::French) if value >= 70.0 => "Avertissement",
        (value, Language::Vietnamese) if value >= 70.0 => "Cảnh báo",
        (value, Language::Turkish) if value >= 70.0 => "Uyarı",
        (value, Language::Arabic) if value >= 70.0 => "تحذير",
        (_, Language::Korean) => "안정",
        (_, Language::English) => "Healthy",
        (_, Language::Spanish) => "Correcto",
        (_, Language::PortugueseBrazil) => "Saudável",
        (_, Language::Indonesian) => "Sehat",
        (_, Language::Japanese) => "正常",
        (_, Language::Hindi) => "ठीक",
        (_, Language::German) => "In Ordnung",
        (_, Language::French) => "Sain",
        (_, Language::Vietnamese) => "Ổn định",
        (_, Language::Turkish) => "Sağlıklı",
        (_, Language::Arabic) => "سليم",
    }
}

fn effective_language(preference: LanguagePreference) -> Language {
    let (language, locale) = native::user_ui_language();
    resolve_windows_language(preference, language, locale.as_deref())
}

fn update_checker() -> Option<UpdateChecker> {
    UpdateChecker::new(
        env!("CARGO_PKG_VERSION"),
        option_env!("CARGO_PKG_REPOSITORY").filter(|value| !value.is_empty()),
        64 * 1024,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiagnosticSummary {
    settings_valid: bool,
    proxy_present: bool,
    auth_exists: bool,
    taskbar_available: bool,
    cli: &'static str,
    app_server: &'static str,
    login: &'static str,
    response_format: &'static str,
}

impl DiagnosticSummary {
    fn localized(&self, language: Language) -> (&'static str, String) {
        let copy = diagnostic_copy(language);
        match language {
            Language::Korean => (
                "Codex 사용량 모니터 진단",
                format!(
                    "설정: {}\n프록시 설정: {}\n로그인 파일: {}\n작업 표시줄 호환성: {}\nCodex CLI: {}\n앱 서버: {}\n로그인: {}\n응답 형식: {}",
                    pass_fail(self.settings_valid, language),
                    if self.proxy_present { "감지됨" } else { "없음" },
                    pass_fail(self.auth_exists, language),
                    pass_fail(self.taskbar_available, language),
                    diagnostic_status(self.cli, language),
                    diagnostic_status(self.app_server, language),
                    diagnostic_status(self.login, language),
                    diagnostic_status(self.response_format, language),
                ),
            ),
            Language::English => (
                "Codex Usage Monitor diagnostics",
                format!(
                    "Settings: {}\nProxy configuration: {}\nLogin file: {}\nTaskbar compatibility: {}\nCodex CLI: {}\nApp server: {}\nLogin: {}\nResponse format: {}",
                    pass_fail(self.settings_valid, language),
                    if self.proxy_present { "detected" } else { "none" },
                    pass_fail(self.auth_exists, language),
                    pass_fail(self.taskbar_available, language),
                    self.cli,
                    self.app_server,
                    self.login,
                    self.response_format,
                ),
            ),
            _ => (
                copy.title,
                format!(
                    "{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
                    copy.settings,
                    pass_fail(self.settings_valid, language),
                    copy.proxy,
                    proxy_presence(self.proxy_present, language),
                    copy.login_file,
                    pass_fail(self.auth_exists, language),
                    copy.taskbar,
                    pass_fail(self.taskbar_available, language),
                    copy.cli,
                    diagnostic_status(self.cli, language),
                    copy.app_server,
                    diagnostic_status(self.app_server, language),
                    copy.login,
                    diagnostic_status(self.login, language),
                    copy.response_format,
                    diagnostic_status(self.response_format, language),
                ),
            ),
        }
    }
}

struct DiagnosticCopy {
    title: &'static str,
    settings: &'static str,
    proxy: &'static str,
    login_file: &'static str,
    taskbar: &'static str,
    cli: &'static str,
    app_server: &'static str,
    login: &'static str,
    response_format: &'static str,
}

const fn diagnostic_copy(language: Language) -> DiagnosticCopy {
    match language {
        Language::Korean => DiagnosticCopy {
            title: "Codex 사용량 모니터 진단",
            settings: "설정",
            proxy: "프록시 설정",
            login_file: "로그인 파일",
            taskbar: "작업 표시줄 호환성",
            cli: "Codex CLI",
            app_server: "앱 서버",
            login: "로그인",
            response_format: "응답 형식",
        },
        Language::English => DiagnosticCopy {
            title: "Codex Usage Monitor diagnostics",
            settings: "Settings",
            proxy: "Proxy configuration",
            login_file: "Login file",
            taskbar: "Taskbar compatibility",
            cli: "Codex CLI",
            app_server: "App server",
            login: "Login",
            response_format: "Response format",
        },
        Language::Spanish => DiagnosticCopy {
            title: "Diagnóstico de Codex Usage Monitor",
            settings: "Configuración",
            proxy: "Configuración de proxy",
            login_file: "Archivo de inicio de sesión",
            taskbar: "Compatibilidad con la barra de tareas",
            cli: "Codex CLI",
            app_server: "Servidor de la app",
            login: "Inicio de sesión",
            response_format: "Formato de respuesta",
        },
        Language::PortugueseBrazil => DiagnosticCopy {
            title: "Diagnóstico do Codex Usage Monitor",
            settings: "Configurações",
            proxy: "Configuração de proxy",
            login_file: "Arquivo de login",
            taskbar: "Compatibilidade com a barra de tarefas",
            cli: "Codex CLI",
            app_server: "Servidor do app",
            login: "Login",
            response_format: "Formato da resposta",
        },
        Language::Indonesian => DiagnosticCopy {
            title: "Diagnostik Codex Usage Monitor",
            settings: "Pengaturan",
            proxy: "Konfigurasi proxy",
            login_file: "File login",
            taskbar: "Kompatibilitas taskbar",
            cli: "Codex CLI",
            app_server: "Server aplikasi",
            login: "Login",
            response_format: "Format respons",
        },
        Language::Japanese => DiagnosticCopy {
            title: "Codex 使用量モニター診断",
            settings: "設定",
            proxy: "プロキシ設定",
            login_file: "ログインファイル",
            taskbar: "タスクバー互換性",
            cli: "Codex CLI",
            app_server: "アプリサーバー",
            login: "ログイン",
            response_format: "応答形式",
        },
        Language::Hindi => DiagnosticCopy {
            title: "Codex Usage Monitor निदान",
            settings: "सेटिंग्स",
            proxy: "प्रॉक्सी कॉन्फ़िगरेशन",
            login_file: "लॉगिन फ़ाइल",
            taskbar: "टास्कबार संगतता",
            cli: "Codex CLI",
            app_server: "ऐप सर्वर",
            login: "लॉगिन",
            response_format: "प्रतिक्रिया प्रारूप",
        },
        Language::German => DiagnosticCopy {
            title: "Codex Usage Monitor Diagnose",
            settings: "Einstellungen",
            proxy: "Proxy-Konfiguration",
            login_file: "Anmeldedatei",
            taskbar: "Taskleisten-Kompatibilität",
            cli: "Codex CLI",
            app_server: "App-Server",
            login: "Anmeldung",
            response_format: "Antwortformat",
        },
        Language::French => DiagnosticCopy {
            title: "Diagnostic de Codex Usage Monitor",
            settings: "Paramètres",
            proxy: "Configuration du proxy",
            login_file: "Fichier de connexion",
            taskbar: "Compatibilité avec la barre des tâches",
            cli: "Codex CLI",
            app_server: "Serveur d'application",
            login: "Connexion",
            response_format: "Format de réponse",
        },
        Language::Vietnamese => DiagnosticCopy {
            title: "Chẩn đoán Codex Usage Monitor",
            settings: "Cài đặt",
            proxy: "Cấu hình proxy",
            login_file: "Tệp đăng nhập",
            taskbar: "Tương thích thanh tác vụ",
            cli: "Codex CLI",
            app_server: "Máy chủ ứng dụng",
            login: "Đăng nhập",
            response_format: "Định dạng phản hồi",
        },
        Language::Turkish => DiagnosticCopy {
            title: "Codex Usage Monitor tanılama",
            settings: "Ayarlar",
            proxy: "Proxy yapılandırması",
            login_file: "Oturum açma dosyası",
            taskbar: "Görev çubuğu uyumluluğu",
            cli: "Codex CLI",
            app_server: "Uygulama sunucusu",
            login: "Oturum açma",
            response_format: "Yanıt biçimi",
        },
        Language::Arabic => DiagnosticCopy {
            title: "تشخيص Codex Usage Monitor",
            settings: "الإعدادات",
            proxy: "إعدادات الوكيل",
            login_file: "ملف تسجيل الدخول",
            taskbar: "توافق شريط المهام",
            cli: "Codex CLI",
            app_server: "خادم التطبيق",
            login: "تسجيل الدخول",
            response_format: "تنسيق الاستجابة",
        },
    }
}

const fn pass_fail(value: bool, language: Language) -> &'static str {
    match (value, language) {
        (true, Language::Korean) => "정상",
        (false, Language::Korean) => "확인 필요",
        (true, Language::English) => "OK",
        (false, Language::English) => "needs attention",
        (true, Language::Spanish) => "correcto",
        (false, Language::Spanish) => "requiere atención",
        (true, Language::PortugueseBrazil) => "OK",
        (false, Language::PortugueseBrazil) => "requer atenção",
        (true, Language::Indonesian) => "OK",
        (false, Language::Indonesian) => "perlu perhatian",
        (true, Language::Japanese) => "正常",
        (false, Language::Japanese) => "確認が必要",
        (true, Language::Hindi) => "ठीक",
        (false, Language::Hindi) => "ध्यान चाहिए",
        (true, Language::German) => "OK",
        (false, Language::German) => "Aufmerksamkeit erforderlich",
        (true, Language::French) => "OK",
        (false, Language::French) => "attention requise",
        (true, Language::Vietnamese) => "OK",
        (false, Language::Vietnamese) => "cần chú ý",
        (true, Language::Turkish) => "Tamam",
        (false, Language::Turkish) => "dikkat gerekiyor",
        (true, Language::Arabic) => "حسنًا",
        (false, Language::Arabic) => "يتطلب الانتباه",
    }
}

const fn proxy_presence(present: bool, language: Language) -> &'static str {
    match (present, language) {
        (true, Language::Korean) => "감지됨",
        (false, Language::Korean) => "없음",
        (true, Language::English) => "detected",
        (false, Language::English) => "none",
        (true, Language::Spanish) => "detectada",
        (false, Language::Spanish) => "ninguna",
        (true, Language::PortugueseBrazil) => "detectada",
        (false, Language::PortugueseBrazil) => "nenhuma",
        (true, Language::Indonesian) => "terdeteksi",
        (false, Language::Indonesian) => "tidak ada",
        (true, Language::Japanese) => "検出済み",
        (false, Language::Japanese) => "なし",
        (true, Language::Hindi) => "मिला",
        (false, Language::Hindi) => "कोई नहीं",
        (true, Language::German) => "erkannt",
        (false, Language::German) => "keine",
        (true, Language::French) => "détectée",
        (false, Language::French) => "aucune",
        (true, Language::Vietnamese) => "đã phát hiện",
        (false, Language::Vietnamese) => "không có",
        (true, Language::Turkish) => "algılandı",
        (false, Language::Turkish) => "yok",
        (true, Language::Arabic) => "تم الاكتشاف",
        (false, Language::Arabic) => "لا يوجد",
    }
}

fn diagnostic_status(value: &'static str, language: Language) -> &'static str {
    if matches!(language, Language::English) {
        return value;
    }
    match value {
        "ok" | "started" => diagnostic_ok(language),
        "unavailable" => diagnostic_unavailable(language),
        "failed" | "request failed" => diagnostic_failed(language),
        "invalid" => diagnostic_invalid(language),
        "not checked" => diagnostic_not_checked(language),
        "unknown" => diagnostic_unknown(language),
        _ => value,
    }
}

const fn diagnostic_ok(language: Language) -> &'static str {
    match language {
        Language::Korean => "정상",
        Language::English => "ok",
        Language::Spanish => "correcto",
        Language::PortugueseBrazil => "OK",
        Language::Indonesian => "OK",
        Language::Japanese => "正常",
        Language::Hindi => "ठीक",
        Language::German => "OK",
        Language::French => "OK",
        Language::Vietnamese => "OK",
        Language::Turkish => "Tamam",
        Language::Arabic => "حسنًا",
    }
}

const fn diagnostic_unavailable(language: Language) -> &'static str {
    match language {
        Language::Korean => "사용할 수 없음",
        Language::English => "unavailable",
        Language::Spanish => "no disponible",
        Language::PortugueseBrazil => "indisponível",
        Language::Indonesian => "tidak tersedia",
        Language::Japanese => "利用不可",
        Language::Hindi => "उपलब्ध नहीं",
        Language::German => "nicht verfügbar",
        Language::French => "indisponible",
        Language::Vietnamese => "không khả dụng",
        Language::Turkish => "kullanılamıyor",
        Language::Arabic => "غير متاح",
    }
}

const fn diagnostic_failed(language: Language) -> &'static str {
    match language {
        Language::Korean => "실패",
        Language::English => "failed",
        Language::Spanish => "falló",
        Language::PortugueseBrazil => "falhou",
        Language::Indonesian => "gagal",
        Language::Japanese => "失敗",
        Language::Hindi => "विफल",
        Language::German => "fehlgeschlagen",
        Language::French => "échec",
        Language::Vietnamese => "thất bại",
        Language::Turkish => "başarısız",
        Language::Arabic => "فشل",
    }
}

const fn diagnostic_invalid(language: Language) -> &'static str {
    match language {
        Language::Korean => "잘못됨",
        Language::English => "invalid",
        Language::Spanish => "no válido",
        Language::PortugueseBrazil => "inválido",
        Language::Indonesian => "tidak valid",
        Language::Japanese => "無効",
        Language::Hindi => "अमान्य",
        Language::German => "ungültig",
        Language::French => "invalide",
        Language::Vietnamese => "không hợp lệ",
        Language::Turkish => "geçersiz",
        Language::Arabic => "غير صالح",
    }
}

const fn diagnostic_not_checked(language: Language) -> &'static str {
    match language {
        Language::Korean => "확인하지 못함",
        Language::English => "not checked",
        Language::Spanish => "no comprobado",
        Language::PortugueseBrazil => "não verificado",
        Language::Indonesian => "belum diperiksa",
        Language::Japanese => "未確認",
        Language::Hindi => "जाँचा नहीं गया",
        Language::German => "nicht geprüft",
        Language::French => "non vérifié",
        Language::Vietnamese => "chưa kiểm tra",
        Language::Turkish => "kontrol edilmedi",
        Language::Arabic => "لم يتم الفحص",
    }
}

const fn diagnostic_unknown(language: Language) -> &'static str {
    match language {
        Language::Korean => "알 수 없음",
        Language::English => "unknown",
        Language::Spanish => "desconocido",
        Language::PortugueseBrazil => "desconhecido",
        Language::Indonesian => "tidak diketahui",
        Language::Japanese => "不明",
        Language::Hindi => "अज्ञात",
        Language::German => "unbekannt",
        Language::French => "inconnu",
        Language::Vietnamese => "không xác định",
        Language::Turkish => "bilinmiyor",
        Language::Arabic => "غير معروف",
    }
}

fn run_safe_diagnostics(write_console: bool) -> io::Result<DiagnosticSummary> {
    let logger = DiagnosticLogger::new();
    let store = SettingsStore::new();
    let settings_valid = inspect_settings_for_diagnostics(&store, &logger)?;

    let proxy_present = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some());
    let _ = logger.record_safe(SafeDiagnostic::Proxy {
        present: proxy_present,
    });

    let auth_path = auth_path();
    let auth_exists = auth_path.is_file();
    let _ = logger.record_safe(SafeDiagnostic::Login {
        auth_path: auth_path.clone(),
        exists: auth_exists,
    });

    let taskbar_available = taskbar::taskbar_available();
    let _ = logger.record_safe(SafeDiagnostic::Taskbar {
        available: taskbar_available,
    });

    let cli_result = locate_supported_cli();
    match &cli_result {
        Ok(path) => {
            let _ = logger.record_safe(SafeDiagnostic::Cli {
                path: path.clone(),
                exists: path.is_file(),
            });
        }
        Err(_) => {
            let _ = logger.record_safe(SafeDiagnostic::Cli {
                path: PathBuf::from("<unavailable>"),
                exists: false,
            });
        }
    }

    let profile_diagnostics = diagnose_configured_profiles(&store, settings_valid)?;
    let rpc = profile_diagnostics.system_result;
    if let Err(error) = rpc {
        let code = match error {
            UsageError::CliNotFound | UsageError::UnsupportedCli => DiagnosticCode::CliUnavailable,
            UsageError::NotLoggedIn | UsageError::AuthenticationExpired => {
                DiagnosticCode::LoginUnavailable
            }
            _ => DiagnosticCode::RpcFailed,
        };
        let _ = logger.record_safe(SafeDiagnostic::Rpc { code });
    }

    let configured_profiles = profile_diagnostics.configured;
    let ok_profiles = profile_diagnostics.ok;
    let login_required_profiles = profile_diagnostics.login_required;
    let request_failed_profiles = profile_diagnostics.request_failed;
    let _ = logger.record_safe(profile_diagnostics.safe_diagnostic(settings_valid));

    let cli_status = if cli_result.is_ok() {
        "ok"
    } else {
        "unavailable"
    };
    let app_server_status = match rpc {
        Ok(_) => "ok",
        Err(UsageError::CliNotFound | UsageError::UnsupportedCli) => "not checked",
        Err(UsageError::AppServerStartFailed) => "failed",
        Err(_) => "started",
    };
    let login_status = match rpc {
        Ok(_) => "ok",
        Err(UsageError::NotLoggedIn | UsageError::AuthenticationExpired) => "failed",
        Err(
            UsageError::CliNotFound | UsageError::UnsupportedCli | UsageError::AppServerStartFailed,
        ) => "not checked",
        Err(_) => "unknown",
    };
    let response_format_status = match rpc {
        Ok(_) => "ok",
        Err(UsageError::InvalidResponse | UsageError::RateLimitUnavailable) => "invalid",
        Err(UsageError::RequestFailed | UsageError::RpcTimeout | UsageError::RpcOverloaded) => {
            "request failed"
        }
        Err(_) => "not checked",
    };
    let summary = DiagnosticSummary {
        settings_valid,
        proxy_present,
        auth_exists,
        taskbar_available,
        cli: cli_status,
        app_server: app_server_status,
        login: login_status,
        response_format: response_format_status,
    };

    if write_console {
        println!("settings_valid={settings_valid}");
        println!("proxy_present={proxy_present}");
        println!("auth_path={}", safe_path_text(&auth_path));
        println!("auth_exists={auth_exists}");
        println!("taskbar_available={taskbar_available}");
        println!("cli={cli_status}");
        println!("app_server={app_server_status}");
        println!("login={login_status}");
        println!("response_format={response_format_status}");
        println!("profiles_configured={configured_profiles}");
        println!("profiles_ok={ok_profiles}");
        println!("profiles_login_required={login_required_profiles}");
        println!("profiles_request_failed={request_failed_profiles}");
        if let Err(error) = rpc {
            println!("usage_check={}", error.diagnostic_code());
        }
    }
    Ok(summary)
}

fn diagnose_configured_profiles(
    store: &SettingsStore,
    settings_valid: bool,
) -> io::Result<ProfileDiagnosticRun> {
    let provider = AppServerUsageProvider::new();
    if !settings_valid {
        return Ok(diagnose_profile_contexts(
            &provider,
            1,
            &[ProfileExecutionContext::system()],
        ));
    }
    let settings = store.load()?;
    let (service, startup) = ProfileSettingsService::start_with_recovery(
        store.clone(),
        settings,
        NativeProfileFileSystem::default(),
    );
    let run = diagnose_profile_contexts(
        &provider,
        startup.report().configured,
        startup.execution_contexts(),
    );
    service.stop()?;
    Ok(run)
}

fn auth_path() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
        .join("auth.json")
}

fn safe_path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().replace(['\r', '\n', '\0'], "?")
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{Duration, SystemTime},
    };

    use super::{
        append_forecast_tooltip, data_state_for_snapshot, diagnostic_status,
        forecast_duration_text, forecast_view, last_success_text, pass_fail,
        profile_usage_presentation_for_snapshot, profile_usage_presentation_for_window,
        proxy_presence, row_view, row_view_with_reset_time, status_with_update, taskbar_copy,
        taskbar_risk_text, AppRuntime, DiagnosticSummary,
    };
    use crate::codex::{LoginPageOpener, OperationCancellation, ProfileAccountProvider};
    use crate::windows::{ForecastView, ProfileUsageStatus, UiAction, UiBackend, WidgetDataState};
    use crate::{
        domain::ResetDateTime, windows::UsageRowView, AsyncDiagnosticWriter, AvailableUpdate,
        CodexUsage, CorrelatedProfileSettingsEvent, DiagnosticLogger, Forecast, ForecastQuality,
        ForecastResult, Language, LanguagePreference, NativeProfileFileSystem, PollSnapshot,
        ProfileExecutionContext, ProfilePollingService, ProfileRuntimeState,
        ProfileSettingsOperation, ProfileSettingsRequestId, ProfileSettingsService, ResetCredits,
        Settings, SettingsStore, UpdateCheckNotice, UpdatePresentation, UpdatePresentationStatus,
        UsageError, UsageForecastService, UsageHistoryStore, UsageLevel, UsageProfileId,
        UsageProfileRoot, UsageWindow, WindowKind,
    };
    const ALL_LANGUAGES: [Language; 12] = [
        Language::Korean,
        Language::English,
        Language::Spanish,
        Language::PortugueseBrazil,
        Language::Indonesian,
        Language::Japanese,
        Language::Hindi,
        Language::German,
        Language::French,
        Language::Vietnamese,
        Language::Turkish,
        Language::Arabic,
    ];

    fn usage_window(used_percent: f64) -> UsageWindow {
        UsageWindow::new(WindowKind::Secondary, used_percent, None, None).unwrap()
    }

    #[test]
    fn profile_usage_presentation_keeps_summary_and_typed_consumed_usage() {
        let presentation = profile_usage_presentation_for_window(Some(&usage_window(81.4)));
        assert_eq!(presentation.used_percent, Some(81));
        assert_eq!(presentation.usage_status, Some(ProfileUsageStatus::Warning));
    }

    #[test]
    fn profile_usage_presentation_omits_fake_progress_without_valid_usage() {
        let presentation = profile_usage_presentation_for_window(None);
        assert_eq!(presentation.used_percent, None);
        assert_eq!(presentation.usage_status, None);
    }

    #[test]
    fn profile_usage_presentation_keeps_retained_usage_after_a_transient_error() {
        let snapshot = PollSnapshot {
            usage: Some(CodexUsage {
                primary: None,
                secondary: Some(usage_window(81.4)),
                reset_credits: None,
                fetched_at: std::time::UNIX_EPOCH,
            }),
            last_error: Some(UsageError::RequestFailed),
            ..PollSnapshot::default()
        };

        let presentation =
            profile_usage_presentation_for_snapshot(Some(&snapshot), false, Language::English);

        assert_eq!(presentation.used_percent, Some(81));
        assert_eq!(presentation.usage_status, Some(ProfileUsageStatus::Warning));
    }

    #[test]
    fn profile_usage_presentation_summarizes_reset_credits_and_both_limit_windows() {
        let snapshot = PollSnapshot {
            usage: Some(CodexUsage {
                primary: Some(
                    UsageWindow::new(WindowKind::Primary, 28.0, Some(300), None).unwrap(),
                ),
                secondary: Some(
                    UsageWindow::new(WindowKind::Secondary, 81.0, Some(10_080), None).unwrap(),
                ),
                reset_credits: Some(ResetCredits {
                    available_count: 2,
                    nearest_expiry: None,
                }),
                fetched_at: std::time::UNIX_EPOCH,
            }),
            ..PollSnapshot::default()
        };

        let presentation =
            profile_usage_presentation_for_snapshot(Some(&snapshot), false, Language::English);

        assert_eq!(presentation.summary, "Reset coupons: 2");
        assert!(presentation.details.contains("28% used"));
        assert!(presentation.details.contains("81% used"));
        assert!(presentation.details.contains(" | "));
    }

    #[test]
    fn profile_success_event_preserves_newer_local_preferences() {
        let mut state = ProfileRuntimeState::new(
            Settings::default(),
            UsageProfileRoot::new(std::path::PathBuf::from("runtime-preference-test")),
        );
        state.settings.show_remaining_percent = true;
        let mut worker_settings = Settings::default();
        let id = worker_settings.usage_profiles.add("Work").unwrap().id();
        state.request_add("Work".to_owned()).unwrap();
        let request_id = ProfileSettingsRequestId::new(1).unwrap();
        state.bind_settings_request(request_id, ProfileSettingsOperation::Add);

        state.apply_settings_event(CorrelatedProfileSettingsEvent::Added {
            request_id,
            settings: worker_settings,
            id,
        });

        assert!(state.settings.show_remaining_percent);
        assert_eq!(
            state.settings.usage_profiles.managed()[0].id(),
            UsageProfileId::Managed(1)
        );
    }

    #[test]
    fn selected_profile_without_cached_usage_renders_loading() {
        assert_eq!(
            data_state_for_snapshot(&PollSnapshot::default(), false),
            WidgetDataState::Loading
        );
    }

    #[test]
    fn custom_system_profile_name_is_used_outside_manager() {
        let mut settings = Settings::default();
        settings
            .usage_profiles
            .rename(UsageProfileId::System, "Main")
            .unwrap();
        settings.language = LanguagePreference::English;

        let mut runtime = test_app_runtime(settings);
        let snapshot = runtime.snapshot();
        let ui_settings = runtime.settings();
        runtime.shutdown();

        assert_eq!(snapshot.usage_profile_label, "Main");
        assert!(snapshot
            .taskbar_tooltip
            .starts_with("Usage profiles: Main\n"));
        assert_eq!(
            ui_settings
                .usage_profiles
                .iter()
                .find(|profile| profile.id == UsageProfileId::System)
                .map(|profile| profile.label.as_str()),
            Some("Main")
        );
    }

    #[test]
    fn failed_system_rename_save_preserves_local_state_and_records_safe_diagnostic() {
        let root = std::env::temp_dir().join(format!(
            "codex-peek-system-rename-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let blocked_store_root = root.join("blocked-settings-root");
        std::fs::write(&blocked_store_root, b"not a directory").unwrap();
        let store = SettingsStore::for_root(&blocked_store_root);
        let log_path = root.join("diagnostics.log");
        let diagnostics = AsyncDiagnosticWriter::start(DiagnosticLogger::for_path(&log_path), 8);
        let mut runtime =
            test_app_runtime_with_store(Settings::default(), store, Some(diagnostics));

        runtime.dispatch(UiAction::RenameUsageProfile(
            UsageProfileId::System,
            "Main".to_owned(),
        ));
        assert!(runtime
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .mutation_pending());

        assert_eq!(
            runtime.profile_settings().flush().unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        runtime.drain_profile_events();
        let settings = runtime.settings_snapshot();
        assert_eq!(settings.usage_profiles.system_label(), None);
        assert_eq!(settings.usage_profiles.selected(), UsageProfileId::System);
        assert!(!runtime
            .profile_state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .mutation_pending());

        runtime.shutdown();
        let diagnostic = std::fs::read_to_string(&log_path).unwrap();
        assert!(diagnostic.contains("settings_invalid valid=false"));
        assert!(!diagnostic.contains("Main"));
        let _ = std::fs::remove_dir_all(root);
    }

    struct DisplayProfileProvider;

    impl ProfileAccountProvider for DisplayProfileProvider {
        fn fetch_profile(
            &self,
            _profile: &ProfileExecutionContext,
            _allow_auth_refresh: bool,
            _cancellation: OperationCancellation,
        ) -> Result<CodexUsage, UsageError> {
            Err(UsageError::NotLoggedIn)
        }

        fn login_profile(
            &self,
            _profile: &ProfileExecutionContext,
            _open: LoginPageOpener,
            _cancellation: OperationCancellation,
        ) -> Result<bool, UsageError> {
            Ok(false)
        }

        fn logout_profile(
            &self,
            _profile: &ProfileExecutionContext,
            _cancellation: OperationCancellation,
        ) -> Result<(), UsageError> {
            Ok(())
        }
    }

    fn test_app_runtime(settings: Settings) -> AppRuntime {
        let store = SettingsStore::for_root(std::env::temp_dir().join(format!(
            "codex-peek-app-display-test-{}",
            std::process::id()
        )));
        test_app_runtime_with_store(settings, store, None)
    }

    fn test_app_runtime_with_store(
        settings: Settings,
        store: SettingsStore,
        diagnostics: Option<AsyncDiagnosticWriter>,
    ) -> AppRuntime {
        let profile_settings = ProfileSettingsService::start(
            store.clone(),
            settings.clone(),
            NativeProfileFileSystem::default(),
        );
        let profile_poller = ProfilePollingService::start(
            Arc::new(DisplayProfileProvider),
            vec![ProfileExecutionContext::system()],
            UsageProfileId::System,
            settings.refresh_interval_minutes,
            settings.auto_auth_refresh,
        )
        .expect("system-only profile poller starts");
        let usage_forecast = Arc::new(UsageForecastService::start(
            UsageHistoryStore::for_root(store.root().to_path_buf()),
            [UsageProfileId::System],
            crate::ForecastPolicy::default(),
        ));
        usage_forecast.set_enabled(settings.usage_forecast_enabled);
        AppRuntime {
            profile_settings: Some(profile_settings),
            profile_poller: Some(profile_poller),
            usage_forecast: Some(usage_forecast),
            profile_state: std::sync::Mutex::new(ProfileRuntimeState::new(
                settings,
                UsageProfileRoot::new(store.root().to_path_buf()),
            )),
            diagnostics,
            startup_hidden: false,
            update_presentation: UpdatePresentation::default(),
        }
    }

    #[test]
    fn update_status_is_appended_without_hiding_usage_error() {
        assert_eq!(
            status_with_update(
                "Usage request failed".to_owned(),
                UpdatePresentationStatus::Failed,
                Language::English,
            ),
            "Usage request failed · Update check failed"
        );
    }

    #[test]
    fn forecast_toggle_updates_ui_preference_and_service_visibility() {
        let mut runtime = test_app_runtime(Settings::default());
        assert!(runtime.settings().usage_forecast_enabled);

        let settings = runtime.dispatch(UiAction::ToggleUsageForecast);

        assert!(!settings.usage_forecast_enabled);
        assert!(!runtime.settings_snapshot().usage_forecast_enabled);
        assert!(runtime
            .usage_forecast()
            .forecast_at(
                UsageProfileId::System,
                WindowKind::Primary,
                std::time::SystemTime::now()
            )
            .is_none());

        let settings = runtime.dispatch(UiAction::ToggleUsageForecast);
        assert!(settings.usage_forecast_enabled);
        runtime.shutdown();
    }

    #[test]
    fn clear_forecast_history_is_an_action_without_changing_preference() {
        let mut runtime = test_app_runtime(Settings::default());
        let before = runtime.settings_snapshot().usage_forecast_enabled;

        let settings = runtime.dispatch(UiAction::ClearUsageHistory);

        assert_eq!(settings.usage_forecast_enabled, before);
        assert_eq!(runtime.settings_snapshot().usage_forecast_enabled, before);
        runtime.shutdown();
    }

    #[test]
    fn update_notice_remains_pending_until_the_ui_boundary_takes_it() {
        let mut runtime = test_app_runtime(Settings::default());
        runtime
            .update_presentation
            .begin_check(crate::UpdateCheckIntent::UserInitiated);
        runtime.update_presentation.record_result(Ok(None));

        let _ = runtime.snapshot();
        assert_eq!(
            runtime.take_update_notice(),
            Some(UpdateCheckNotice::Current)
        );
        assert!(runtime.take_update_notice().is_none());
        runtime.shutdown();
    }

    #[test]
    fn selecting_an_existing_automatic_update_queues_it_for_the_ui_boundary() {
        let mut runtime = test_app_runtime(Settings::default());
        let update = AvailableUpdate {
            version: semver::Version::parse("2.0.0").unwrap(),
            release_url: "https://github.com/owner/repo/releases/tag/v2.0.0".to_owned(),
        };
        runtime
            .update_presentation
            .begin_check(crate::UpdateCheckIntent::Automatic);
        runtime
            .update_presentation
            .record_result(Ok(Some(update.clone())));

        let _ = runtime.dispatch(UiAction::CheckForUpdates);

        assert_eq!(
            runtime.take_update_notice(),
            Some(UpdateCheckNotice::Available(update))
        );
        runtime.shutdown();
    }

    #[test]
    fn app_dynamic_copy_is_nonempty_for_every_language() {
        let row = UsageRowView {
            label: "7d".to_owned(),
            used_percent: 73.0,
            display_percent: 73.0,
            percent_text: "73%".to_owned(),
            reset_text: "2026-07-27 03:00".to_owned(),
            level: UsageLevel::Caution,
            forecast: ForecastView::Hidden,
        };
        let summary = DiagnosticSummary {
            settings_valid: true,
            proxy_present: true,
            auth_exists: false,
            taskbar_available: true,
            cli: "ok",
            app_server: "started",
            login: "failed",
            response_format: "invalid",
        };

        for language in ALL_LANGUAGES {
            assert!(!last_success_text(42, language).trim().is_empty());
            assert!(last_success_text(42, language).contains("42"));
            assert!(!pass_fail(true, language).trim().is_empty());
            assert!(!pass_fail(false, language).trim().is_empty());
            assert!(!proxy_presence(true, language).trim().is_empty());
            assert!(!proxy_presence(false, language).trim().is_empty());
            for status in [
                "ok",
                "started",
                "unavailable",
                "failed",
                "request failed",
                "invalid",
                "not checked",
                "unknown",
            ] {
                assert!(!diagnostic_status(status, language).trim().is_empty());
            }
            assert!(!taskbar_risk_text(40.0, language).trim().is_empty());
            assert!(!taskbar_risk_text(75.0, language).trim().is_empty());
            assert!(!taskbar_risk_text(95.0, language).trim().is_empty());

            let used = taskbar_copy(Some(&row), language, "status", false, None);
            assert!(!used.label.trim().is_empty());
            assert!(used.tooltip.contains("73%"));
            assert!(used.tooltip.contains("27%"));
            assert!(used.tooltip.contains("2026-07-27 03:00"));
            assert!(used.tooltip.contains("status"));

            let remaining = taskbar_copy(Some(&row), language, "status", true, None);
            assert!(!remaining.label.trim().is_empty());
            assert!(remaining.tooltip.contains("73%"));
            assert!(remaining.tooltip.contains("27%"));

            let unavailable = taskbar_copy(None, language, "status", false, None);
            assert!(!unavailable.label.trim().is_empty());
            assert!(unavailable.tooltip.contains("status"));

            let reset_credits = "reset-credit-line";
            let used_with_reset =
                taskbar_copy(Some(&row), language, "status", false, Some(reset_credits));
            assert!(used_with_reset.tooltip.contains(reset_credits));
            let unavailable_with_reset =
                taskbar_copy(None, language, "status", false, Some(reset_credits));
            assert!(unavailable_with_reset.tooltip.contains(reset_credits));

            let (title, body) = summary.localized(language);
            assert!(!title.trim().is_empty());
            assert!(!body.trim().is_empty());
            assert!(body.contains("Codex CLI"));
            assert!(body.lines().count() >= 8);
        }
    }

    #[test]
    fn app_dynamic_copy_has_representative_unicode_translations() {
        assert_eq!(
            last_success_text(5, Language::Spanish),
            "Último éxito hace 5 s"
        );
        assert_eq!(
            taskbar_copy(None, Language::Japanese, "自動更新中", false, None).tooltip,
            "Codex 使用量\n状態: 自動更新中"
        );
        assert_eq!(taskbar_risk_text(95.0, Language::Arabic), "حرج");
        assert_eq!(pass_fail(false, Language::Hindi), "ध्यान चाहिए");
    }

    #[test]
    fn taskbar_copy_is_explicit_and_keeps_reset_details_in_the_tooltip() {
        let korean_row = UsageRowView {
            label: "7일".to_owned(),
            used_percent: 8.0,
            display_percent: 8.0,
            percent_text: "8%".to_owned(),
            reset_text: "2026-07-27 (월) 03:00".to_owned(),
            level: UsageLevel::Stable,
            forecast: ForecastView::Hidden,
        };

        let korean = taskbar_copy(
            Some(&korean_row),
            Language::Korean,
            "자동 갱신 중",
            false,
            None,
        );
        assert_eq!(korean.label, "주간 사용량");
        assert!(korean.tooltip.starts_with("Codex 7일 사용량\n"));
        assert!(korean.tooltip.contains("현재 사용량: 8%"));
        assert!(korean.tooltip.contains("남은 사용량: 92%"));
        assert!(korean
            .tooltip
            .contains("초기화 시각: 2026-07-27 (월) 03:00"));
        assert!(korean.tooltip.contains("상태: 안정"));

        let english_row = UsageRowView {
            label: "7d".to_owned(),
            reset_text: "2026-07-27 (Mon) 03:00".to_owned(),
            ..korean_row.clone()
        };
        let english = taskbar_copy(
            Some(&english_row),
            Language::English,
            "Polling",
            false,
            None,
        );
        assert_eq!(english.label, "Weekly usage");
        assert!(english.tooltip.starts_with("Codex 7d usage\n"));
        assert!(english.tooltip.contains("Current usage: 8%"));
        assert!(english.tooltip.contains("Remaining: 92%"));
        assert!(english.tooltip.contains("Reset at: 2026-07-27 (Mon) 03:00"));
        assert!(english.tooltip.contains("Status: Healthy"));

        let remaining = taskbar_copy(
            Some(&korean_row),
            Language::Korean,
            "자동 갱신 중",
            true,
            None,
        );
        assert_eq!(remaining.label, "남은 사용량");
        assert!(remaining.tooltip.contains("현재 사용량: 8%"));
        assert!(remaining.tooltip.contains("남은 사용량: 92%"));

        let unavailable = taskbar_copy(None, Language::Korean, "정보 없음", false, None);
        assert!(unavailable.tooltip.starts_with("Codex 사용량\n"));
        let unavailable = taskbar_copy(None, Language::English, "Unavailable", false, None);
        assert!(unavailable.tooltip.starts_with("Codex usage\n"));
    }

    #[test]
    fn taskbar_copy_appends_reset_credits_line_to_tooltip() {
        let row = UsageRowView {
            label: "7d".to_owned(),
            used_percent: 8.0,
            display_percent: 8.0,
            percent_text: "8%".to_owned(),
            reset_text: "2026-07-27 (Mon) 03:00".to_owned(),
            level: UsageLevel::Stable,
            forecast: ForecastView::Hidden,
        };

        let korean = taskbar_copy(
            Some(&row),
            Language::Korean,
            "자동 갱신 중",
            false,
            Some("Full reset 1개 (만료 2026-07-31 (목) 23:59)"),
        );
        assert!(korean
            .tooltip
            .contains("Full reset 1개 (만료 2026-07-31 (목) 23:59)"));
        assert!(korean.tooltip.contains("상태: 안정"));

        let english = taskbar_copy(
            Some(&row),
            Language::English,
            "Polling",
            false,
            Some("Full reset: 1 (expires 2026-07-31 (Thu) 23:59)"),
        );
        assert!(english
            .tooltip
            .contains("Full reset: 1 (expires 2026-07-31 (Thu) 23:59)"));
        assert!(english.tooltip.contains("Status: Healthy"));
    }

    #[test]
    fn taskbar_remaining_percent_never_becomes_negative() {
        let row = UsageRowView {
            label: "7d".to_owned(),
            used_percent: 125.0,
            display_percent: 125.0,
            percent_text: "125%".to_owned(),
            reset_text: "1d".to_owned(),
            level: UsageLevel::Limited,
            forecast: ForecastView::Hidden,
        };

        let copy = taskbar_copy(Some(&row), Language::English, "Critical", true, None);

        assert!(copy.tooltip.contains("Remaining: 0%"));
        assert_eq!(row.used_percent, 125.0);
    }

    #[test]
    fn row_view_shows_used_or_remaining_percent() {
        let window = UsageWindow::new(WindowKind::Primary, 8.0, None, None).unwrap();

        let used = row_view(&window, Language::English, false);
        assert_eq!(used.percent_text, "8%");
        assert_eq!(used.used_percent, 8.0);
        assert_eq!(used.display_percent, 8.0);

        let remaining = row_view(&window, Language::English, true);
        assert_eq!(remaining.percent_text, "92%");
        assert_eq!(remaining.used_percent, 8.0);
        assert_eq!(remaining.display_percent, 92.0);

        let over = UsageWindow::new(WindowKind::Primary, 125.0, None, None).unwrap();
        let remaining_clamped = row_view(&over, Language::English, true);
        assert_eq!(remaining_clamped.percent_text, "0%");
        assert_eq!(remaining_clamped.display_percent, 0.0);
    }

    #[test]
    fn row_view_uses_absolute_local_reset_time_and_unavailable_fallback() {
        let window = UsageWindow::new(
            WindowKind::Secondary,
            8.0,
            Some(10_080),
            Some(std::time::UNIX_EPOCH),
        )
        .unwrap();
        let local_reset = ResetDateTime::new(2026, 7, 27, 1, 3, 4).unwrap();

        let korean = row_view_with_reset_time(&window, Language::Korean, false, Some(local_reset));
        assert_eq!(korean.reset_text, "2026-07-27 (월) 03:04");

        let english =
            row_view_with_reset_time(&window, Language::English, false, Some(local_reset));
        assert_eq!(english.reset_text, "2026-07-27 (Mon) 03:04");

        let unavailable = row_view_with_reset_time(&window, Language::English, false, None);
        assert_eq!(unavailable.reset_text, "Reset unavailable");
    }

    fn sample_forecast(
        now: SystemTime,
        seconds_until_exhaustion: u64,
        before_reset: Option<bool>,
        remaining_at_reset: Option<f64>,
    ) -> Forecast {
        Forecast {
            hourly_rate: 4.0,
            exhaustion_at: now + Duration::from_secs(seconds_until_exhaustion),
            exhausts_before_reset: before_reset,
            expected_used_percent_at_reset: remaining_at_reset.map(|value| 100.0 - value),
            expected_remaining_percent_at_reset: remaining_at_reset,
            sample_count: 8,
            observation_span: Duration::from_secs(2 * 60 * 60),
            quality: ForecastQuality::High,
        }
    }

    #[test]
    fn forecast_view_rounds_duration_and_localizes_state() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let result = forecast_view(
            Some(ForecastResult::ForecastAvailable(sample_forecast(
                now,
                14 * 60 * 60 + 20 * 60,
                None,
                None,
            ))),
            true,
            now,
            Language::English,
        );
        let ForecastView::ForecastAvailable { line } = result else {
            panic!("expected available forecast");
        };
        assert!(line.contains("about 14 hours"), "{line}");

        let long_term = forecast_view(
            Some(ForecastResult::ForecastAvailable(sample_forecast(
                now,
                7 * 24 * 60 * 60,
                None,
                None,
            ))),
            true,
            now,
            Language::English,
        );
        assert_eq!(
            long_term.line(),
            Some("Exhaustion is estimated to be at least 7 days away")
        );
    }

    #[test]
    fn forecast_view_shows_collecting_reset_risk_and_remaining_states() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let collecting = forecast_view(None, true, now, Language::Korean);
        assert_eq!(collecting.line(), Some("예측 데이터 수집 중 · 0/3"));

        let risk = forecast_view(
            Some(ForecastResult::ForecastAvailable(sample_forecast(
                now,
                2 * 60 * 60,
                Some(true),
                Some(0.0),
            ))),
            true,
            now,
            Language::English,
        );
        assert!(risk
            .line()
            .is_some_and(|line| line.contains("Likely exhausted before reset")));

        let remaining = forecast_view(
            Some(ForecastResult::ForecastAvailable(sample_forecast(
                now,
                2 * 24 * 60 * 60,
                Some(false),
                Some(21.4),
            ))),
            true,
            now,
            Language::English,
        );
        assert!(remaining
            .line()
            .is_some_and(|line| line.contains("About 21% remaining at reset")));

        assert!(matches!(
            forecast_view(Some(ForecastResult::Stale), true, now, Language::English),
            ForecastView::Stale { .. }
        ));
        assert_eq!(
            forecast_view(Some(ForecastResult::Invalid), true, now, Language::English).line(),
            Some("Forecast data is unavailable")
        );
        assert_eq!(
            forecast_view(
                Some(ForecastResult::AlreadyExhausted),
                true,
                now,
                Language::English
            )
            .line(),
            Some("Limit exhausted")
        );
    }

    #[test]
    fn forecast_tooltip_lines_keep_primary_before_secondary() {
        let primary = UsageRowView {
            label: "5h".to_owned(),
            used_percent: 50.0,
            display_percent: 50.0,
            percent_text: "50%".to_owned(),
            reset_text: "tomorrow".to_owned(),
            level: UsageLevel::Normal,
            forecast: ForecastView::ForecastAvailable {
                line: "primary estimate".to_owned(),
            },
        };
        let secondary = UsageRowView {
            forecast: ForecastView::Collecting {
                line: "secondary collecting".to_owned(),
            },
            ..primary.clone()
        };
        let tooltip = append_forecast_tooltip(
            "Codex usage\nStatus: Polling",
            Some(&primary),
            Some(&secondary),
            Language::English,
        );
        assert!(tooltip
            .ends_with("Primary window: primary estimate\nSecondary window: secondary collecting"));
        assert!(
            tooltip.find("Primary window").unwrap() < tooltip.find("Secondary window").unwrap()
        );
    }

    #[test]
    fn forecast_duration_uses_localized_plural_units() {
        assert_eq!(
            forecast_duration_text(Duration::from_secs(60), Language::Korean).as_deref(),
            Some("1분")
        );
        assert_eq!(
            forecast_duration_text(Duration::from_secs(60), Language::Japanese).as_deref(),
            Some("1分")
        );
        for language in ALL_LANGUAGES {
            let singular = forecast_duration_text(Duration::from_secs(60), language).unwrap();
            let plural = forecast_duration_text(Duration::from_secs(2 * 60), language).unwrap();
            assert!(singular.contains('1'), "{language:?}: {singular}");
            assert!(plural.contains('2'), "{language:?}: {plural}");
        }
    }
}
