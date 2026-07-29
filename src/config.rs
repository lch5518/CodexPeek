use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::profiles::UsageProfileCatalog;

const SCHEMA_VERSION: u32 = 2;
const MAX_LOGICAL_COORDINATE: i32 = 2_000_000;
const SETTINGS_DIRECTORY: &str = "CodexPeek";
const LEGACY_SETTINGS_DIRECTORY: &str = "CodexUsageMonitor";
static FILE_NONCE: AtomicU64 = AtomicU64::new(0);
static SETTINGS_GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

/// 운영체제 설정 디렉터리 아래에서 CodexPeek 데이터 루트를 결정하고 레거시 루트를 이전합니다.
///
/// `config_dir`은 운영체제가 제공한 사용자별 설정 디렉터리입니다. 새 루트가 없고 레거시
/// `CodexUsageMonitor` 디렉터리만 있으면 디렉터리 자체를 같은 부모 안에서 원자적으로 이동합니다.
/// 새 루트가 이미 있으면 덮어쓰거나 병합하지 않으며, 이동 실패 시 기존 데이터 보존을 위해 레거시
/// 루트를 반환합니다. 인증 파일의 내용은 열거나 복사하지 않습니다.
fn resolve_settings_root(config_dir: &Path) -> PathBuf {
    let current = config_dir.join(SETTINGS_DIRECTORY);
    if fs::symlink_metadata(&current).is_ok() {
        return current;
    }

    let legacy = config_dir.join(LEGACY_SETTINGS_DIRECTORY);
    let legacy_is_directory = fs::symlink_metadata(&legacy)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    if !legacy_is_directory {
        return current;
    }

    match fs::rename(&legacy, &current) {
        Ok(()) => current,
        Err(_) if fs::symlink_metadata(&current).is_ok() => current,
        Err(_) => legacy,
    }
}

/// 시작할 때 표시할 기본 화면을 나타냅니다.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupView {
    /// 위젯을 바로 표시합니다.
    Widget,
    /// 트레이 아이콘만 표시합니다.
    TrayOnly,
}

/// 사용자가 선택한 언어 설정을 나타냅니다.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    /// 운영 체제 언어에 맞춰 표시합니다.
    Auto,
    /// 한국어로 표시합니다.
    Korean,
    /// 영어로 표시합니다.
    English,
    /// 스페인어로 표시합니다.
    Spanish,
    /// 브라질 포르투갈어로 표시합니다.
    PortugueseBrazil,
    /// 인도네시아어로 표시합니다.
    Indonesian,
    /// 일본어로 표시합니다.
    Japanese,
    /// 힌디어로 표시합니다.
    Hindi,
    /// 독일어로 표시합니다.
    German,
    /// 프랑스어로 표시합니다.
    French,
    /// 베트남어로 표시합니다.
    Vietnamese,
    /// 터키어로 표시합니다.
    Turkish,
    /// 아랍어로 표시합니다.
    Arabic,
}

/// 다중 모니터에서 작업표시줄 위젯을 표시할 범위입니다.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarDisplayMode {
    /// 사용 가능한 모든 모니터의 작업표시줄에 표시합니다.
    #[default]
    All,
    /// Windows 주 모니터의 작업표시줄에만 표시합니다.
    Primary,
}

/// 영속화하는 사용자 환경설정입니다.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// 설정 파일 형식 버전입니다.
    pub schema_version: u32,
    /// 자동 새로 고침 주기(분)입니다.
    pub refresh_interval_minutes: u32,
    /// 위젯 표시 여부입니다.
    pub widget_visible: bool,
    /// 작업 표시줄에서 적용할 논리 픽셀 오프셋입니다.
    pub taskbar_offset: i32,
    /// 다중 모니터에서 작업표시줄 위젯을 표시할 범위입니다.
    #[serde(default)]
    pub taskbar_display_mode: TaskbarDisplayMode,
    /// Windows 로그인 때 시작할지 여부입니다.
    pub start_with_windows: bool,
    /// 시작 시 표시할 화면입니다.
    pub startup_view: StartupView,
    /// 자동 인증 갱신 허용 여부입니다.
    pub auto_auth_refresh: bool,
    /// 사용자 언어 선택입니다.
    #[serde(default = "default_language_preference")]
    pub language: LanguagePreference,
    /// 마지막 업데이트 확인의 UNIX 초입니다.
    pub last_update_check_unix: Option<u64>,
    /// 위젯에 남은 한도(%)를 표시할지 여부입니다.
    ///
    /// `false`면 사용량을, `true`면 남은 한도를 큰 숫자로 보여줍니다.
    #[serde(default)]
    pub show_remaining_percent: bool,
    /// 사용량을 조회할 프로필 목록과 현재 선택 상태입니다.
    pub usage_profiles: UsageProfileCatalog,
}

#[derive(Deserialize)]
struct SettingsEnvelope {
    schema_version: u32,
}

