use core::fmt::Write as _;

use winemetal_wire::{
    WMT_COMPARE_FUNCTION_ALWAYS, WMT_COMPARE_FUNCTION_EQUAL, WMT_COMPARE_FUNCTION_GREATER,
    WMT_COMPARE_FUNCTION_GREATER_EQUAL, WMT_COMPARE_FUNCTION_LESS, WMT_COMPARE_FUNCTION_LESS_EQUAL,
    WMT_COMPARE_FUNCTION_NOT_EQUAL, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR,
    WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE,
    WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT, WMT_SAMPLER_ADDRESS_MODE_REPEAT,
};

use super::*;
use crate::abi::sampler::{
    STATIC_BORDER_COLOR_OPAQUE_WHITE, TEXTURE_ADDRESS_MODE_CLAMP, TEXTURE_ADDRESS_MODE_WRAP,
};

/// A byte no field is ever set to by chance, so "left alone" shows.
const LEFT: u8 = 0xa5;

/// A description whose every field says "the function did not write me".
fn untouched() -> WMTSamplerInfo {
    WMTSamplerInfo {
        min_filter: LEFT,
        mag_filter: LEFT,
        mip_filter: LEFT,
        r_address_mode: LEFT,
        s_address_mode: LEFT,
        t_address_mode: LEFT,
        border_color: LEFT,
        compare_function: LEFT,
        lod_min_clamp: f32::from_bits(0xa5a5_a5a5),
        lod_max_clamp: f32::from_bits(0xa5a5_a5a5),
        max_anisotroy: 0xa5a5_a5a5,
        normalized_coords: LEFT,
        lod_average: LEFT,
        support_argument_buffers: LEFT,
        gpu_resource_id: 0xa5a5_a5a5_a5a5_a5a5,
    }
}

/// The inputs the two descriptions have in common.
#[derive(Clone, Copy)]
struct Input {
    filter: u32,
    address: [u32; 3],
    comparison_func: u32,
    max_anisotropy: u32,
}

impl Input {
    const fn new(filter: u32) -> Self {
        Self {
            filter,
            address: [TEXTURE_ADDRESS_MODE_WRAP; 3],
            comparison_func: 2,
            max_anisotropy: 1,
        }
    }

    const fn address(mut self, u: u32, v: u32, w: u32) -> Self {
        self.address = [u, v, w];
        self
    }

    const fn comparison(mut self, func: u32) -> Self {
        self.comparison_func = func;
        self
    }

    const fn anisotropy(mut self, max: u32) -> Self {
        self.max_anisotropy = max;
        self
    }

    /// The static description: a border colour is the enumeration's.
    const fn static_desc(self, border_color: u32) -> StaticSamplerDesc {
        StaticSamplerDesc {
            filter: self.filter,
            address_u: self.address[0],
            address_v: self.address[1],
            address_w: self.address[2],
            mip_lod_bias: 3.0,
            max_anisotropy: self.max_anisotropy,
            comparison_func: self.comparison_func,
            border_color,
            min_lod: 0.25,
            max_lod: 11.5,
            shader_register: 7,
            register_space: 1,
            shader_visibility: 0,
        }
    }

    /// The dynamic description.
    const fn desc(self, border_color: [f32; 4]) -> SamplerDesc {
        SamplerDesc {
            filter: self.filter,
            address_u: self.address[0],
            address_v: self.address[1],
            address_w: self.address[2],
            mip_lod_bias: 3.0,
            max_anisotropy: self.max_anisotropy,
            comparison_func: self.comparison_func,
            border_color,
            min_lod: 0.25,
            max_lod: 11.5,
        }
    }
}

/// What the C++ writes, and `None` where it leaves the field alone.
struct Want {
    min: u8,
    mag: u8,
    mip: u8,
    /// The S, T and R address modes.
    address: [Option<u8>; 3],
    compare: u8,
    anisotropy: u32,
}

