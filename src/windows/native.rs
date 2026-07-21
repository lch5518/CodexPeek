//! Win32 메시지 루프 진입점입니다.

use std::io;

use super::UiBackend;

#[cfg(windows)]
mod platform;

/// 네이티브 단일 인스턴스 UI 메시지 루프를 실행합니다.
pub fn run(backend: &mut dyn UiBackend) -> io::Result<()> {
    #[cfg(windows)]
    {
        platform::run(backend)
    }
    #[cfg(not(windows))]
    {
        let _ = backend;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows UI is unavailable",
        ))
    }
}

/// 진단 모드에서 부모 프로세스의 콘솔에 연결합니다.
pub fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        platform::attach_parent_console();
    }
}

/// 검증된 GitHub 태그 페이지를 기본 브라우저로 엽니다.
pub fn open_validated_tag_page(url: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        unsafe { platform::open_validated_tag_page(url) }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
}
