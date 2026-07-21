//! HKCU Run 자동 시작 등록입니다.

use std::{io, path::Path};

/// 현재 사용자 자동 시작 레지스트리 키입니다.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 제품 자동 시작 레지스트리 값 이름입니다.
pub const VALUE_NAME: &str = "CodexUsageMonitor";

/// 자동 시작 저장소의 최소 동작을 추상화합니다.
pub trait RegistryBackend {
    /// 자동 시작 명령을 기록합니다.
    fn write(&self, value: &str) -> io::Result<()>;
    /// 현재 자동 시작 명령을 읽습니다.
    fn read(&self) -> io::Result<Option<String>>;
    /// 자동 시작 값을 제거합니다.
    fn remove(&self) -> io::Result<()>;
}

/// 실행 파일 경로를 정확히 인용한 자동 시작 명령을 만듭니다.
pub fn autostart_command(executable: &Path) -> io::Result<String> {
    let path = executable.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "executable path is not valid Unicode",
        )
    })?;
    if path.contains(['"', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "executable path contains an unsafe character",
        ));
    }
    Ok(format!("\"{path}\" --startup"))
}

/// 자동 시작 값을 변경하고 활성화 시 정확한 왕복 값을 검증합니다.
pub fn set_autostart(
    backend: &dyn RegistryBackend,
    enabled: bool,
    executable: &Path,
) -> io::Result<()> {
    if !enabled {
        return backend.remove();
    }
    let command = autostart_command(executable)?;
    backend.write(&command)?;
    if backend.read()?.as_deref() != Some(command.as_str()) {
        return Err(io::Error::other("autostart verification failed"));
    }
    Ok(())
}

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::WindowsRegistry;
