use windows::{
    core::w,
    Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
};

/// Windows 앱 테마 레지스트리를 조회해 시스템 영역이 밝은 테마를 사용하는지 반환합니다.
///
/// 레지스트리 값이 없거나 조회에 실패하면 안전한 기존 동작인 어두운 테마(`false`)로
/// 복구합니다. 이 함수는 인증 또는 설정 파일을 읽지 않으며 호출 스레드에서 읽기 전용
/// 레지스트리 조회 한 번만 수행합니다.
pub(crate) fn system_uses_light_theme() -> bool {
    let value = unsafe {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
        .then_some(value)
    };
    light_theme_from_registry_value(value)
}

fn light_theme_from_registry_value(value: Option<u32>) -> bool {
    value.is_some_and(|value| value != 0)
}

#[cfg(test)]
mod tests {
    use super::light_theme_from_registry_value;

    #[test]
    fn missing_theme_registry_value_falls_back_to_dark() {
        assert!(!light_theme_from_registry_value(None));
        assert!(!light_theme_from_registry_value(Some(0)));
        assert!(light_theme_from_registry_value(Some(1)));
    }
}
