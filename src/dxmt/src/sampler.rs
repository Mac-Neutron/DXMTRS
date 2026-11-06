//! Sampler states, in the two forms a shader binds one, and the description they are made from.

use core::fmt;
use std::sync::Arc;

use winemetal_sys::{
    MTLDevice_newSamplerState, Obj,
    wire::{
        WMT_COMPARE_FUNCTION_NEVER, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
        WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE, WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
        WMT_SAMPLER_MIN_MAG_FILTER_LINEAR, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
        WMT_SAMPLER_MIP_FILTER_LINEAR, WMT_SAMPLER_MIP_FILTER_NEAREST, WMTSamplerInfo,
    },
};

use crate::state_map;

/// A sampler description, as D3D11 and D3D12 both give it.
pub struct SamplerDesc {
    /// The `D3D*_FILTER`, whose filter-type bits both APIs lay out alike.
    pub filter: u32,
    /// The U, V and W address modes.
    pub address: [u32; 3],
    /// Whether the filter compares, which each API decodes its own way.
    pub comparison: bool,
    pub comparison_func: u32,
    pub max_anisotropy: u32,
    pub min_lod: f32,
    pub max_lod: f32,
    /// A dynamic sampler's border colour; `None` leaves opaque white for the caller to replace.
    pub border_color: Option<[f32; 4]>,
}

/// A line DXMT's C++ logs as a warning while it fills a description.
#[derive(Debug, PartialEq)]
pub enum Warning {
    /// A comparison filter with a `ComparisonFunc` outside `NEVER` to `ALWAYS`.
    InvalidComparisonFunc,
    /// A border colour that is none of the three Metal has, with its components.
    UnsupportedBorderColor([f32; 4]),
}

impl fmt::Display for Warning {
    /// The C++'s message.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComparisonFunc => {
                out.write_str("CreateSamplerState: invalid ComparisonFunc")
            }
            Self::UnsupportedBorderColor([r, g, b, a]) => write!(
                out,
                "CreateSamplerState: Unsupported border color ({r}, {g}, {b}, {a})"
            ),
        }
    }
}

/// Fills `info` from `desc` the way the C++'s `PopulateWMTSamplerInfo` does.
///
/// An address mode outside 1 to 5 and `gpu_resource_id` are left as the caller set them. What the
/// C++ logs is reported through `warn`, so the front end writes it to its own log, or drops it.
pub fn populate(info: &mut WMTSamplerInfo, desc: &SamplerDesc, warn: &mut impl FnMut(Warning)) {
    /// Metal's min or mag filter for a filter-type field, linear for any bit set.
    const fn min_mag(filter_type: u32) -> u8 {
        if filter_type == 0 {
            WMT_SAMPLER_MIN_MAG_FILTER_NEAREST
        } else {
            WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
        }
    }
    let filter = desc.filter;
    let (min, mag, mip) = ((filter >> 4) & 3, (filter >> 2) & 3, filter & 3);
    info.lod_average = 0;
    info.min_filter = min_mag(min);
    info.mag_filter = min_mag(mag);
    info.mip_filter = if mip == 0 {
        WMT_SAMPLER_MIP_FILTER_NEAREST
    } else {
        WMT_SAMPLER_MIP_FILTER_LINEAR
    };
    info.lod_min_clamp = desc.min_lod;
    info.lod_max_clamp = desc.max_lod;
    let anisotropic = filter & 0x40 != 0 && min == 1 && mag == 1 && mip == 1;
    info.max_anisotroy = if anisotropic {
        desc.max_anisotropy.clamp(1, 16)
    } else {
        1
    };
    let modes = [
        &mut info.s_address_mode,
        &mut info.t_address_mode,
        &mut info.r_address_mode,
    ];
    for (mode, &d3d) in modes.into_iter().zip(&desc.address) {
        if let Some(metal) = state_map::address_mode(d3d) {
            *mode = metal;
        }
    }
    info.compare_function = WMT_COMPARE_FUNCTION_NEVER;
    if desc.comparison {
        // `NEVER` (1) to `ALWAYS` (8); state_map also answers 0 as `NEVER`, which D3D refuses.
        match state_map::compare_func(desc.comparison_func).filter(|_| desc.comparison_func != 0) {
            Some(function) => info.compare_function = function,
            None => warn(Warning::InvalidComparisonFunc),
        }
    }
    info.support_argument_buffers = 1;
    info.normalized_coords = 1;
    info.border_color = WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE;
    // `BORDER` (4) is the only mode that reads the colour.
    if let Some(color) = desc.border_color
        && desc.address.contains(&4)
    {
        match color {
            [0.0, 0.0, 0.0, 0.0] => info.border_color = WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
            [0.0, 0.0, 0.0, 1.0] => info.border_color = WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
            [1.0, 1.0, 1.0, 1.0] => {}
            other => warn(Warning::UnsupportedBorderColor(other)),
        }
    }
}

