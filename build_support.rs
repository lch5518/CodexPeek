const ICON_SIZES: [u8; 3] = [16, 32, 48];
const ICON_HEADER_SIZE: usize = 6;
const ICON_ENTRY_SIZE: usize = 16;
const BITMAP_HEADER_SIZE: usize = 40;

/// Windows 버전 리소스에 사용할 네 개의 16비트 버전 값을 패킹합니다.
///
/// `version`은 `major.minor.patch` 또는 `major.minor.patch.build` 형식이어야 합니다.
/// 각 구성 요소가 16비트 범위를 벗어나거나 숫자가 아니면 오류를 반환합니다.
pub fn version_quad(version: &str) -> Result<u64, &'static str> {
    let version = version.split(['-', '+']).next().unwrap_or(version);
    let components = version.split('.').collect::<Vec<_>>();
    if !(3..=4).contains(&components.len()) {
        return Err("version must have three or four numeric components");
    }
    let mut values = [0_u16; 4];
    for (index, component) in components.iter().enumerate() {
        values[index] = component
            .parse::<u16>()
            .map_err(|_| "version component must fit in 16 bits")?;
    }
    Ok((u64::from(values[0]) << 48)
        | (u64::from(values[1]) << 32)
        | (u64::from(values[2]) << 16)
        | u64::from(values[3]))
}

/// 두 개의 사용량 막대를 표현하는 결정적 멀티사이즈 ICO 데이터를 생성합니다.
///
/// 반환값에는 16, 32, 48픽셀 32비트 DIB 이미지와 투명 마스크가 포함됩니다.
/// 외부 파일이나 시스템 설정을 읽지 않으므로 같은 코드에서는 항상 같은 바이트를 반환합니다.
pub fn usage_meter_icon() -> Vec<u8> {
    let images = ICON_SIZES
        .iter()
        .map(|size| dib_image(*size))
        .collect::<Vec<_>>();
    let entries_size = ICON_ENTRY_SIZE * images.len();
    let mut icon = Vec::with_capacity(
        ICON_HEADER_SIZE + entries_size + images.iter().map(Vec::len).sum::<usize>(),
    );
    push_u16(&mut icon, 0);
    push_u16(&mut icon, 1);
    push_u16(&mut icon, images.len() as u16);

    let mut offset = ICON_HEADER_SIZE + entries_size;
    for (size, image) in ICON_SIZES.iter().zip(&images) {
        icon.push(*size);
        icon.push(*size);
        icon.push(0);
        icon.push(0);
        push_u16(&mut icon, 1);
        push_u16(&mut icon, 32);
        push_u32(&mut icon, image.len() as u32);
        push_u32(&mut icon, offset as u32);
        offset += image.len();
    }
    for image in images {
        icon.extend_from_slice(&image);
    }
    icon
}

fn dib_image(size: u8) -> Vec<u8> {
    let size = usize::from(size);
    let mask_stride = size.div_ceil(32) * 4;
    let pixel_bytes = size * size * 4;
    let mask_bytes = mask_stride * size;
    let mut dib = Vec::with_capacity(BITMAP_HEADER_SIZE + pixel_bytes + mask_bytes);
    push_u32(&mut dib, BITMAP_HEADER_SIZE as u32);
    push_i32(&mut dib, size as i32);
    push_i32(&mut dib, (size * 2) as i32);
    push_u16(&mut dib, 1);
    push_u16(&mut dib, 32);
    push_u32(&mut dib, 0);
    push_u32(&mut dib, pixel_bytes as u32);
    push_i32(&mut dib, 0);
    push_i32(&mut dib, 0);
    push_u32(&mut dib, 0);
    push_u32(&mut dib, 0);

    for source_y in (0..size).rev() {
        for x in 0..size {
            let [red, green, blue, alpha] = icon_pixel(size, x, source_y);
            dib.extend_from_slice(&[blue, green, red, alpha]);
        }
    }
    for source_y in (0..size).rev() {
        let mut row = vec![0_u8; mask_stride];
        for x in 0..size {
            if icon_pixel(size, x, source_y)[3] == 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        dib.extend_from_slice(&row);
    }
    dib
}

fn icon_pixel(size: usize, x: usize, y: usize) -> [u8; 4] {
    let scale = size / 16;
    let radius = 3 * scale;
    let corner_x = if x < radius {
        radius - x
    } else {
        x.saturating_sub(size - radius - 1)
    };
    let corner_y = if y < radius {
        radius - y
    } else {
        y.saturating_sub(size - radius - 1)
    };
    if corner_x > 0 && corner_y > 0 && corner_x * corner_x + corner_y * corner_y > radius * radius {
        return [0, 0, 0, 0];
    }

    let top_start = 4 * scale;
    let bottom_start = 10 * scale;
    let bar_height = 3 * scale;
    let left = 3 * scale;
    let right = size - 3 * scale;
    let in_top = (top_start..top_start + bar_height).contains(&y) && (left..right).contains(&x);
    let in_bottom =
        (bottom_start..bottom_start + bar_height).contains(&y) && (left..right).contains(&x);
    if in_top {
        let fill_end = left + (right - left) * 3 / 5;
        return if x < fill_end {
            [45, 212, 191, 255]
        } else {
            [72, 82, 108, 255]
        };
    }
    if in_bottom {
        let fill_end = left + (right - left) * 4 / 5;
        return if x < fill_end {
            [250, 174, 55, 255]
        } else {
            [72, 82, 108, 255]
        };
    }
    [24, 30, 48, 255]
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(buffer: &mut Vec<u8>, value: i32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}
