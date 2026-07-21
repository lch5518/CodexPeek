//! 런타임 서비스와 Windows UI를 조합하는 애플리케이션 계층입니다.

use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    codex::{AppServerUsageProvider, UsageProvider},
    localized_text,
    windows::{
        autostart::{set_autostart, WindowsRegistry},
        initial_widget_visible, native, taskbar, LaunchMode, UiAction, UiBackend, UiSettings,
        UsageRowView, WidgetViewModel,
    },
    DiagnosticCode, DiagnosticLogger, Language, LanguagePreference, LocalizationKey, PollSnapshot,
    PollingService, SafeDiagnostic, Settings, SettingsStore, UpdateChecker, UreqHttpClient,
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
        return run_safe_diagnostics(true);
    }

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
    store: SettingsStore,
    logger: DiagnosticLogger,
    settings: Settings,
    poller: Option<PollingService>,
    startup_hidden: bool,
}

impl AppRuntime {
    fn new(store: SettingsStore, settings: Settings, startup_hidden: bool) -> io::Result<Self> {
        let poller = start_poller(&settings)?;
        Ok(Self {
            store,
            logger: DiagnosticLogger::new(),
            settings,
            poller: Some(poller),
            startup_hidden,
        })
    }

    fn save_settings(&self) {
        if self.store.save(&self.settings).is_err() {
            let _ = self
                .logger
                .record_safe(SafeDiagnostic::Settings { valid: false });
        }
    }

    fn restart_poller(&mut self) {
        self.poller.take();
        match start_poller(&self.settings) {
            Ok(poller) => self.poller = Some(poller),
            Err(_) => {
                let _ = self.logger.record_safe(SafeDiagnostic::Rpc {
                    code: DiagnosticCode::RpcFailed,
                });
            }
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
        self.settings.last_update_check_unix = now
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs());
        self.save_settings();
        thread::spawn(move || {
            if let Ok(Some(update)) = checker.check_if_due(&UreqHttpClient, last_check, now) {
                let _ = native::open_validated_tag_page(&update.release_url);
            }
        });
    }

    fn snapshot_inner(&self) -> PollSnapshot {
        self.poller
            .as_ref()
            .map(PollingService::snapshot)
            .unwrap_or_default()
    }
}

impl UiBackend for AppRuntime {
    fn snapshot(&self) -> WidgetViewModel {
        let snapshot = self.snapshot_inner();
        let language = effective_language(self.settings.language);
        let now = SystemTime::now();
        let status = if snapshot.is_fetching {
            localized_text(LocalizationKey::Refreshing, language).to_owned()
        } else if let Some(error) = snapshot.last_error {
            error.user_message(language).to_owned()
        } else if snapshot.is_stale {
            localized_text(LocalizationKey::Stale, language).to_owned()
        } else {
            localized_text(LocalizationKey::Polling, language).to_owned()
        };
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
        ui_settings(&self.settings, self.startup_hidden)
    }

    fn dispatch(&mut self, action: UiAction) -> UiSettings {
        let mut restart_poller = false;
        match action {
            UiAction::Refresh => {
                if let Some(poller) = &self.poller {
                    poller.refresh();
                }
            }
            UiAction::SetDisplayMode(mode) => self.settings.display_mode = mode,
            UiAction::SetRefreshInterval(minutes) if matches!(minutes, 1 | 5 | 10 | 15 | 30) => {
                if self.settings.refresh_interval_minutes != minutes {
                    self.settings.refresh_interval_minutes = minutes;
                    restart_poller = true;
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
                if let Some(poller) = &self.poller {
                    poller.refresh_with_auth();
                }
            }
            UiAction::ToggleAutoAuthRefresh => {
                self.settings.auto_auth_refresh = !self.settings.auto_auth_refresh;
                restart_poller = true;
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
                thread::spawn(|| {
                    let _ = run_safe_diagnostics(false);
                });
            }
            UiAction::CheckForUpdates => self.start_update_check(true),
            UiAction::ToggleWidget => {
                if self.startup_hidden {
                    self.startup_hidden = false;
                } else {
                    self.settings.widget_visible = !self.settings.widget_visible;
                }
            }
            UiAction::Exit => {}
        }
        if restart_poller {
            self.restart_poller();
        }
        self.save_settings();
        ui_settings(&self.settings, self.startup_hidden)
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

fn ui_settings(settings: &Settings, startup_hidden: bool) -> UiSettings {
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
    match preference {
        LanguagePreference::Korean => Language::Korean,
        LanguagePreference::English => Language::English,
        LanguagePreference::Auto => std::env::var("LANG")
            .ok()
            .filter(|value| value.to_ascii_lowercase().starts_with("ko"))
            .map_or(Language::English, |_| Language::Korean),
    }
}

fn update_checker() -> Option<UpdateChecker> {
    UpdateChecker::new(
        env!("CARGO_PKG_VERSION"),
        option_env!("CARGO_PKG_REPOSITORY").filter(|value| !value.is_empty()),
        64 * 1024,
    )
}

fn run_safe_diagnostics(write_console: bool) -> io::Result<()> {
    let logger = DiagnosticLogger::new();
    let store = SettingsStore::new();
    let settings_valid = store.load().is_ok();
    let _ = logger.record_safe(SafeDiagnostic::Settings {
        valid: settings_valid,
    });

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

    if write_console {
        println!("settings_valid={settings_valid}");
        println!("proxy_present={proxy_present}");
        println!("auth_path={}", safe_path_text(&auth_path));
        println!("auth_exists={auth_exists}");
        println!("taskbar_available={taskbar_available}");
        match rpc {
            Ok(_) => {
                println!("cli=ok");
                println!("rpc=ok");
                println!("login=ok");
                println!("response_format=ok");
            }
            Err(error) => println!("usage_check={}", error.diagnostic_code()),
        }
    }
    Ok(())
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
