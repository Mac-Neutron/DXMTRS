use super::*;

#[test]
fn values_are_the_sdk_ones() {
    assert_eq!(DXGI_FORMAT_R8G8B8A8_UNORM, 28);
    assert_eq!(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 91);
    assert_eq!(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, 17);
    assert_eq!(DXGI_USAGE_UNORDERED_ACCESS, 0x400);
    assert_eq!(DXGI_USAGE_BACK_BUFFER, 0x40);
    assert_eq!(D3D11_BIND_UNORDERED_ACCESS, 0x80);
}

/// A format number listed twice under two names would be a copy slip.
#[test]
fn format_numbers_are_distinct() {
    let source = include_str!("../dxgi_consts.rs");
    let mut seen: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("pub const DXGI_FORMAT_") {
            let (name, value) = rest.split_once(": u32 = ").unwrap();
            assert!(
                seen.iter().all(|(_, v)| v != value),
                "{name} shares its number {value}"
            );
            seen.push((name.to_owned(), value.to_owned()));
        }
    }
    assert!(seen.len() > 30);
}
