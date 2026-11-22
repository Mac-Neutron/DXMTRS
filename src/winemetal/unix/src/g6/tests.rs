use core::{ffi::c_void, ptr};

use objc2_core_graphics::CGMainDisplayID;
use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice, MTLSharedEvent};
use winemetal_wire::{
    MemPtr, UnixcallGenericObjRet, UnixcallMtldrawableSetpresentedevent,
    UnixcallQueryDisplaySetting, UnixcallUpdateDisplaySetting, UnixcallWmtViewSyncAndPoll,
    WMT_COLOR_SPACE_INVALID, WMT_PRESENTED_EVENT_FLAG_PACED,
    WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY, WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED,
    WMT_VIEW_POLL_FLAG_CURSOR_KICK, WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING,
    WMT_VIEW_POLL_FLAG_HDR_ACTIVE, WMT_VIEW_POLL_FLAG_OCCLUDED,
    WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE, WMTHDRMetadata, WMTViewPollOut,
};

use super::{
    attachment::{
        self, AttachLatches, CURSOR_POLICY_AUTO, STATIC_HDR_ENABLE, UNDISPLAYED_PACED_LIMIT,
    },
    cursor_overlay::sprite_hash,
    display::{query_display_setting, update_display_setting, xy},
    window::{
        detach_metal_view_if_attached, mtl_drawable_set_presented_event,
        wmt_declare_latency_critical_activity, wmt_view_sync_and_poll,
    },
    *,
};
use crate::{float_cast::f64_to_f32, handle::ret_owned};

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

#[test]
fn latency_critical_activity_returns_active() {
    let mut args = UnixcallGenericObjRet { ret: 0 };
    // SAFETY: live stack params block for slot 158.
    unsafe { wmt_declare_latency_critical_activity(ptr::from_mut(&mut args).cast::<c_void>()) };
    assert_eq!(args.ret, 1);
}

#[test]
fn view_sync_and_poll_reads_and_clears_latched_flags() {
    let view = 0x1000_0010usize;
    let (att, prev) = attachment::register(
        view,
        0x1000_0020,
        &AttachLatches {
            hwnd: 0x42,
            static_flags: STATIC_HDR_ENABLE,
            hdr_active: true,
            software_cursor_policy: CURSOR_POLICY_AUTO,
            backing_scale: 2,
            edr_current: 2.5,
            edr_potential: 4.0,
            panel_max_hz: 120,
            screen_id: 0,
            client_surface: 0,
        },
    );
    assert!(prev.is_none());
    att.set_window_occluded(true);
    att.set_client_surface_dirty();
    att.set_cursor_kick();
    att.mark_colorspace_changed();

    let mut out = WMTViewPollOut {
        drawable_width: 0,
        drawable_height: 0,
        backing_scale: 0,
        edr_current: 0.0,
        edr_potential: 0.0,
        panel_max_hz: 0,
        flags: 0,
        reserved: 1,
    };
    let mut args = UnixcallWmtViewSyncAndPoll {
        view: view as u64,
        layer: 0,
        out: MemPtr {
            ptr: ptr::from_mut(&mut out).expose_provenance() as u64,
        },
        flags: 0,
        reserved: 0,
    };
    // SAFETY: live stack params block for slot 161.
    unsafe { wmt_view_sync_and_poll(ptr::from_mut(&mut args).cast::<c_void>()) };
    assert_eq!(out.backing_scale, 2);
    assert_eq!(out.edr_current.to_bits(), 2.5_f32.to_bits());
    assert_eq!(out.edr_potential.to_bits(), 4.0_f32.to_bits());
    assert_eq!(out.panel_max_hz, 120);
    assert_eq!(
        out.flags,
        WMT_VIEW_POLL_FLAG_HDR_ACTIVE
            | WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE
            | WMT_VIEW_POLL_FLAG_OCCLUDED
            | WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY
            | WMT_VIEW_POLL_FLAG_CURSOR_KICK
            | WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED
    );

    // Second poll observes the edge latches cleared while steady-state bits stay set.
    // SAFETY: live stack params block for slot 161.
    unsafe { wmt_view_sync_and_poll(ptr::from_mut(&mut args).cast::<c_void>()) };
    assert_eq!(
        out.flags,
        WMT_VIEW_POLL_FLAG_HDR_ACTIVE
            | WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE
            | WMT_VIEW_POLL_FLAG_OCCLUDED
    );

    assert!(detach_metal_view_if_attached(view as u64).is_some());
    assert!(detach_metal_view_if_attached(view as u64).is_none());
}