/// Runs both functions on `input` and checks both against `want` and the border colour `border`.
fn check(input: Input, want: &Want, border: u8) {
    let mut warnings = Vec::new();
    let mut from_static = untouched();
    populate_static_sampler_info(
        &mut from_static,
        &input.static_desc(STATIC_BORDER_COLOR_OPAQUE_WHITE),
        &mut |w| warnings.push(w),
    );
    let mut from_dynamic = untouched();
    populate_sampler_info(&mut from_dynamic, &input.desc([1.0; 4]), &mut |w| {
        warnings.push(w);
    });
    assert_eq!(warnings, []);
    for info in [&from_static, &from_dynamic] {
        assert_eq!(info.min_filter, want.min);
        assert_eq!(info.mag_filter, want.mag);
        assert_eq!(info.mip_filter, want.mip);
        assert_eq!(info.s_address_mode, want.address[0].unwrap_or(LEFT));
        assert_eq!(info.t_address_mode, want.address[1].unwrap_or(LEFT));
        assert_eq!(info.r_address_mode, want.address[2].unwrap_or(LEFT));
        assert_eq!(info.compare_function, want.compare);
        assert_eq!(info.max_anisotroy, want.anisotropy);
        assert_eq!(info.lod_min_clamp.to_bits(), 0.25_f32.to_bits());
        assert_eq!(info.lod_max_clamp.to_bits(), 11.5_f32.to_bits());
        assert_eq!(info.border_color, border);
        assert_eq!(info.lod_average, 0);
        assert_eq!(info.normalized_coords, 1);
        assert_eq!(info.support_argument_buffers, 1);
        assert_eq!(info.gpu_resource_id, 0xa5a5_a5a5_a5a5_a5a5);
    }
}

const NEAREST: u8 = WMT_SAMPLER_MIN_MAG_FILTER_NEAREST;
const LINEAR: u8 = WMT_SAMPLER_MIN_MAG_FILTER_LINEAR;
const WHITE: u8 = WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE;

#[test]
fn static_and_dynamic_sampler_infos_as_cpp() {
    // MIN_MAG_MIP_POINT: nothing linear, no anisotropy, no comparison.
    let point = Want {
        min: NEAREST,
        mag: NEAREST,
        mip: WMT_SAMPLER_MIP_FILTER_NEAREST,
        address: [
            Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT),
            Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT),
            Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT),
        ],
        compare: WMT_COMPARE_FUNCTION_NEVER,
        anisotropy: 1,
    };
    check(Input::new(0x00), &point, WHITE);
    // The filter type bits are tested for any bit set, so 2 and 3 are linear as 1 is.
    check(
        Input::new(0x2b),
        &Want {
            min: LINEAR,
            mag: LINEAR,
            mip: WMT_SAMPLER_MIP_FILTER_LINEAR,
            ..point
        },
        WHITE,
    );
    check(
        Input::new(0x10),
        &Want {
            min: LINEAR,
            ..point
        },
        WHITE,
    );
    check(
        Input::new(0x04),
        &Want {
            mag: LINEAR,
            ..point
        },
        WHITE,
    );
    check(
        Input::new(0x01),
        &Want {
            mip: WMT_SAMPLER_MIP_FILTER_LINEAR,
            ..point
        },
        WHITE,
    );
    // The reduction type's bits do not touch the filter types.
    check(Input::new(0x180), &point, WHITE);
}

