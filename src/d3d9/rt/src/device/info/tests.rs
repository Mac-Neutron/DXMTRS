use mtld3d_core::gpu_caps::DeviceCapsFlags;

use super::{copy_utf8_prefix, device_caps_from, query_default_device_info};

#[test]
fn apple_silicon_caps_match_unix_parity() {
    let caps = device_caps_from("Apple M3 Max", true, true, true, [true, true, true]);
    assert!(caps.contains(DeviceCapsFlags::SAMPLER_BORDER));
    assert!(caps.contains(DeviceCapsFlags::NATIVE_PACKED16));
    assert!(caps.contains(DeviceCapsFlags::FLOAT32_FILTERING));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_2));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_4));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_8));
    assert!(!caps.contains(DeviceCapsFlags::RESOLVE_NEEDS_RETIRE));
    assert!(!caps.contains(DeviceCapsFlags::PAGES_FROM_UNIX));
}

#[test]
fn paravirtual_caps_drop_sampler_border_and_set_resolve_retire() {
    let caps = device_caps_from(
        "Apple Paravirtual device",
        true,
        true,
        true,
        [true, false, true],
    );
    assert!(!caps.contains(DeviceCapsFlags::SAMPLER_BORDER));
    assert!(caps.contains(DeviceCapsFlags::NATIVE_PACKED16));
    assert!(caps.contains(DeviceCapsFlags::FLOAT32_FILTERING));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_2));
    assert!(!caps.contains(DeviceCapsFlags::SAMPLE_COUNT_4));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_8));
    assert!(caps.contains(DeviceCapsFlags::RESOLVE_NEEDS_RETIRE));
    assert!(!caps.contains(DeviceCapsFlags::PAGES_FROM_UNIX));
}

#[test]
fn mac_only_intel_or_amd_caps_match_unix_parity() {
    let caps = device_caps_from("AMD Radeon Pro", true, false, false, [false, true, false]);
    assert!(caps.contains(DeviceCapsFlags::SAMPLER_BORDER));
    assert!(!caps.contains(DeviceCapsFlags::NATIVE_PACKED16));
    assert!(caps.contains(DeviceCapsFlags::FLOAT32_FILTERING));
    assert!(!caps.contains(DeviceCapsFlags::SAMPLE_COUNT_2));
    assert!(caps.contains(DeviceCapsFlags::SAMPLE_COUNT_4));
    assert!(!caps.contains(DeviceCapsFlags::SAMPLE_COUNT_8));
    assert!(!caps.contains(DeviceCapsFlags::RESOLVE_NEEDS_RETIRE));
    assert!(!caps.contains(DeviceCapsFlags::PAGES_FROM_UNIX));
}

#[test]
fn copy_utf8_prefix_truncates_on_scalar_boundary() {
    let mut dst = [0_u8; 256];
    // 84 * 3 = 252 bytes plus one 4-byte emoji = 256 bytes; limit is 255, so emoji drops cleanly.
    let mut src = "中".repeat(84);
    src.push('🎨');
    let len = copy_utf8_prefix(src.as_bytes(), &mut dst);
    assert_eq!(len, 252);
    assert_eq!(dst[len], 0);
    assert_eq!(core::str::from_utf8(&dst[..len]).unwrap(), "中".repeat(84));
}

#[test]
fn query_default_device_info_runs_on_fakes() {
    let info = query_default_device_info().expect("dxmt-fakes returns FAKE_DEVICES[0]");
    assert_eq!(
        info.registry_id,
        dxmt_fakes::fake_registry_id(dxmt_fakes::FAKE_DEVICES[0])
    );
    assert!(!info.caps.contains(DeviceCapsFlags::PAGES_FROM_UNIX));
    assert!(info.caps.contains(DeviceCapsFlags::SAMPLE_COUNT_2));
    assert!(info.caps.contains(DeviceCapsFlags::SAMPLE_COUNT_4));
    assert!(info.caps.contains(DeviceCapsFlags::SAMPLE_COUNT_8));
    assert!(info.name_len <= info.name.len());
}
