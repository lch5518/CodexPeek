use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
            HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
        },
    },
};

use super::{RegistryBackend, RUN_KEY, VALUE_NAME};

/// 현재 사용자 레지스트리의 Run 키를 사용하는 자동 시작 저장소입니다.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsRegistry;

impl RegistryBackend for WindowsRegistry {
    fn write(&self, value: &str) -> io::Result<()> {
        let key = open_run_key(KEY_READ | KEY_WRITE)?;
        let name = wide(VALUE_NAME);
        let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let bytes =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        status(unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) })
    }

    fn read(&self) -> io::Result<Option<String>> {
        let key = open_run_key(KEY_READ)?;
        let name = wide(VALUE_NAME);
        let mut kind = REG_SZ;
        let mut byte_count = 0_u32;
        let first = unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut byte_count),
            )
        };
        if first == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status(first)?;
        if kind != REG_SZ || byte_count == 0 || byte_count % 2 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "autostart registry value is not REG_SZ",
            ));
        }
        let mut data = vec![0_u16; byte_count as usize / 2];
        status(unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr().cast::<u8>()),
                Some(&mut byte_count),
            )
        })?;
        if data.last() == Some(&0) {
            data.pop();
        }
        String::from_utf16(&data)
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid registry text"))
    }

    fn remove(&self) -> io::Result<()> {
        let key = open_run_key(KEY_WRITE)?;
        let name = wide(VALUE_NAME);
        let result = unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) };
        if result == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            status(result)
        }
    }
}

struct OwnedKey(HKEY);

impl Drop for OwnedKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> io::Result<OwnedKey> {
    let path = wide(RUN_KEY);
    let mut key = HKEY::default();
    status(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    })?;
    Ok(OwnedKey(key))
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

fn status(error: WIN32_ERROR) -> io::Result<()> {
    if error == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error.0 as i32))
    }
}
