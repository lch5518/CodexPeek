#[path = "../build_support.rs"]
mod build_support;

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn icon_is_deterministic_and_contains_expected_dib_sizes() {
    let first = build_support::usage_meter_icon();
    let second = build_support::usage_meter_icon();
    assert_eq!(first, second);
    assert_eq!(&first[..6], &[0, 0, 1, 0, 3, 0]);

    for (index, expected_size) in [16_u8, 32, 48].into_iter().enumerate() {
        let entry = 6 + index * 16;
        assert_eq!(first[entry], expected_size);
        assert_eq!(first[entry + 1], expected_size);
        assert_eq!(read_u16(&first, entry + 4), 1);
        assert_eq!(read_u16(&first, entry + 6), 32);
        let length = read_u32(&first, entry + 8) as usize;
        let offset = read_u32(&first, entry + 12) as usize;
        assert_eq!(read_u32(&first, offset), 40);
        assert_eq!(read_u32(&first, offset + 4), u32::from(expected_size));
        assert_eq!(read_u32(&first, offset + 8), u32::from(expected_size) * 2);
        assert!(offset + length <= first.len());
    }
}

#[test]
fn version_quad_accepts_cargo_versions_and_rejects_invalid_values() {
    assert_eq!(
        build_support::version_quad("1.2.3").unwrap(),
        0x0001_0002_0003_0000
    );
    assert_eq!(
        build_support::version_quad("1.2.3.4-beta.1").unwrap(),
        0x0001_0002_0003_0004
    );
    assert_eq!(
        build_support::version_quad("1.2.3+build.9").unwrap(),
        0x0001_0002_0003_0000
    );
    assert!(build_support::version_quad("1.2").is_err());
    assert!(build_support::version_quad("1.2.70000").is_err());
}
