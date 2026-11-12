use mtld3d_core::sampler_state::{
    description_from_snapshot, key_from_snapshot, snapshot_from_state,
};

use crate::device::fixture::core_on_fakes;

/// D3D9's default sampler states with U clamped, point min/mag, linear mip, MAXMIPLEVEL 2.
fn states() -> [u32; mtld3d_types::SAMPLER_STATE_COUNT] {
    let mut s = mtld3d_types::sampler_state_defaults();
    s[mtld3d_types::D3DSAMP_ADDRESSU as usize] = mtld3d_types::D3DTADDRESS_CLAMP;
    s[mtld3d_types::D3DSAMP_MINFILTER as usize] = mtld3d_types::D3DTEXF_POINT;
    s[mtld3d_types::D3DSAMP_MAGFILTER as usize] = mtld3d_types::D3DTEXF_POINT;
    s[mtld3d_types::D3DSAMP_MIPFILTER as usize] = mtld3d_types::D3DTEXF_LINEAR;
    s[mtld3d_types::D3DSAMP_MAXMIPLEVEL as usize] = 2;
    s
}

/// One Metal sampler per key: the second request is the cached one.
#[test]
fn sampler_cached_by_key() {
    const DEVICE: u64 = 0xc1;
    let mut core = core_on_fakes(DEVICE);
    let snapshot = snapshot_from_state(&states(), false);
    let key = key_from_snapshot(&snapshot);
    let desc = description_from_snapshot(&snapshot, key);
    let a = core.sampler(key, &desc).expect("a sampler");
    let b = core.sampler(key, &desc).expect("a sampler");
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    let calls = dxmt_fakes::take_calls(DEVICE);
    // The state and its cube twin, once (the core's own dummy sampler takes argument buffers).
    let made = calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newSamplerState") && c.ends_with("argument_buffers=0"))
        .count();
    assert_eq!(made, 2, "{calls:#?}");
}

/// The Metal sampler's fields are the description's.
///
/// Metal-valued enums, U/V/W as s/t/r, the LOD clamps, no argument-buffer support (bound directly).
#[test]
fn sampler_info_fields() {
    const DEVICE: u64 = 0xc2;
    let mut core = core_on_fakes(DEVICE);
    let snapshot = snapshot_from_state(&states(), false);
    let key = key_from_snapshot(&snapshot);
    let desc = description_from_snapshot(&snapshot, key);
    core.sampler(key, &desc).expect("a sampler");
    let calls = dxmt_fakes::take_calls(DEVICE);
    let line = calls
        .iter()
        .find(|c| c.starts_with("MTLDevice_newSamplerState") && c.ends_with("argument_buffers=0"))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(line.contains("min=0 mag=0 mip=2 address=0,2,2"), "{line}");
    assert!(line.contains("lod=2.."), "{line}");
    assert!(line.contains("normalized=1 argument_buffers=0"), "{line}");
}

/// The black fallback is one 1x1 texture holding opaque black, uploaded once.
#[test]
fn black_is_opaque_once() {
    const DEVICE: u64 = 0xc3;
    let mut core = core_on_fakes(DEVICE);
    let (a, _) = core.black();
    let (b, _) = core.black();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    assert_eq!(core.black_texel(), [0, 0, 0, 0xff]);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newTexture") && c.contains("size=1x1x1"))
        .count();
    let copies = calls
        .iter()
        .filter(|c| c.contains("blit copy_to_texture"))
        .count();
    assert_eq!((made, copies), (1, 1), "{calls:#?}");
}
