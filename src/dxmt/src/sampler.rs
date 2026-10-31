//! Sampler states, in the two forms a shader binds one.

use std::sync::Arc;

use winemetal_sys::{
    MTLDevice_newSamplerState, Obj,
    wire::{
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        WMT_SAMPLER_MIN_MAG_FILTER_LINEAR, WMTSamplerInfo,
    },
};

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
const fn cube_info(info: &WMTSamplerInfo) -> WMTSamplerInfo {
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
