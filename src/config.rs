use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc,
    sync::{Arc, Mutex, OnceLock, Weak},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_LOGICAL_COORDINATE: i32 = 2_000_000;
static FILE_NONCE: AtomicU64 = AtomicU64::new(0);
static SETTINGS_GATES: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

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
            || self.taskbar_offset < 0
            || self.taskbar_offset > MAX_LOGICAL_COORDINATE
            || self.monitor_device.as_ref().is_some_and(|value| {
                value.trim().is_empty() || value.len() > 512 || value.contains(['\r', '\n', '\0'])
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
    gate: Arc<Mutex<()>>,
}

impl SettingsStore {
    /// 기본 앱 데이터 경로를 사용하는 저장소를 만듭니다.
    pub fn new() -> Self {
        let root = dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("CodexUsageMonitor");
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
        Ok(serde_json::from_slice::<Settings>(&contents)
            .is_ok_and(|settings| settings.validate().is_ok()))
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
        match serde_json::from_slice::<Settings>(&contents).and_then(|settings| {
            settings
                .validate()
                .map(|()| settings)
                .map_err(serde_json::Error::io)
        }) {
            Ok(settings) => Ok(settings),
            Err(_) => {
                self.back_up_corrupt(&path)?;
                Ok(Settings::default())
            }
        }
    }

    /// 설정을 같은 디렉터리의 임시 파일을 거쳐 교체 저장합니다.
    ///
    /// `settings`는 저장 전에 검증하며, 유효하지 않으면 대상 파일을 변경하지 않고 오류를 반환합니다.
    /// 성공 시 임시 파일을 flush·sync한 뒤 원자 교체하고, 실패한 임시 파일은 정리합니다.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        settings.validate()?;
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

enum SettingsWriteCommand {
    Save(Settings),
    Flush(mpsc::SyncSender<io::Result<()>>),
    Stop,
}

/// 설정 저장을 제출 순서대로 별도 스레드에서 수행하는 기록기입니다.
pub struct AsyncSettingsWriter {
    sender: mpsc::Sender<SettingsWriteCommand>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl AsyncSettingsWriter {
    /// 지정한 저장소를 소유하는 직렬 설정 기록 스레드를 시작합니다.
    pub fn start(store: SettingsStore) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || settings_writer_loop(store, receiver));
        Self {
            sender,
            worker: Some(worker),
        }
    }

    /// 설정 복사본을 대기하지 않고 저장 대기열에 추가합니다.
    pub fn save(&self, settings: Settings) -> io::Result<()> {
        self.sender
            .send(SettingsWriteCommand::Save(settings))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "settings writer stopped"))
    }

    /// 앞서 제출한 모든 저장이 끝날 때까지 기다리고 첫 I/O 오류를 반환합니다.
    ///
    /// 테스트, 진단 또는 애플리케이션 종료에서만 사용해야 하며 UI 동작 처리 중에는 호출하지 않습니다.
    pub fn flush(&self) -> io::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .send(SettingsWriteCommand::Flush(sender))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "settings writer stopped"))?;
        receiver.recv().map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "settings writer response lost")
        })?
    }

    /// 대기열을 모두 처리하고 기록 스레드를 종료합니다.
    pub fn stop(mut self) -> io::Result<()> {
        let _ = self.sender.send(SettingsWriteCommand::Stop);
        join_settings_writer(self.worker.take())
    }
}

impl Drop for AsyncSettingsWriter {
    fn drop(&mut self) {
        let _ = self.sender.send(SettingsWriteCommand::Stop);
        let _ = join_settings_writer(self.worker.take());
    }
}

fn settings_writer_loop(
    store: SettingsStore,
    receiver: mpsc::Receiver<SettingsWriteCommand>,
) -> io::Result<()> {
    let mut first_error: Option<(io::ErrorKind, String)> = None;
    while let Ok(command) = receiver.recv() {
        match command {
            SettingsWriteCommand::Save(settings) => {
                if let Err(error) = store.save(&settings) {
                    if first_error.is_none() {
                        first_error = Some((error.kind(), error.to_string()));
                    }
                }
            }
            SettingsWriteCommand::Flush(sender) => {
                let result = first_error
                    .as_ref()
                    .map(|(kind, message)| Err(io::Error::new(*kind, message.clone())))
                    .unwrap_or(Ok(()));
                let _ = sender.send(result);
            }
            SettingsWriteCommand::Stop => break,
        }
    }
    first_error
        .map(|(kind, message)| Err(io::Error::new(kind, message)))
        .unwrap_or(Ok(()))
}

fn join_settings_writer(worker: Option<JoinHandle<io::Result<()>>>) -> io::Result<()> {
    match worker {
        Some(worker) => worker
            .join()
            .map_err(|_| io::Error::other("settings writer panicked"))?,
        None => Ok(()),
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
