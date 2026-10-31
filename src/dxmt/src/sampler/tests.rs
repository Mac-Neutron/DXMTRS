use winemetal_sys::wire::{
    WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
    WMT_SAMPLER_ADDRESS_MODE_REPEAT, WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
    WMT_SAMPLER_MIN_MAG_FILTER_NEAREST, WMTSamplerInfo,
};

use super::cube_info;

fn info(min_filter: u8, mag_filter: u8) -> WMTSamplerInfo {
    WMTSamplerInfo {
        min_filter,
        mag_filter,
        mip_filter: 1,
        r_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        s_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        t_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        border_color: 2,
        compare_function: 3,
        lod_min_clamp: 0.5,
        lod_max_clamp: 12.0,
        max_anisotroy: 16,
        normalized_coords: 1,
        lod_average: 1,
        support_argument_buffers: 1,
        gpu_resource_id: 99,
    }
}

fn modes(info: &WMTSamplerInfo) -> [u8; 3] {
    [
        info.r_address_mode,
        info.s_address_mode,
        info.t_address_mode,
    ]
}

#[test]
fn cube_sampler_clamps_as_cpp() {
    let linear = WMT_SAMPLER_MIN_MAG_FILTER_LINEAR;
    let nearest = WMT_SAMPLER_MIN_MAG_FILTER_NEAREST;
    // Both filters linear: the border colour; any other pair: the edge.
    assert_eq!(
        modes(&cube_info(&info(linear, linear))),
        [WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR; 3]
    );
    for (min, mag) in [(linear, nearest), (nearest, linear), (nearest, nearest)] {
        assert_eq!(
            modes(&cube_info(&info(min, mag))),
            [WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE; 3]
        );
    }
}

#[test]
fn cube_sampler_keeps_everything_else() {
    let src = info(
        WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
        WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
    );
    let cube = cube_info(&src);
    assert_eq!(cube.border_color, src.border_color);
    assert_eq!(cube.compare_function, src.compare_function);
    assert_eq!(cube.lod_min_clamp.to_bits(), src.lod_min_clamp.to_bits());
    assert_eq!(cube.lod_max_clamp.to_bits(), src.lod_max_clamp.to_bits());
    assert_eq!(cube.max_anisotroy, src.max_anisotroy);
    assert_eq!(cube.gpu_resource_id, src.gpu_resource_id);
}
