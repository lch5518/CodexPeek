pub mod app;
pub mod codex;
mod config;
mod diagnostics;
mod domain;
mod errors;
mod forecast;
mod localization;
mod poller;
mod profile_poller;
mod profile_settings;
mod profiles;
pub mod self_update;
mod update_check;
mod usage_forecast;
mod usage_history;
pub mod windows;

pub use app::{ProfileRuntimeCommand, ProfileRuntimeState};

pub use config::{LanguagePreference, Settings, SettingsStore, StartupView, TaskbarDisplayMode};
pub use diagnostics::{
    aggregate_profile_diagnostics, diagnose_profile_contexts, inspect_settings_for_diagnostics,
    AsyncDiagnosticWriter, DiagnosticCode, DiagnosticLogger, ProfileDiagnosticRun,
    ProfileDiagnosticSnapshot, SafeDiagnostic, UsageHistoryOperation,
};
pub use domain::{CodexUsage, ResetCredits, UsageLevel, UsageWindow, WindowKind};
pub use errors::UsageError;
pub use forecast::{
    ConsumptionPaceAssessment, ConsumptionPaceLevel, ConsumptionPaceMetrics, Forecast,
    ForecastAnalysis, ForecastCollectionReason, ForecastEngine, ForecastPolicy, ForecastQuality,
    ForecastResult,
};
pub use localization::{localized_text, Language, LocalizationKey};
pub use poller::{PollSnapshot, PollState, PollTrigger, PollingService};
pub use profile_poller::{ProfilePollEvent, ProfilePollingService, UsageSampleSink};
pub use profile_settings::{
    CorrelatedProfileSettingsEvent, NativeProfileFileSystem, ProfileFileSystem,
    ProfileSettingsEvent, ProfileSettingsMutation, ProfileSettingsOperation,
    ProfileSettingsRequestId, ProfileSettingsService, ProfileSettingsStartup,
    ProfileSettingsStartupReport,
};
pub use profiles::{
    normalize_profile_label, ManagedUsageProfile, ProfileExecutionContext, ProfileValidationError,
    UsageProfileCatalog, UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
};
pub use self_update::{
    apply_update_helper, download_verified_executable, fetch_available_self_update,
    native_update_helper, prepare_and_spawn_update_helper, prepare_update_helper, release_api_url,
    run_update_helper_mode, spawn_prepared_update_helper, DownloadResponse, HelperOutcome,
    HelperPlan, PreparedUpdateHelper, ReleaseAsset, SelfUpdateError, SelfUpdateHttpClient,
    SelfUpdatePlatform, SelfUpdateRelease, SpawnedUpdateHelper, UreqSelfUpdateHttpClient,
    VerifiedExecutable,
};
pub use update_check::{
    AvailableUpdate, HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateCheckIntent,
    UpdateCheckNotice, UpdateCheckStart, UpdateChecker, UpdatePresentation,
    UpdatePresentationStatus, UpdateUserAction, UreqHttpClient,
};
pub use usage_forecast::UsageForecastService;
pub use usage_history::{
    UsageHistory, UsageHistoryError, UsageHistoryRecord, UsageHistoryStore, UsageSample,
};