/// A Metal sampler state and its twin for cube textures.
///
/// Behaviour is the C++ `Sampler`'s. Written ahead for the switches of `d3d11.dll` and
/// `d3d12.dll`, which are its first callers.
pub struct Sampler {
    lod_bias: f32,
    state: Obj,
    state_cube: Obj,
    gpu_resource_id: u64,
    gpu_resource_id_cube: u64,
}

impl Sampler {
    /// The sampler `info` describes on `device`, shared as the C++'s `Rc<Sampler>` is.
    ///
    /// `None` when Metal makes neither state; both are made before either is kept.
    #[must_use]
    pub fn create(device: &Obj, info: &WMTSamplerInfo, lod_bias: f32) -> Option<Arc<Self>> {
        let mut info = *info;
        let state = new_state(device, &mut info)?;
        let gpu_resource_id = info.gpu_resource_id;
        let mut cube = cube_info(&info);
        let state_cube = new_state(device, &mut cube)?;
        Some(Arc::new(Self {
            lod_bias,
            state,
            state_cube,
            gpu_resource_id,
            gpu_resource_id_cube: cube.gpu_resource_id,
        }))
    }

    /// The LOD bias the front end asked for, which a shader applies itself.
    #[must_use]
    pub const fn lod_bias(&self) -> f32 {
        self.lod_bias
    }

    /// The state for every texture type but a cube's.
    #[must_use]
    pub const fn state(&self) -> &Obj {
        &self.state
    }

    /// The state for a cube texture.
    #[must_use]
    pub const fn state_cube(&self) -> &Obj {
        &self.state_cube
    }

    /// [`Self::state`]'s id in an argument buffer.
    #[must_use]
    pub const fn gpu_resource_id(&self) -> u64 {
        self.gpu_resource_id
    }

    /// [`Self::state_cube`]'s id in an argument buffer.
    #[must_use]
    pub const fn gpu_resource_id_cube(&self) -> u64 {
        self.gpu_resource_id_cube
    }
}

/// The twin for cube textures: every address mode clamps.
///
/// To the border colour when both filters are linear, to the edge otherwise. As the C++ says, it
/// is probably still wrong, but the best result to be had.
#[must_use]
pub const fn cube_info(info: &WMTSamplerInfo) -> WMTSamplerInfo {
    let mode = if info.min_filter == WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
        && info.mag_filter == WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
    {
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR
    } else {
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
    };
    WMTSamplerInfo {
        r_address_mode: mode,
        s_address_mode: mode,
        t_address_mode: mode,
        ..*info
    }
}

/// A state made from `info`, whose `gpu_resource_id` Metal fills in.
fn new_state(device: &Obj, info: &mut WMTSamplerInfo) -> Option<Obj> {
    // SAFETY: `device` holds a live `MTLDevice`; `info` is a valid sampler description, which the
    // call reads and writes the id of.
    let state = unsafe { MTLDevice_newSamplerState(device.handle(), info) };
    // SAFETY: a new state's handle carries the reference it was made with.
    unsafe { Obj::adopt(state) }
}

#[cfg(test)]
mod tests;
