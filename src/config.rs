use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_LOGICAL_COORDINATE: i32 = 2_000_000;

/// 위젯 표시 위치 방식을 나타냅니다.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    /// 작업 표시줄에 위젯을 표시합니다.
    Taskbar,
    /// 독립된 떠 있는 위젯을 표시합니다.
    Floating,
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
}

/// DPI에 독립적인 떠 있는 위젯의 논리 좌표입니다.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LogicalPosition {
    /// 화면 왼쪽에서 떨어진 논리 픽셀 거리입니다.
    pub x: i32,
    /// 화면 위쪽에서 떨어진 논리 픽셀 거리입니다.
    pub y: i32,
}

/// 영속화하는 사용자 환경설정입니다.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// 설정 파일 형식 버전입니다.
    pub schema_version: u32,
    /// 자동 새로 고침 주기(분)입니다.
    pub refresh_interval_minutes: u32,
    /// 위젯 표시 방식입니다.
    pub display_mode: DisplayMode,
    /// 위젯 표시 여부입니다.
    pub widget_visible: bool,
    /// 작업 표시줄에서 적용할 논리 픽셀 오프셋입니다.
    pub taskbar_offset: i32,
    /// 마지막으로 사용한 모니터 장치 이름입니다.
    pub monitor_device: Option<String>,
    /// 떠 있는 위젯의 저장 좌표입니다.
    pub floating_position: Option<LogicalPosition>,
    /// 떠 있는 위젯을 항상 위에 표시할지 여부입니다.
    pub always_on_top: bool,
    /// Windows 로그인 때 시작할지 여부입니다.
    pub start_with_windows: bool,
    /// 시작 시 표시할 화면입니다.
    pub startup_view: StartupView,
    /// 자동 인증 갱신 허용 여부입니다.
    pub auto_auth_refresh: bool,
    /// 사용자 언어 선택입니다.
    pub language: LanguagePreference,
    /// 마지막 업데이트 확인의 UNIX 초입니다.
    pub last_update_check_unix: Option<u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            refresh_interval_minutes: 5,
            display_mode: DisplayMode::Taskbar,
            widget_visible: true,
            taskbar_offset: 0,
            monitor_device: None,
            floating_position: None,
            always_on_top: true,
            start_with_windows: false,
            startup_view: StartupView::Widget,
            auto_auth_refresh: true,
            language: LanguagePreference::Auto,
            last_update_check_unix: None,
        }
    }
}

impl Settings {
    fn validate(&self) -> io::Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || !matches!(self.refresh_interval_minutes, 1 | 5 | 10 | 15 | 30)
            || self.taskbar_offset.unsigned_abs() > MAX_LOGICAL_COORDINATE as u32
            || self.monitor_device.as_ref().is_some_and(|value| {
                value.is_empty() || value.len() > 512 || value.contains(['\r', '\n', '\0'])
            })
            || self.floating_position.is_some_and(|value| {
                value.x.unsigned_abs() > MAX_LOGICAL_COORDINATE as u32
                    || value.y.unsigned_abs() > MAX_LOGICAL_COORDINATE as u32
            })
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
}

impl SettingsStore {
    /// 기본 앱 데이터 경로를 사용하는 저장소를 만듭니다.
    pub fn new() -> Self {
        let root = dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("CodexUsageMonitor");
        Self { root }
    }

    /// 테스트 또는 이식 가능한 실행을 위해 지정 경로를 사용하는 저장소를 만듭니다.
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 설정 파일의 전체 경로를 반환합니다.
    pub fn path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    /// 설정을 읽고 손상되었으면 원본을 보관한 뒤 기본값을 반환합니다.
    pub fn load(&self) -> Settings {
        let path = self.path();
        let Ok(contents) = fs::read(&path) else {
            return Settings::default();
        };
        match serde_json::from_slice::<Settings>(&contents).and_then(|settings| {
            settings
                .validate()
                .map(|()| settings)
                .map_err(serde_json::Error::io)
        }) {
            Ok(settings) => settings,
            Err(_) => {
                let _ = self.back_up_corrupt(&path);
                Settings::default()
            }
        }
    }

    /// 설정을 같은 디렉터리의 임시 파일을 거쳐 교체 저장합니다.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        settings.validate()?;
        fs::create_dir_all(&self.root)?;
        let serialized = serde_json::to_vec_pretty(settings)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let temp = self.root.join(format!(
            ".settings.tmp-{}-{}",
            std::process::id(),
            unix_now()
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
        let backup = self
            .root
            .join(format!("settings.corrupt-{}.json", unix_now()));
        fs::rename(path, backup)
    }
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
