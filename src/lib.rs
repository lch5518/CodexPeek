pub mod app;
pub mod codex;
mod config;
mod diagnostics;
mod domain;
mod errors;
mod localization;
mod poller;
mod profile_poller;
mod profile_settings;
mod profiles;
mod update_check;
pub mod windows;

pub use app::{ProfileRuntimeCommand, ProfileRuntimeState};

pub use config::{LanguagePreference, Settings, SettingsStore, StartupView, TaskbarDisplayMode};
pub use diagnostics::{
    inspect_settings_for_diagnostics, DiagnosticCode, DiagnosticLogger, SafeDiagnostic,
};
pub use domain::{CodexUsage, ResetCredits, UsageLevel, UsageWindow, WindowKind};
pub use errors::UsageError;
pub use localization::{localized_text, Language, LocalizationKey};
pub use poller::{PollSnapshot, PollState, PollTrigger, PollingService};
pub use profile_poller::{ProfilePollEvent, ProfilePollingService};
pub use profile_settings::{
    NativeProfileFileSystem, ProfileFileSystem, ProfileSettingsEvent, ProfileSettingsMutation,
    ProfileSettingsService,
};
pub use profiles::{
    normalize_profile_label, ManagedUsageProfile, ProfileExecutionContext, ProfileValidationError,
    UsageProfileCatalog, UsageProfileId, UsageProfileRoot, MAX_USAGE_PROFILES,
};
pub use update_check::{
    AvailableUpdate, HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateCheckIntent,
    UpdateCheckStart, UpdateChecker, UpdatePresentation, UpdatePresentationStatus,
    UpdateUserAction, UreqHttpClient,
};
