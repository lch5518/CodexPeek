//! 런타임 서비스와 Windows UI를 조합하는 애플리케이션 계층입니다.

use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    codex::{locate_supported_cli, AppServerUsageProvider, UsageProvider},
    inspect_settings_for_diagnostics, localized_text,
    windows::{
        autostart::{set_autostart, WindowsRegistry},
        initial_widget_visible, native, resolve_windows_language, taskbar, LaunchMode, UiAction,
        UiBackend, UiSettings, UsageRowView, WidgetViewModel,
    },
    AsyncSettingsWriter, DiagnosticCode, DiagnosticLogger, Language, LanguagePreference,
    LocalizationKey, PollSnapshot, PollingService, SafeDiagnostic, Settings, SettingsStore,
    UpdateCheckIntent, UpdateChecker, UpdatePresentation, UpdateUserAction, UreqHttpClient,
    UsageError, UsageWindow,
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
    runtime.start_update_check(false);
    native::run(&mut runtime)
}

struct AppRuntime {
    settings_writer: AsyncSettingsWriter,
    logger: DiagnosticLogger,
    settings: Settings,
    poller: PollingService,
    startup_hidden: bool,
    update_check_in_flight: Arc<AtomicBool>,
    update_presentation: UpdatePresentation,
}

impl AppRuntime {
    fn new(store: SettingsStore, settings: Settings, startup_hidden: bool) -> io::Result<Self> {
        let poller = start_poller(&settings)?;
        Ok(Self {
            settings_writer: AsyncSettingsWriter::start(store),
            logger: DiagnosticLogger::new(),
            settings,
            poller,
            startup_hidden,
            update_check_in_flight: Arc::new(AtomicBool::new(false)),
            update_presentation: UpdatePresentation::default(),
        })
    }

    fn save_settings(&self) {
        if self.settings_writer.save(self.settings.clone()).is_err() {
            let _ = self
                .logger
                .record_safe(SafeDiagnostic::Settings { valid: false });
        }
    }

    fn start_update_check(&mut self, forced: bool) {
        let Some(checker) = update_checker() else {
            return;
        };
        let now = SystemTime::now();
        let last_check = if forced {
            None
        } else {
            self.settings
                .last_update_check_unix
                .map(|seconds| UNIX_EPOCH + std::time::Duration::from_secs(seconds))
        };
        if !forced
            && last_check.is_some_and(|checked| {
                now.duration_since(checked)
                    .is_ok_and(|elapsed| elapsed < std::time::Duration::from_secs(24 * 60 * 60))
            })
        {
            return;
        }
        if self
            .update_check_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.settings.last_update_check_unix = now
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs());
        self.save_settings();
        let in_flight = Arc::clone(&self.update_check_in_flight);
        let presentation = self.update_presentation.clone();
        let intent = if forced {
            UpdateCheckIntent::UserInitiated
        } else {
            UpdateCheckIntent::Automatic
        };
        thread::spawn(move || {
            if let Ok(update) = checker.check_if_due(&UreqHttpClient, last_check, now) {
                presentation.record_result(intent, update);
            }
            in_flight.store(false, Ordering::Release);
        });
    }

    fn snapshot_inner(&self) -> PollSnapshot {
        self.poller.snapshot()
    }

    fn consume_update_open_request(&self) {
        if let Some(update) = self.update_presentation.take_open_request() {
            let _ = native::open_validated_tag_page(&update.release_url);
        }
    }
}

impl UiBackend for AppRuntime {
    fn snapshot(&self) -> WidgetViewModel {
        self.consume_update_open_request();
        let snapshot = self.snapshot_inner();
        let language = effective_language(self.settings.language);
        let now = SystemTime::now();
        let mut status = if snapshot.is_fetching {
            localized_text(LocalizationKey::Refreshing, language).to_owned()
        } else if let Some(error) = snapshot.last_error {
            error.user_message(language).to_owned()
        } else if snapshot.is_stale {
            localized_text(LocalizationKey::Stale, language).to_owned()
        } else {
            localized_text(LocalizationKey::Polling, language).to_owned()
        };
        if self.update_presentation.available_update().is_some() {
            status.push_str(" · ");
            status.push_str(localized_text(LocalizationKey::UpdateAvailable, language));
        }
        let last_success = snapshot
            .last_success_at
            .and_then(|time| now.duration_since(time).ok())
            .map(|duration| match language {
                Language::Korean => format!("마지막 성공 {}초 전", duration.as_secs()),
                Language::English => format!("Last success {}s ago", duration.as_secs()),
            })
            .unwrap_or_default();
        WidgetViewModel {
            primary: snapshot
                .usage
                .as_ref()
                .and_then(|usage| usage.primary.as_ref())
                .map(|window| row_view(window, language, now)),
            secondary: snapshot
                .usage
                .as_ref()
                .and_then(|usage| usage.secondary.as_ref())
                .map(|window| row_view(window, language, now)),
            status,
            last_success,
            is_stale: snapshot.is_stale,
        }
    }

    fn settings(&self) -> UiSettings {
        self.consume_update_open_request();
        ui_settings(
            &self.settings,
            self.startup_hidden,
            self.update_presentation.available_update().is_some(),
        )
    }