#[derive(Deserialize)]
struct LegacySettingsV1 {
    schema_version: u32,
    refresh_interval_minutes: u32,
    widget_visible: bool,
    taskbar_offset: i32,
    #[serde(default)]
    taskbar_display_mode: TaskbarDisplayMode,
    start_with_windows: bool,
    startup_view: StartupView,
    auto_auth_refresh: bool,
    #[serde(default = "default_language_preference")]
    language: LanguagePreference,
    last_update_check_unix: Option<u64>,
    #[serde(default)]
    show_remaining_percent: bool,
}

impl LegacySettingsV1 {
    fn into_current(self) -> Option<Settings> {
        (self.schema_version == 1).then_some(Settings {
            schema_version: SCHEMA_VERSION,
            refresh_interval_minutes: self.refresh_interval_minutes,
            widget_visible: self.widget_visible,
            taskbar_offset: self.taskbar_offset,
            taskbar_display_mode: self.taskbar_display_mode,
            start_with_windows: self.start_with_windows,
            startup_view: self.startup_view,
            auto_auth_refresh: self.auto_auth_refresh,
            language: self.language,
            last_update_check_unix: self.last_update_check_unix,
            show_remaining_percent: self.show_remaining_percent,
            usage_profiles: UsageProfileCatalog::default(),
        })
    }
}

const fn default_language_preference() -> LanguagePreference {
    LanguagePreference::Auto
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            refresh_interval_minutes: 5,
            widget_visible: true,
            taskbar_offset: 0,
            taskbar_display_mode: TaskbarDisplayMode::All,
            start_with_windows: false,
            startup_view: StartupView::Widget,
            auto_auth_refresh: true,
            language: default_language_preference(),
            last_update_check_unix: None,
            show_remaining_percent: false,
            usage_profiles: UsageProfileCatalog::default(),
        }
    }
}

impl Settings {
    fn validate(&self) -> io::Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !matches!(self.refresh_interval_minutes, 1 | 5 | 10 | 15 | 30)
            || self.taskbar_offset < 0
            || self.taskbar_offset > MAX_LOGICAL_COORDINATE
            || self.usage_profiles.validate().is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid settings",
            ));
        }
        Ok(())
    }
}

