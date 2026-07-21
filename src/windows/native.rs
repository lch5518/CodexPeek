//! Win32 메시지 루프 진입점입니다.

use std::io;

use super::UiBackend;

#[cfg(windows)]
mod platform;

/// 프로세스가 살아 있는 동안 이름 있는 뮤텍스를 보유하는 단일 인스턴스 가드입니다.
pub struct SingleInstanceGuard {
    _inner: platform::SingleInstanceGuard,
}

/// 설정 또는 작업자를 시작하기 전에 단일 인스턴스 소유권을 획득합니다.
pub fn acquire_single_instance() -> io::Result<SingleInstanceGuard> {
    #[cfg(windows)]
    {
        platform::acquire_single_instance().map(|inner| SingleInstanceGuard { _inner: inner })
    }
    #[cfg(not(windows))]
    {
        Ok(SingleInstanceGuard(platform::SingleInstanceGuard))
    }
}

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
pub(crate) fn open_validated_tag_page(url: &str) -> io::Result<()> {
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

/// Windows 사용자 UI 언어 식별자와 로캘 이름을 반환합니다.
pub fn user_ui_language() -> (Option<u16>, Option<String>) {
    #[cfg(windows)]
    unsafe {
        platform::user_ui_language()
    }
    #[cfg(not(windows))]
    {
        (None, None)
    }
}

/// 진단 결과를 민감 정보가 없는 모달 Windows 대화 상자로 표시합니다.
pub fn show_diagnostic_summary(title: &str, message: &str) -> io::Result<()> {
    #[cfg(windows)]
    unsafe {
        platform::show_diagnostic_summary(title, message)
    }
    #[cfg(not(windows))]
    {
        let _ = (title, message);
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
}
