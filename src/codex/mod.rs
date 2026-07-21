mod app_server;
pub(crate) mod locator;
mod process;

pub use app_server::{AppServerUsageProvider, UsageProvider};

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::UsageError;

/// 제한 시간 안에 지원되는 Codex CLI 실행 파일 경로를 안전하게 찾습니다.
///
/// 반환 경로는 버전 확인을 통과한 실행 파일만 가리키며 파일 내용이나 인증 정보는 읽지 않습니다.
pub fn locate_supported_cli() -> Result<PathBuf, UsageError> {
    locator::locate_cli(Instant::now() + Duration::from_secs(5)).map(|candidate| candidate.path)
}
