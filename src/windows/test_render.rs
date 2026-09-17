//! 실제 GDI 그리기를 메모리 비트맵에서 검증하는 테스트 전용 도우미입니다.

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC, HGDIOBJ,
};

/// 메모리 DC에 테스트 데이터를 그리고 선택적으로 BMP를 저장합니다.
///
/// `paint`는 유효한 DC를 호출 동안 빌립니다. `CODEX_PEEK_RENDER_DIR`가 있을 때만 그 디렉터리에
/// `name.bmp`를 쓰며, 테스트 호출자는 실제 사용자 정보가 없는 고정 데이터만 전달해야 합니다.
pub(crate) fn surface(name: &str, width: i32, height: i32, paint: impl FnOnce(HDC)) {
    assert!(width > 0 && height > 0);
    // SAFETY: 아래 DC와 비트맵은 이 함수가 소유하며 paint 종료 뒤 선택 해제 후 정리합니다.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        assert!(!dc.is_invalid());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            paint(dc);
            if let Some(directory) = std::env::var_os("CODEX_PEEK_RENDER_DIR") {
                let pixels =
                    std::slice::from_raw_parts(bits.cast::<u8>(), (width * height * 4) as usize);
                let mut bmp = Vec::with_capacity(54 + pixels.len());
                bmp.extend_from_slice(b"BM");
                bmp.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
                bmp.extend_from_slice(&[0; 4]);
                bmp.extend_from_slice(&54_u32.to_le_bytes());
                bmp.extend_from_slice(&40_u32.to_le_bytes());
                bmp.extend_from_slice(&width.to_le_bytes());
                bmp.extend_from_slice(&(-height).to_le_bytes());
                bmp.extend_from_slice(&1_u16.to_le_bytes());
                bmp.extend_from_slice(&32_u16.to_le_bytes());
                bmp.extend_from_slice(&[0; 24]);
                bmp.extend_from_slice(pixels);
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(directory.join(format!("{name}.bmp")), bmp).unwrap();
            }
        }));
        SelectObject(dc, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(dc);
        let _ = ReleaseDC(None, screen);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