#[test]
fn anisotropy_follows_the_filter() {
    let aniso = Want {
        min: LINEAR,
        mag: LINEAR,
        mip: WMT_SAMPLER_MIP_FILTER_LINEAR,
        address: [None; 3],
        compare: WMT_COMPARE_FUNCTION_NEVER,
        anisotropy: 0,
    };
    // ANISOTROPIC (0x55) clamps the request to 1 through 16.
    for (request, anisotropy) in [(0, 1), (1, 1), (7, 7), (16, 16), (17, 16), (u32::MAX, 16)] {
        check(
            Input::new(0x55).anisotropy(request).address(0, 6, u32::MAX),
            &Want {
                anisotropy,
                ..aniso
            },
            WHITE,
        );
    }
    // The bit with a point filter (min is point in 0x45), or a filter without the bit, is not
    // anisotropic.
    check(
        Input::new(0x45).anisotropy(8).address(0, 6, u32::MAX),
        &Want {
            min: NEAREST,
            mag: LINEAR,
            mip: WMT_SAMPLER_MIP_FILTER_LINEAR,
            anisotropy: 1,
            ..aniso
        },
        WHITE,
    );
    check(
        Input::new(0x15).anisotropy(8).address(0, 6, u32::MAX),
        &Want {
            anisotropy: 1,
            ..aniso
        },
        WHITE,
    );
    // The comparison variant (0xd5) is anisotropic too, and compares.
    check(
        Input::new(0xd5)
            .anisotropy(4)
            .comparison(8)
            .address(0, 6, u32::MAX),
        &Want {
            anisotropy: 4,
            compare: WMT_COMPARE_FUNCTION_ALWAYS,
            ..aniso
        },
        WHITE,
    );
}

#[test]
fn address_modes_map_and_other_values_are_left_alone() {
    let modes = [
        (1, Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT)),
        (2, Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT)),
        (3, Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE)),
        (4, Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR)),
        (5, Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE)),
        (0, None),
        (6, None),
        (u32::MAX, None),
    ];
    for (mode, mapped) in modes {
        // Each of the three axes in turn, the others clamped.
        let clamp = TEXTURE_ADDRESS_MODE_CLAMP;
        let edge = Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE);
        let want = |address| Want {
            min: NEAREST,
            mag: NEAREST,
            mip: WMT_SAMPLER_MIP_FILTER_NEAREST,
            address,
            compare: WMT_COMPARE_FUNCTION_NEVER,
            anisotropy: 1,
        };
        // A border address mode makes the dynamic colour [1; 4] a supported opaque white.
        check(
            Input::new(0).address(mode, clamp, clamp),
            &want([mapped, edge, edge]),
            WHITE,
        );
        check(
            Input::new(0).address(clamp, mode, clamp),
            &want([edge, mapped, edge]),
            WHITE,
        );
        check(
            Input::new(0).address(clamp, clamp, mode),
            &want([edge, edge, mapped]),
            WHITE,
        );
    }
}

#[test]
fn comparison_functions_map_by_filter() {
    let want = |compare| Want {
        min: LINEAR,
        mag: LINEAR,
        mip: WMT_SAMPLER_MIP_FILTER_LINEAR,
        address: [None; 3],
        compare,
        anisotropy: 1,
    };
    let functions = [
        WMT_COMPARE_FUNCTION_NEVER,
        WMT_COMPARE_FUNCTION_LESS,
        WMT_COMPARE_FUNCTION_EQUAL,
        WMT_COMPARE_FUNCTION_LESS_EQUAL,
        WMT_COMPARE_FUNCTION_GREATER,
        WMT_COMPARE_FUNCTION_NOT_EQUAL,
        WMT_COMPARE_FUNCTION_GREATER_EQUAL,
        WMT_COMPARE_FUNCTION_ALWAYS,
    ];
    for (func, mapped) in (1..=8).zip(functions) {
        // COMPARISON_MIN_MAG_MIP_LINEAR.
        check(
            Input::new(0x95).comparison(func).address(0, 6, 9),
            &want(mapped),
            WHITE,
        );
        // Any other reduction type ignores the function, valid or not.
        for filter in [0x15, 0x115, 0x195] {
            check(
                Input::new(filter).comparison(func).address(0, 6, 9),
                &want(WMT_COMPARE_FUNCTION_NEVER),
                WHITE,
            );
        }
    }
    for filter in [0x15, 0x115, 0x195] {
        check(
            Input::new(filter).comparison(0).address(0, 6, 9),
            &want(WMT_COMPARE_FUNCTION_NEVER),
            WHITE,
        );
    }
}

