//! D3D9's samplers on the core (DXMTRS C4b-3b), and the black an unbound slot reads.
//!
//! A sampler is made once per [`SamplerKey`] from mtld3d-core's [`SamplerDescription`], whose
//! fields already hold Metal's values; D3D9 binds it directly, so it is made without argument-
//! buffer support (C4b3b-5). An unbound sampled slot reads opaque black: one 1x1 2D texture
//! (the emitter types an unbound slot 2D, C4b3b-7), kept in the texture store under an id the
//! front end never mints.

use std::sync::Arc;

use dxmt_core::{
    resource::{Texture, TextureViewKey},
    sampler::Sampler,
};
use mtld3d_core::{
    ids::TextureId,
    sampler_state::{SamplerDescription, SamplerFlags, SamplerKey},
};
use winemetal_sys::wire::{
    WMT_COMPARE_FUNCTION_LESS_EQUAL, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMTSamplerInfo,
};

use super::{CoreDevice, CoreTextureDesc, TextureKind, UploadRegion};

/// The black texture's id: past every id the front end mints (they count up from 1).
const BLACK: TextureId = TextureId::from_raw(u64::MAX);

/// The black texture's one texel, BGRA: opaque black, as D3D9 reads an unbound slot.
const BLACK_TEXEL: [u8; 4] = [0, 0, 0, 0xff];

impl CoreDevice {
    /// The sampler of `key`, made from `desc` at its first request; `None` when Metal makes none.
    pub fn sampler(&mut self, key: SamplerKey, desc: &SamplerDescription) -> Option<Arc<Sampler>> {
        if let Some(sampler) = self.samplers.get(&key.raw()) {
            return Some(Arc::clone(sampler));
        }
        let info = WMTSamplerInfo {
            min_filter: desc.min_filter as u8,
            mag_filter: desc.mag_filter as u8,
            mip_filter: desc.mip_filter as u8,
            r_address_mode: desc.address_w as u8,
            s_address_mode: desc.address_u as u8,
            t_address_mode: desc.address_v as u8,
            border_color: desc.border_color as u8,
            // A shadow sample's sampler: D3D9's hardware shadow filter is less-equal (1 is lit).
            compare_function: if desc.flags.contains(SamplerFlags::IS_COMPARE) {
                WMT_COMPARE_FUNCTION_LESS_EQUAL
            } else {
                0
            },
            lod_min_clamp: desc.lod_min_clamp,
            lod_max_clamp: desc.lod_max_clamp,
            max_anisotroy: desc.max_anisotropy.max(1),
            normalized_coords: 1,
            lod_average: 0,
            support_argument_buffers: 0,
            gpu_resource_id: 0,
        };
        let sampler = Sampler::create(self.device.device(), &info, 0.0)?;
        self.samplers.insert(key.raw(), Arc::clone(&sampler));
        Some(sampler)
    }

    /// The 1x1 opaque-black 2D texture and its view, made and uploaded at the first request.
    ///
    /// # Panics
    ///
    /// Never: the texture is in the store once made.
    pub fn black(&mut self) -> (Arc<Texture>, TextureViewKey) {
        if !self.has_texture(BLACK) {
            let desc = CoreTextureDesc {
                pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                kind: TextureKind::Tex2d,
                width: 1,
                height: 1,
                depth: 1,
                levels: 1,
                view_twins: false,
                block: (1, 1, 4),
                render_target: false,
            };
            self.texture_create(BLACK, &desc);
            let whole = UploadRegion {
                level: 0,
                slice: 0,
                x: 0,
                y: 0,
                z: 0,
                width: 1,
                height: 1,
                depth: 1,
            };
            self.texture_upload(BLACK, whole, &BLACK_TEXEL, 4, 4);
        }
        self.texture_view(BLACK, WMT_PIXEL_FORMAT_BGRA8_UNORM)
            .expect("the black texture was just made")
    }

    /// The black texture's texel, BGRA.
    #[must_use]
    pub const fn black_texel(&self) -> [u8; 4] {
        BLACK_TEXEL
    }
}

#[cfg(test)]
mod tests;
