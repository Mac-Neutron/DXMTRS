//! Surface classification flags shared with D3D9 device state.

bitflags::bitflags! {
    /// `StretchRect`-eligibility classification of a surface.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct StretchSurfaceFlags: u8 {
        /// The surface is a render target.
        ///
        /// Either a standalone backbuffer/RT, or a
        /// texture-level surface whose texture carries `D3DUSAGE_RENDERTARGET`.
        const IS_RENDER_TARGET = 1 << 0;
        /// The surface is a `CreateOffscreenPlainSurface(D3DPOOL_DEFAULT)` surface.
        ///
        /// A valid `StretchRect` destination, unlike an ordinary
        /// texture-level surface.
        const IS_OFFSCREEN_PLAIN_DEFAULT = 1 << 1;
        /// The surface is a standalone depth-stencil surface (`CreateDepthStencilSurface`).
        ///
        /// `StretchRect` allows only a 1:1
        /// depth→depth copy between two such surfaces.
        const IS_DEPTH_STENCIL = 1 << 2;
    }
}

#[cfg(test)]
mod tests;