#[test]
fn an_invalid_comparison_function_is_never_and_a_warning() {
    for func in [0, 9, 100, u32::MAX] {
        let input = Input::new(0x95).comparison(func).address(0, 6, 9);
        let mut warnings = Vec::new();
        let mut info = untouched();
        populate_static_sampler_info(
            &mut info,
            &input.static_desc(STATIC_BORDER_COLOR_OPAQUE_WHITE),
            &mut |w| warnings.push(w),
        );
        assert_eq!(info.compare_function, WMT_COMPARE_FUNCTION_NEVER);
        assert_eq!(warnings, [Warning::InvalidComparisonFunc]);
        let mut info = untouched();
        let mut warnings = Vec::new();
        populate_sampler_info(&mut info, &input.desc([1.0; 4]), &mut |w| warnings.push(w));
        assert_eq!(info.compare_function, WMT_COMPARE_FUNCTION_NEVER);
        assert_eq!(warnings, [Warning::InvalidComparisonFunc]);
    }
}

#[test]
fn static_border_colours_are_the_enumeration() {
    let input = Input::new(0);
    for (border, want) in [
        (0, WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK),
        (1, WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK),
        (2, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE),
        (3, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE),
        (u32::MAX, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE),
    ] {
        let mut info = untouched();
        let mut warned = false;
        populate_static_sampler_info(&mut info, &input.static_desc(border), &mut |_| {
            warned = true;
        });
        assert_eq!(info.border_color, want);
        assert!(!warned);
        // The static colour does not depend on an address mode being BORDER.
        let mut info = untouched();
        populate_static_sampler_info(
            &mut info,
            &input.address(4, 4, 4).static_desc(border),
            &mut |_| warned = true,
        );
        assert_eq!(info.border_color, want);
        assert!(!warned);
    }
}

/// The border colour a dynamic sampler ends with, and the warnings it gave.
fn dynamic_border(address: [u32; 3], color: [f32; 4]) -> (u8, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut info = untouched();
    let input = Input::new(0).address(address[0], address[1], address[2]);
    populate_sampler_info(&mut info, &input.desc(color), &mut |w| warnings.push(w));
    (info.border_color, warnings)
}

#[test]
fn dynamic_border_colours_matter_only_with_a_border_address_mode() {
    let clamp = TEXTURE_ADDRESS_MODE_CLAMP;
    let border = TEXTURE_ADDRESS_MODE_BORDER;
    // Without BORDER on any axis the colour is not looked at: opaque white, no warning.
    for color in [[0.0; 4], [0.0, 0.0, 0.0, 1.0], [0.5; 4], [f32::NAN; 4]] {
        assert_eq!(
            dynamic_border([clamp, clamp, clamp], color),
            (WHITE, vec![])
        );
        assert_eq!(dynamic_border([0, 6, 9], color), (WHITE, vec![]));
    }
    // BORDER on any one axis does.
    for address in [
        [border, clamp, clamp],
        [clamp, border, clamp],
        [clamp, clamp, border],
    ] {
        assert_eq!(
            dynamic_border(address, [0.0; 4]),
            (WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK, vec![])
        );
        assert_eq!(
            dynamic_border(address, [0.0, 0.0, 0.0, 1.0]),
            (WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK, vec![])
        );
        assert_eq!(
            dynamic_border(address, [1.0; 4]),
            (WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE, vec![])
        );
        // Negative zero equals zero, as in the C++'s comparison.
        assert_eq!(
            dynamic_border(address, [-0.0, 0.0, -0.0, 0.0]),
            (WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK, vec![])
        );
    }
}

