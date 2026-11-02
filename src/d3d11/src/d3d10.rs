//! D3D10 and D3D10.1 COM object layer over D3D11 (`src/d3d10/d3d10_*.{cpp,hpp}`).

use core::ffi::c_void;

pub mod buffer;
pub mod device;
pub mod input_layout;
pub mod query;
pub mod shader;
pub mod state;
pub mod texture;
pub mod util;
pub mod view;

#[cfg(test)]
mod tests;

pub use buffer::BUFFER_VTBL;
pub use device::{DEVICE_VTBL, Device};
pub use input_layout::INPUT_LAYOUT_VTBL;
pub use query::QUERY_VTBL;
pub use shader::{GEOMETRY_SHADER_VTBL, PIXEL_SHADER_VTBL, VERTEX_SHADER_VTBL};
pub use state::{
    BLEND_STATE_VTBL, DEPTH_STENCIL_STATE_VTBL, RASTERIZER_STATE_VTBL, SAMPLER_STATE_VTBL,
};
pub use texture::{TEXTURE1D_VTBL, TEXTURE2D_VTBL, TEXTURE3D_VTBL};
pub use view::{DSV_VTBL, RTV_VTBL, SRV_VTBL};

/// Embedded D3D10 wrapper subobject (`MTLD3D10Buffer`, `MTLD3D10Texture*`, `MTLD3D10*View`, etc.).
///
/// The layout `[vtbl, d3d11, context]` is shared across all 16 D3D10 wrapper types so
/// [`d3d11_of`] (`GetD3D11Interface`, `d3d10_device.cpp:13-17`) can read `d3d11` at offset
/// `size_of::<usize>()` regardless of which `ID3D10*` subtype is passed.
#[repr(C)]
pub struct Wrapper<V: 'static> {
    pub vtbl: &'static V,
    pub d3d11: *mut c_void,
    pub context: *mut c_void,
}

impl<V: 'static> Wrapper<V> {
    /// Creates a new embedded D3D10 wrapper pointing to `d3d11` and `context`.
    #[must_use]
    pub const fn new(vtbl: &'static V, d3d11: *mut c_void, context: *mut c_void) -> Self {
        Self {
            vtbl,
            d3d11,
            context,
        }
    }
}

/// Unwraps a nullable `ID3D10*` wrapper pointer to `ID3D11*` (`GetD3D11Interface`).
///
/// # Safety
///
/// If non-null, `ptr` points to a live [`Wrapper<V>`].
#[must_use]
pub unsafe fn d3d11_of(ptr: *mut c_void) -> *mut c_void {
    if ptr.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `Wrapper<V>` has `d3d11` at index 1 (`offset_of!(Wrapper<u8>, d3d11)`).
    unsafe { (*ptr.cast::<Wrapper<u8>>()).d3d11 }
}
