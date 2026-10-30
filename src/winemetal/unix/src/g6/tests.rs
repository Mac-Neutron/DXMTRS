use core::{ffi::c_void, ptr};

use objc2_core_graphics::CGMainDisplayID;
use winemetal_wire::{
    MemPtr, UnixcallQueryDisplaySetting, UnixcallUpdateDisplaySetting, WMT_COLOR_SPACE_INVALID,
    WMTHDRMetadata,
};

use super::{
    display::{query_display_setting, update_display_setting, xy},
    *,
};
use crate::float_cast::f64_to_f32;

/// An ICC `XYZ ` record: the signature, 4 reserved bytes, then X, Y and Z in s15.16.
fn record(sig: [u8; 4], xyz: [i32; 3]) -> Vec<u8> {
    let mut r = sig.to_vec();
    r.extend_from_slice(&[0; 4]);
    for v in xyz {
        r.extend_from_slice(&v.to_be_bytes());
    }
    r
}

#[test]
fn xy_reads_an_xyz_record() {
    // D50 (0.9642, 1, 0.8249), as a display profile's white point.
    let d50 = [0xF6D6, 0x1_0000, 0xD32D];
    let [x, y] = xy(&record(*b"XYZ ", d50)).expect("an XYZ record");
    let [cx, cy, cz] = d50.map(|v| f64::from(v) / 65536.0);
    assert_eq!(x.to_bits(), f64_to_f32(cx / (cx + cy + cz)).to_bits());
    assert_eq!(y.to_bits(), f64_to_f32(cy / (cx + cy + cz)).to_bits());
    assert!(xy(&record(*b"XYZX", d50)).is_none());
    assert!(xy(&record(*b"XYZ ", d50)[..19]).is_none());
    let mut long = record(*b"XYZ ", d50);
    long.push(0);
    assert!(xy(&long).is_none());
}

#[test]
fn colorspace_names() {
    // SAFETY: CoreGraphics' constants.
    let srgb = unsafe { kCGColorSpaceSRGB };
    // SAFETY: as above.
    let linear = unsafe { kCGColorSpaceExtendedLinearSRGB };
    // SAFETY: as above.
    let bt2020 = unsafe { kCGColorSpaceITUR_2020_sRGBGamma };
    // SAFETY: as above.
    let pq = unsafe { kCGColorSpaceITUR_2100_PQ };
    let want = [(0, srgb), (1, linear), (2, bt2020), (4, pq), (5, linear)];
    for (cs, name) in want {
        assert!(
            colorspace_name(cs).is_some_and(|n| ptr::eq(n, name)),
            "{cs}"
        );
    }
    for cs in [3, 6, 7, 8, WMT_COLOR_SPACE_INVALID] {
        assert!(colorspace_name(cs).is_none(), "{cs}");
    }
}

/// The metadata slot 99 answers for `display`, `None` when unset.
fn query(display: u64) -> Option<(u64, u16)> {
    let mut m = WMTHDRMetadata {
        red_primary: [0; 2],
        green_primary: [0; 2],
        blue_primary: [0; 2],
        white_point: [0; 2],
        max_mastering_luminance: 0,
        min_mastering_luminance: 0,
        max_content_light_level: 0,
        max_frame_average_light_level: 0,
    };
    let mut q = UnixcallQueryDisplaySetting {
        display_id: display,
        colorspace: u64::MAX,
        hdr_metadata: MemPtr {
            ptr: ptr::from_mut(&mut m).expose_provenance() as u64,
        },
        ret: 7,
    };
    // SAFETY: a live params block.
    unsafe { query_display_setting(ptr::from_mut(&mut q).cast::<c_void>()) };
    (q.ret == 1).then_some((q.colorspace, m.max_content_light_level))
}

/// Slot 100 for `display`: `level` as the metadata's content light level, or unset.
fn update(display: u64, colorspace: u64, level: Option<u16>) {
    let m = level.map(|level| WMTHDRMetadata {
        red_primary: [0; 2],
        green_primary: [0; 2],
        blue_primary: [0; 2],
        white_point: [0; 2],
        max_mastering_luminance: 0,
        min_mastering_luminance: 0,
        max_content_light_level: level,
        max_frame_average_light_level: 0,
    });
    let addr = m
        .as_ref()
        .map_or(0, |m| ptr::from_ref(m).expose_provenance() as u64);
    let mut u = UnixcallUpdateDisplaySetting {
        display_id: display,
        colorspace,
        hdr_metadata: MemPtr { ptr: addr },
    };
    // SAFETY: a live params block.
    unsafe { update_display_setting(ptr::from_mut(&mut u).cast::<c_void>()) };
}

#[test]
fn settings_split_the_main_display_from_the_rest() {
    let main = u64::from(CGMainDisplayID());
    update(main, 4, Some(1000));
    update(0xDEAD_BEEF, 5, Some(400));
    assert_eq!(query(main), Some((4, 1000)));
    // Every other ID shares one setting; the ID is a 32-bit display's.
    assert_eq!(query(0), Some((5, 400)));
    assert_eq!(query(main | 1 << 32), Some((4, 1000)));
    update(main, 0, None);
    assert_eq!(query(main), None);
    assert_eq!(query(0xDEAD_BEEF), Some((5, 400)));
    update(0, 0, None);
    assert_eq!(query(0xDEAD_BEEF), None);
}