/// 설정 파일을 안전하게 읽고 쓰는 저장소입니다.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    root: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl SettingsStore {
    /// 기본 앱 데이터 경로를 사용하는 저장소를 만듭니다.
    pub fn new() -> Self {
        let config_dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
        let root = resolve_settings_root(&config_dir);
        Self::for_root(root)
    }

    /// 테스트 또는 이식 가능한 실행을 위해 지정 경로를 사용하는 저장소를 만듭니다.
    ///
    /// `root` 아래에 `settings.json`을 사용합니다. 생성 시에는 디렉터리나 파일을 만들지 않습니다.
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            gate: shared_gate(&root),
            root,
        }
    }

    /// 설정 파일과 관리 프로필 데이터를 보관하는 루트 경로를 반환합니다.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 설정 파일의 전체 경로를 반환합니다.
    pub fn path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// 설정 파일을 변경하지 않고 현재 내용의 유효성을 확인합니다.
    ///
    /// 파일이 없으면 기본 설정을 사용할 수 있으므로 `true`를 반환합니다. 파일 읽기 오류는
    /// 호출자에게 전달하며, JSON 형식이나 스키마 및 필드 검증이 실패하면 `false`를 반환합니다.
    pub fn inspect_validity(&self) -> io::Result<bool> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let contents = match fs::read(self.path()) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        };
        let Ok(envelope) = serde_json::from_slice::<SettingsEnvelope>(&contents) else {
            return Ok(false);
        };
        Ok(match envelope.schema_version {
            SCHEMA_VERSION => serde_json::from_slice::<Settings>(&contents)
                .is_ok_and(|settings| settings.validate().is_ok()),
            1 => serde_json::from_slice::<LegacySettingsV1>(&contents).is_ok_and(|settings| {
                settings
                    .into_current()
                    .is_some_and(|settings| settings.validate().is_ok())
            }),
            _ => false,
        })
    }

    /// 설정을 읽고 손상되었으면 원본을 보관한 뒤 기본값을 반환합니다.
    ///
    /// 파일이 없으면 디렉터리를 만들지 않고 기본값을 반환합니다. JSON·스키마·필드가 유효하지 않으면 원본을
    /// `settings.corrupt-<unix>-<nonce>.json`으로 보관한 뒤 기본값을 반환하며, 읽기 또는 보관 실패는 전달합니다.
    pub fn load(&self) -> io::Result<Settings> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let path = self.path();
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(error) => return Err(error),
        };
        let schema_version = match serde_json::from_slice::<SettingsEnvelope>(&contents) {
            Ok(envelope) => envelope.schema_version,
            Err(_) => {
                self.back_up_corrupt(&path)?;
                return Ok(Settings::default());
            }
        };
        let loaded = match schema_version {
            SCHEMA_VERSION => serde_json::from_slice::<Settings>(&contents).and_then(|settings| {
                settings
                    .validate()
                    .map(|()| settings)
                    .map_err(serde_json::Error::io)
            }),
            1 => serde_json::from_slice::<LegacySettingsV1>(&contents).and_then(|legacy| {
                let settings = legacy.into_current().ok_or_else(|| {
                    serde_json::Error::io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid legacy settings schema",
                    ))
                })?;
                settings
                    .validate()
                    .map(|()| settings)
                    .map_err(serde_json::Error::io)
            }),
            _ => Err(serde_json::Error::io(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported settings schema",
            ))),
        };
        match loaded {
            Ok(settings) if settings.schema_version == SCHEMA_VERSION => {
                if schema_version == 1 {
                    self.save_locked(&settings)?;
                }
                Ok(settings)
            }
            Err(_) => {
                self.back_up_corrupt(&path)?;
                Ok(Settings::default())
            }
            Ok(_) => unreachable!("loaded settings always use the current schema"),
        }
    }

    /// 설정을 같은 디렉터리의 임시 파일을 거쳐 교체 저장합니다.
    ///
    /// `settings`는 저장 전에 검증하며, 유효하지 않으면 대상 파일을 변경하지 않고 오류를 반환합니다.
    /// 성공 시 임시 파일을 flush·sync한 뒤 원자 교체하고, 실패한 임시 파일은 정리합니다.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        settings.validate()?;
        self.save_locked(settings)
    }

    fn save_locked(&self, settings: &Settings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let serialized = serde_json::to_vec_pretty(settings)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let temp = self.root.join(format!(
            ".settings.tmp-{}-{}",
            std::process::id(),
            FILE_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write_result = (|| {
            let mut file = File::options().write(true).create_new(true).open(&temp)?;
            file.write_all(&serialized)?;
            file.flush()?;
            file.sync_all()?;
            atomic_replace(&temp, &self.path())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        write_result
    }

    fn back_up_corrupt(&self, path: &Path) -> io::Result<()> {
        let backup = self.root.join(format!(
            "settings.corrupt-{}-{}-{}.json",
            unix_now(),
            std::process::id(),
            FILE_NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::rename(path, backup)
    }
}

fn shared_gate(root: &Path) -> Arc<Mutex<()>> {
    let root = normalized_path(root);
    let gates = SETTINGS_GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut gates = gates.lock().unwrap_or_else(|error| error.into_inner());
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&root).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(Mutex::new(()));
    gates.insert(root, Arc::downgrade(&gate));
    gate
}

fn normalized_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            },
        };

        let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination_wide: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            MoveFileExW(
                PCWSTR(source_wide.as_ptr()),
                PCWSTR(destination_wide.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|error| io::Error::from_raw_os_error(error.code().0))
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(test)]
mod settings_root_tests {
    use std::{fs, path::PathBuf, sync::atomic::Ordering};

    use super::{resolve_settings_root, FILE_NONCE};

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "codex-peek-{label}-{}-{}",
                std::process::id(),
                FILE_NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_root_uses_codex_peek_without_creating_it() {
        let config_dir = TestRoot::new("new-settings-root");

        let resolved = resolve_settings_root(&config_dir.0);

        assert_eq!(resolved, config_dir.0.join("CodexPeek"));
        assert!(!resolved.exists());
    }

    #[test]
    fn default_root_moves_the_complete_legacy_directory_atomically() {
        let config_dir = TestRoot::new("migrate-settings-root");
        let legacy = config_dir.0.join("CodexUsageMonitor");
        let legacy_home = legacy
            .join("profiles")
            .join("profile-0001")
            .join("codex-home");
        fs::create_dir_all(&legacy_home).unwrap();
        fs::write(legacy.join("settings.json"), b"legacy-settings").unwrap();
        fs::write(legacy_home.join("opaque-marker"), b"nested-data").unwrap();

        let resolved = resolve_settings_root(&config_dir.0);

        assert_eq!(resolved, config_dir.0.join("CodexPeek"));
        assert!(!legacy.exists());
        assert_eq!(
            fs::read(resolved.join("settings.json")).unwrap(),
            b"legacy-settings"
        );
        assert_eq!(
            fs::read(
                resolved
                    .join("profiles")
                    .join("profile-0001")
                    .join("codex-home")
                    .join("opaque-marker")
            )
            .unwrap(),
            b"nested-data"
        );
    }

    #[test]
    fn existing_codex_peek_root_wins_without_merging_legacy_data() {
        let config_dir = TestRoot::new("conflicting-settings-roots");
        let legacy = config_dir.0.join("CodexUsageMonitor");
        let current = config_dir.0.join("CodexPeek");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(legacy.join("settings.json"), b"legacy-settings").unwrap();
        fs::write(current.join("settings.json"), b"current-settings").unwrap();

        let resolved = resolve_settings_root(&config_dir.0);

        assert_eq!(resolved, current);
        assert_eq!(
            fs::read(legacy.join("settings.json")).unwrap(),
            b"legacy-settings"
        );
        assert_eq!(
            fs::read(resolved.join("settings.json")).unwrap(),
            b"current-settings"
        );
    }
}