#[test]
fn an_unsupported_border_colour_is_opaque_white_and_a_warning() {
    let address = [TEXTURE_ADDRESS_MODE_BORDER; 3];
    for color in [
        [0.0, 0.0, 0.0, 0.5],
        [0.0, 0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 0.0],
        [0.5, 0.5, 0.5, 0.5],
        [0.0, 0.0, 0.0, 2.0],
    ] {
        let (border, warnings) = dynamic_border(address, color);
        assert_eq!(border, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE);
        assert_eq!(warnings, [Warning::UnsupportedBorderColor(color)]);
    }
    // A NaN equals nothing, so it is unsupported too.
    let (border, warnings) = dynamic_border(address, [f32::NAN, 0.0, 0.0, 0.0]);
    assert_eq!(border, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn warnings_read_as_the_cpps_log_lines() {
    assert_eq!(
        Warning::InvalidComparisonFunc.to_string(),
        "CreateSamplerState: invalid ComparisonFunc"
    );
    assert_eq!(
        Warning::UnsupportedBorderColor([0.5, 0.0, 1.0, 0.25]).to_string(),
        "CreateSamplerState: Unsupported border color (0.5, 0, 1, 0.25)"
    );
}

/// One golden line's output half as the C++ oracle prints it, from what the function filled.
fn oracle_text(info: &WMTSamplerInfo, warnings: &[Warning]) -> String {
    let mut text = format!(
        " => {} {} {} {} {} {} {} {} {:08x} {:08x} {} {} {} {} {:x} |",
        info.min_filter,
        info.mag_filter,
        info.mip_filter,
        info.r_address_mode,
        info.s_address_mode,
        info.t_address_mode,
        info.border_color,
        info.compare_function,
        info.lod_min_clamp.to_bits(),
        info.lod_max_clamp.to_bits(),
        info.max_anisotroy,
        info.normalized_coords,
        info.lod_average,
        info.support_argument_buffers,
        info.gpu_resource_id,
    );
    for warning in warnings {
        match warning {
            // The oracle names a border colour warning by its kind: the float text is Rust's.
            Warning::UnsupportedBorderColor(_) => text.push_str(" border"),
            Warning::InvalidComparisonFunc => {
                let _ = write!(text, " [{warning}]");
            }
        }
    }
    text
}

/// The hexadecimal numbers of a golden line's input half, after its kind letter.
fn numbers(input: &str) -> Vec<u32> {
    input
        .split_whitespace()
        .skip(1)
        .map(|word| u32::from_str_radix(word, 16).unwrap())
        .collect()
}

/// Every line of the C++ oracle's recording of `d3d12_sampler.cpp` replays equal.
///
/// The recording is 3045 inputs: each filter value, then seeded combinations of edge values for
/// every other field.
#[test]
fn sampler_infos_replay_the_cpp_oracle() {
    let golden = include_str!("golden/golden.txt");
    let mut lines = 0;
    for line in golden.lines() {
        let (input, _) = line.split_once(" => ").unwrap();
        let n = numbers(input);
        let mut warnings = Vec::new();
        let mut info = untouched();
        if input.starts_with("S ") {
            let desc = StaticSamplerDesc {
                filter: n[0],
                address_u: n[1],
                address_v: n[2],
                address_w: n[3],
                mip_lod_bias: 3.0,
                max_anisotropy: n[4],
                comparison_func: n[5],
                border_color: n[6],
                min_lod: f32::from_bits(n[7]),
                max_lod: f32::from_bits(n[8]),
                shader_register: 0,
                register_space: 0,
                shader_visibility: 0,
            };
            populate_static_sampler_info(&mut info, &desc, &mut |w| warnings.push(w));
        } else {
            let desc = SamplerDesc {
                filter: n[0],
                address_u: n[1],
                address_v: n[2],
                address_w: n[3],
                mip_lod_bias: 3.0,
                max_anisotropy: n[4],
                comparison_func: n[5],
                border_color: [8, 9, 10, 11].map(|k| f32::from_bits(n[k])),
                min_lod: f32::from_bits(n[6]),
                max_lod: f32::from_bits(n[7]),
            };
            populate_sampler_info(&mut info, &desc, &mut |w| warnings.push(w));
        }
        assert_eq!(format!("{input}{}", oracle_text(&info, &warnings)), line);
        lines += 1;
    }
    assert_eq!(lines, 3045);
}
