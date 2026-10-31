use core::mem::{offset_of, size_of};

use super::*;

/// The slot width: the vtables' offsets are multiples of it.
const SLOT: usize = size_of::<usize>();

#[test]
#[cfg(target_pointer_width = "64")]
fn upscale_desc_is_88_with_color_at_24() {
    assert_eq!(size_of::<D3D11UpscaleDesc>(), 88);
    assert_eq!(offset_of!(D3D11UpscaleDesc, color), 24);
    assert_eq!(offset_of!(D3D11UpscaleDesc, motion_vector_scale), 56);
    assert_eq!(offset_of!(D3D11UpscaleDesc, exposure_texture), 72);
    assert_eq!(offset_of!(D3D11UpscaleDesc, jitter_offset), 80);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn d3d12_descs_are_the_cpps() {
    assert_eq!(size_of::<D3D12ScalerDesc>(), 56);
    assert_eq!(offset_of!(D3D12ScalerDesc, color_format), 16);
    assert_eq!(offset_of!(D3D12ScalerDesc, auto_exposure), 36);
    assert_eq!(offset_of!(D3D12ScalerDesc, input_content_min_scale), 48);
    assert_eq!(size_of::<D3D12UpscaleDesc>(), 128);
    assert_eq!(offset_of!(D3D12UpscaleDesc, input_content_width), 48);
    assert_eq!(offset_of!(D3D12UpscaleDesc, color_offset), 56);
    assert_eq!(offset_of!(D3D12UpscaleDesc, jitter_offset), 96);
    assert_eq!(offset_of!(D3D12UpscaleDesc, reset), 116);
    assert_eq!(size_of::<D3D11Piece>(), 24);
}

#[test]
fn vtables_follow_the_interfaces() {
    // IUnknown has three slots; each method follows in declaration order, a derived interface after
    // its base's.
    assert_eq!(offset_of!(D3D11ContextExtVtbl, temporal_upscale), 3 * SLOT);
    assert_eq!(offset_of!(D3D11ContextExtVtbl, begin_uav_overlap), 4 * SLOT);
    assert_eq!(offset_of!(D3D11ContextExtVtbl, end_uav_overlap), 5 * SLOT);
    assert_eq!(
        offset_of!(D3D11ContextExt1Vtbl, check_feature_support),
        6 * SLOT
    );
    assert_eq!(offset_of!(D3D11ContextExt2Vtbl, get_pieces), 7 * SLOT);
    assert_eq!(offset_of!(D3D11ContextExt2Vtbl, use_piece), 8 * SLOT);
    assert_eq!(
        offset_of!(D3D11DeviceExtVtbl, set_shader_extension_slot),
        3 * SLOT
    );
    assert_eq!(
        offset_of!(D3D12DeviceExtVtbl, get_temporal_scaler_scale_range),
        3 * SLOT
    );
    assert_eq!(
        offset_of!(D3D12DeviceExtVtbl, create_temporal_scaler),
        4 * SLOT
    );
    assert_eq!(
        offset_of!(D3D12CommandListExtVtbl, temporal_upscale),
        3 * SLOT
    );
}

/// The interface IDs declared in `header`, as `(interface, IID)` in its order.
fn declared(header: &str) -> Vec<(String, String)> {
    header
        .split("DEFINE_COM_INTERFACE(\"")
        .skip(1)
        .map(|rest| {
            let (iid, rest) = rest.split_once('"').unwrap();
            let name = rest.trim_start_matches(',').split(')').next().unwrap();
            (name.trim().to_owned(), iid.to_owned())
        })
        .collect()
}

/// The IIDs are the headers' (`d3d11_interfaces.hpp`, `d3d12_interfaces.hpp`), each under its name.
#[test]
fn iids_are_the_cpps() {
    let d3d11 = declared(include_str!("../../../d3d11/d3d11_interfaces.hpp"));
    let d3d12 = declared(include_str!("../../../d3d12/d3d12_interfaces.hpp"));
    let ours = [
        ("IMTLD3D11ContextExt", &IID_D3D11_CONTEXT_EXT),
        ("IMTLD3D11ContextExt1", &IID_D3D11_CONTEXT_EXT1),
        ("IMTLD3D11ContextExt2", &IID_D3D11_CONTEXT_EXT2),
        ("IMTLD3D11DeviceExt", &IID_D3D11_DEVICE_EXT),
    ];
    assert_eq!(d3d11.len(), 4);
    for (name, iid) in ours {
        assert!(
            d3d11.contains(&(name.to_owned(), iid.to_string())),
            "{name}"
        );
    }
    let ours = [
        ("IMTLD3D12TemporalScaler", &IID_D3D12_TEMPORAL_SCALER),
        ("IMTLD3D12DeviceExt", &IID_D3D12_DEVICE_EXT),
        ("IMTLD3D12CommandListExt", &IID_D3D12_COMMAND_LIST_EXT),
    ];
    assert_eq!(d3d12.len(), 3);
    for (name, iid) in ours {
        assert!(
            d3d12.contains(&(name.to_owned(), iid.to_string())),
            "{name}"
        );
    }
}
