//! 초기화 시각을 Windows 현지 시간대로 변환하는 경계입니다.

use std::{
    io,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::domain::ResetDateTime;

#[cfg(windows)]
mod platform;

const WINDOWS_EPOCH_OFFSET_TICKS: u64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: u64 = 10_000_000;

/// 시스템 시각을 현재 Windows 시간대의 달력 구성 요소로 변환합니다.
///
/// Windows가 아닌 대상이나 FILETIME으로 표현할 수 없는 값은 오류로 반환합니다.
pub(crate) fn local_reset_time(value: SystemTime) -> io::Result<ResetDateTime> {
    #[cfg(windows)]
    {
        platform::local_reset_time(value)
    }
    #[cfg(not(windows))]
    {
        let _ = value;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows local time is unavailable",
        ))
    }
}

fn file_time_ticks(value: SystemTime) -> io::Result<u64> {
    let duration = value.duration_since(UNIX_EPOCH).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "reset time predates the Unix epoch",
        )
    })?;
    let seconds = duration
        .as_secs()
        .checked_mul(TICKS_PER_SECOND)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "reset time overflow"))?;
    let subsecond_ticks = u64::from(duration.subsec_nanos() / 100);

    WINDOWS_EPOCH_OFFSET_TICKS
        .checked_add(seconds)
        .and_then(|ticks| ticks.checked_add(subsecond_ticks))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "reset time overflow"))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::file_time_ticks;

    #[test]
    fn file_time_ticks_preserve_the_windows_epoch_offset() {
        assert_eq!(
            file_time_ticks(UNIX_EPOCH).unwrap(),
            116_444_736_000_000_000
        );
        assert_eq!(
            file_time_ticks(UNIX_EPOCH + Duration::from_secs(1)).unwrap(),
            116_444_736_010_000_000
        );
    }

    #[test]
    fn file_time_ticks_reject_pre_unix_epoch_values() {
        assert!(file_time_ticks(UNIX_EPOCH - Duration::from_secs(1)).is_err());
    }
}
