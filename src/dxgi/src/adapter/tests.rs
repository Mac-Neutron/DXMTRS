use super::*;

/// A device with a 16 GiB working set, discrete unless `unified`, named `name`.
fn facts(unified: bool, name: Option<&[Wchar; 128]>) -> DeviceFacts {
    DeviceFacts {
        name: name.copied(),
        recommended_max: 16 << 30,
        unified,
        registry_id: 0x0102_0304_0506_0708,
    }
}

fn options(desc: Option<&[u8]>, vendor_id: Option<u16>) -> Options {
    Options {
        vendor_id,
        device_id: None,
        device_desc: desc.map(<[u8]>::to_vec),
        force_sdr: false,
    }
}

#[test]
fn vram_halves_on_unified() {
    let unified = desc3(&facts(true, None), &options(None, None), false);
    let discrete = desc3(&facts(false, None), &options(None, None), false);
    assert_eq!(unified.dedicated_video_memory, 8 << 30);
    assert_eq!(discrete.dedicated_video_memory, 16 << 30);
    assert_eq!(video_memory(16 << 30, true, 64), 8 << 30);
}

/// A 32-bit `SIZE_T` reports at most 3 GiB, after the halving; 64 bits report all of it.
#[test]
fn vram_caps_on_i686() {
    assert_eq!(video_memory(16 << 30, true, 32), 0xc000_0000);
    assert_eq!(video_memory(16 << 30, false, 32), 0xc000_0000);
    assert_eq!(video_memory(4 << 30, true, 32), 2 << 30);
    assert_eq!(video_memory(16 << 30, false, 64), 16 << 30);
}

#[test]
fn vendor_custom_beats_nvext() {
    let none = options(None, None);
    assert_eq!(desc3(&facts(true, None), &none, false).vendor_id, 0x106b);
    assert_eq!(desc3(&facts(true, None), &none, true).vendor_id, 0x10de);
    let custom = options(None, Some(0xbeef));
    assert_eq!(desc3(&facts(true, None), &custom, true).vendor_id, 0xbeef);
    assert_eq!(desc3(&facts(true, None), &custom, false).device_id, 0);
}

/// A custom description fills at most 127 units, whatever the device is called; unit 127 stays 0.
#[test]
fn custom_desc_truncates_at_127() {
    let long: Vec<u8> = (0..200u8).map(|i| b'A' + i % 26).collect();
    let mut name: [Wchar; 128] = [0; 128];
    name.fill(Wchar::from(b'x'));
    let desc = desc3(
        &facts(true, Some(&name)),
        &options(Some(&long), None),
        false,
    );
    let want: Vec<Wchar> = long.iter().take(127).map(|&b| Wchar::from(b)).collect();
    assert_eq!(desc.description.get(..127), Some(want.as_slice()));
    assert_eq!(desc.description.get(127), Some(&0));
}

/// A name that does not fit (none from the device) leaves the description all zeros.
#[test]
fn long_name_leaves_zeros() {
    let desc = desc3(&facts(true, None), &options(None, None), false);
    assert!(desc.description.iter().all(|&unit| unit == 0));
    let mut name: [Wchar; 128] = [0; 128];
    name.get_mut(..5)
        .unwrap()
        .copy_from_slice(&b"Metal".map(Wchar::from));
    let named = desc3(&facts(true, Some(&name)), &options(None, None), false);
    assert_eq!(named.description, name);
}

/// The LUID is the registry ID's bytes reversed, and the IDs and flags are the C++'s.
#[test]
fn luid_and_the_fixed_fields() {
    let desc = desc3(&facts(false, None), &options(None, None), false);
    let mut bits = desc.adapter_luid.low_part.to_ne_bytes().to_vec();
    bits.extend_from_slice(&desc.adapter_luid.high_part.to_ne_bytes());
    assert_eq!(bits, 0x0102_0304_0506_0708_u64.swap_bytes().to_ne_bytes());
    assert_eq!(
        (desc.device_id, desc.sub_sys_id, desc.revision, desc.flags),
        (0, 0, 0, 0)
    );
    assert_eq!(
        (desc.dedicated_system_memory, desc.shared_system_memory),
        (0, 0)
    );
}