#[test]
fn presented_event_tracks_undisplayed_run_and_signals_missing_drawable() {
    let view = 0x1000_0030usize;
    let (att, _) = attachment::register(
        view,
        0x1000_0040,
        &AttachLatches {
            hwnd: 0x43,
            static_flags: 0,
            hdr_active: false,
            software_cursor_policy: CURSOR_POLICY_AUTO,
            backing_scale: 1,
            edr_current: 1.0,
            edr_potential: 1.0,
            panel_max_hz: 60,
            screen_id: 0,
            client_surface: 0,
        },
    );
    att.set_window_occluded(true);
    for _ in 0..(UNDISPLAYED_PACED_LIMIT + 4) {
        assert_eq!(att.note_paced_present(false), None);
    }
    assert!(!att.displays_nothing());

    att.set_window_occluded(false);
    for step in 1..UNDISPLAYED_PACED_LIMIT {
        assert_eq!(att.note_paced_present(false), None, "step {step}");
        assert!(!att.displays_nothing());
    }
    assert_eq!(att.note_paced_present(false), Some(true));
    assert!(att.displays_nothing());

    let mut out = WMTViewPollOut {
        drawable_width: 0,
        drawable_height: 0,
        backing_scale: 0,
        edr_current: 0.0,
        edr_potential: 0.0,
        panel_max_hz: 0,
        flags: 0,
        reserved: 0,
    };
    att.write_poll_out(0, 0, &mut out);
    assert_ne!(out.flags & WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING, 0);

    assert_eq!(att.note_paced_present(true), Some(false));
    assert!(!att.displays_nothing());

    if let Some(device) = MTLCreateSystemDefaultDevice()
        && let Some(shared_event) = device.newSharedEvent()
    {
        let event_handle = ret_owned(Some(shared_event));
        let mut args = UnixcallMtldrawableSetpresentedevent {
            drawable: 0,
            view: view as u64,
            shared_event: event_handle,
            signal_value: 7,
            flags: WMT_PRESENTED_EVENT_FLAG_PACED,
            reserved: 0,
        };
        // SAFETY: live stack params block for slot 163 with null drawable.
        unsafe { mtl_drawable_set_presented_event(ptr::from_mut(&mut args).cast::<c_void>()) };
        // SAFETY: `event_handle` came from `ret_owned` and owns +1 ref.
        let ev = unsafe {
            objc2::rc::Retained::from_raw(
                event_handle as *mut objc2::runtime::ProtocolObject<dyn MTLSharedEvent>,
            )
        };
        assert_eq!(ev.map(|e| e.signaledValue()), Some(7));
    }
    assert!(detach_metal_view_if_attached(view as u64).is_some());
}

#[test]
fn table_preserves_slot_83_and_wires_158_through_164() {
    assert_eq!(crate::table::UNIX_CALL_FUNCS.len(), 165);
    assert!(crate::table::UNIX_CALL_FUNCS[83].is_none());
    for slot_idx in 158..=164 {
        assert!(
            crate::table::UNIX_CALL_FUNCS[slot_idx].is_some(),
            "slot {slot_idx}"
        );
    }
}

#[test]
fn cursor_sprite_hash_distinguishes_hotspot_and_avoids_zero() {
    let pixels = [0x11u8, 0x22, 0x33, 0xff];
    let h1 = sprite_hash(1, 1, 0, 0, &pixels);
    let h2 = sprite_hash(1, 1, 1, 0, &pixels);
    let h3 = sprite_hash(2, 1, 0, 0, &pixels);
    assert_ne!(h1, 0);
    assert_ne!(h2, 0);
    assert_ne!(h3, 0);
    assert_ne!(h1, h2);
    assert_ne!(h1, h3);
}
