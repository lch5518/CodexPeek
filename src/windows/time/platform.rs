use std::{io, time::SystemTime};

use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
};

use crate::domain::ResetDateTime;

use super::file_time_ticks;

pub(super) fn local_reset_time(value: SystemTime) -> io::Result<ResetDateTime> {
    let ticks = file_time_ticks(value)?;
    let file_time = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();

    // SAFETY: 입력과 출력은 호출이 끝날 때까지 유효한 초기화된 구조체이며 서로 겹치지 않습니다.
    unsafe {
        FileTimeToSystemTime(&file_time, &mut utc).map_err(win_error)?;
        SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).map_err(win_error)?;
    }

    ResetDateTime::new(
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wDayOfWeek,
        local.wHour,
        local.wMinute,
    )
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Windows local time"))
}

fn win_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error)
}