    fn dispatch(&mut self, action: UiAction) -> UiSettings {
        match action {
            UiAction::Refresh => {
                self.poller.refresh();
            }
            UiAction::SetDisplayMode(mode) => self.settings.display_mode = mode,
            UiAction::SetRefreshInterval(minutes) if matches!(minutes, 1 | 5 | 10 | 15 | 30) => {
                if self.settings.refresh_interval_minutes != minutes {
                    self.settings.refresh_interval_minutes = minutes;
                    let _ = self.poller.set_refresh_interval(minutes);
                }
            }
            UiAction::SetRefreshInterval(_) => {}
            UiAction::ToggleAutostart => {
                let enabled = !self.settings.start_with_windows;
                if std::env::current_exe()
                    .and_then(|path| set_autostart(&WindowsRegistry, enabled, &path))
                    .is_ok()
                {
                    self.settings.start_with_windows = enabled;
                } else {
                    let _ = self
                        .logger
                        .record_safe(SafeDiagnostic::Settings { valid: false });
                }
            }
            UiAction::SetStartupView(view) => self.settings.startup_view = view,
            UiAction::RefreshWithAuth => {
                self.poller.refresh_with_auth();
            }
            UiAction::ToggleAutoAuthRefresh => {
                self.settings.auto_auth_refresh = !self.settings.auto_auth_refresh;
                let _ = self
                    .poller
                    .set_auto_auth_refresh(self.settings.auto_auth_refresh);
            }
            UiAction::ToggleAlwaysOnTop => {
                self.settings.always_on_top = !self.settings.always_on_top;
            }
            UiAction::SetLanguage(language) => self.settings.language = language,
            UiAction::ResetPosition => {
                self.settings.floating_position = None;
                self.settings.monitor_device = None;
            }
            UiAction::SaveFloatingPosition {
                position,
                monitor_device,
            } => {
                self.settings.floating_position = Some(position);
                self.settings.monitor_device = monitor_device;
            }
            UiAction::RunDiagnostics => {
                let language = effective_language(self.settings.language);
                thread::spawn(move || {
                    if let Ok(summary) = run_safe_diagnostics(false) {
                        let (title, text) = summary.localized(language);
                        let _ = native::show_diagnostic_summary(title, &text);
                    }
                });
            }
            UiAction::CheckForUpdates => match self.update_presentation.user_action() {
                UpdateUserAction::Open(update) => {
                    let _ = native::open_validated_tag_page(&update.release_url);
                }
                UpdateUserAction::Check => self.start_update_check(true),
            },
            UiAction::ToggleWidget => {
                if self.startup_hidden {
                    self.startup_hidden = false;
                } else {
                    self.settings.widget_visible = !self.settings.widget_visible;
                }
            }
            UiAction::Exit => {}
        }
        self.save_settings();
        ui_settings(
            &self.settings,
            self.startup_hidden,
            self.update_presentation.available_update().is_some(),
        )
    }
}

fn start_poller(settings: &Settings) -> io::Result<PollingService> {
    PollingService::start(
        Arc::new(AppServerUsageProvider::new()),
        settings.refresh_interval_minutes,
        settings.auto_auth_refresh,
    )
    .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn ui_settings(settings: &Settings, startup_hidden: bool, update_available: bool) -> UiSettings {
    UiSettings {
        display_mode: settings.display_mode,
        widget_visible: settings.widget_visible && !startup_hidden,
        refresh_interval_minutes: settings.refresh_interval_minutes,
        always_on_top: settings.always_on_top,
        start_with_windows: settings.start_with_windows,
        startup_view: settings.startup_view,
        auto_auth_refresh: settings.auto_auth_refresh,
        language: settings.language,
        resolved_language: effective_language(settings.language),
        taskbar_offset: settings.taskbar_offset,
        floating_position: settings.floating_position,
        monitor_device: settings.monitor_device.clone(),
        update_available,
    }
}

fn row_view(window: &UsageWindow, language: Language, now: SystemTime) -> UsageRowView {
    UsageRowView {
        label: window.period_label(language),
        used_percent: window.used_percent,
        percent_text: format!("{:.0}%", window.used_percent),
        reset_text: window.remaining_label(language, now),
        level: window.level(),
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
        }
    }
}

const fn pass_fail(value: bool, language: Language) -> &'static str {
    match (value, language) {
        (true, Language::Korean) => "정상",
        (false, Language::Korean) => "확인 필요",
        (true, Language::English) => "OK",
        (false, Language::English) => "needs attention",
    }
}

fn diagnostic_status(value: &'static str, language: Language) -> &'static str {
    if matches!(language, Language::English) {
        return value;
    }
    match value {
        "ok" | "started" => "정상",
        "unavailable" => "사용할 수 없음",
        "failed" | "request failed" => "실패",
        "invalid" => "잘못됨",
        "not checked" => "확인하지 못함",
        "unknown" => "알 수 없음",
        _ => value,
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

    let rpc = AppServerUsageProvider::new().fetch(false);
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
        if let Err(error) = rpc {
            println!("usage_check={}", error.diagnostic_code());
        }
    }
    Ok(summary)
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
