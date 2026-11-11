//! Direct binds by slot: D3D9's buffers, textures, samplers and constants, each a record.
//!
//! D3D11 binds through argument buffers its shaders' reflection lays out
//! ([`Encoder::encode_shader_resources`]); D3D9's shaders read Metal's own buffer, texture and
//! sampler slots. Each binder makes the access its slot's contents need (the pass waits for the
//! passes that wrote them and the chunk keeps them alive), then appends the matching
//! `SET_{VERTEX,FRAGMENT}_*` record to the render pass. A render pass records every bind it needs
//! itself: merged passes concatenate their records, so each keeps its binds before its draws.
//!
//! Free functions, not `Encoder` methods: [`Encoder::bind_sampler`] is the argument table's.

use std::sync::Arc;

use winemetal_sys::wire::{
    MemPtr, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
    WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_VERTEX_BYTES,
    WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE, WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE,
    WmtcmdRenderSetbuffer, WmtcmdRenderSetbytes, WmtcmdRenderSetsampler, WmtcmdRenderSettexture,
};

use super::{Encoder, stage};
use crate::{
    deptrack::Access,
    resource::{Buffer, Texture, TextureViewKey},
    sampler::Sampler,
};

/// The most bytes a set-bytes record carries: Metal's `set*Bytes` limit on every GPU family.
pub const SET_BYTES_MAX: usize = 4096;
/// Where a set-bytes record's bytes start, as the encoder aligns records.
const BYTES_ALIGN: usize = 16;

/// Stage `S`'s record types: buffer, bytes, texture, sampler.
const fn types<const S: u8>() -> [u16; 4] {
    const {
        assert!(
            S == stage::VERTEX || S == stage::PIXEL,
            "direct binds are the vertex and pixel stages'"
        );
    };
    if S == stage::VERTEX {
        [
            WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
            WMT_RENDER_COMMAND_SET_VERTEX_BYTES,
            WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE,
            WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
        ]
    } else {
        [
            WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
            WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE,
            WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
        ]
    }
}

/// Binds `buffer` at stage `S`'s buffer `index`, read from byte `offset` of its suballocation.
///
/// `None`, or a buffer without an allocation, unbinds the slot.
pub fn bind_buffer<const S: u8>(
    encoder: &mut Encoder,
    index: u8,
    buffer: Option<&Buffer>,
    offset: u64,
) {
    let current = buffer.and_then(|b| encoder.access_buffer::<S>(b, Access::READ));
    let (buffer, offset) = current.map_or((0, 0), |(allocation, at)| {
        (allocation.buffer(), at + offset)
    });
    encoder.encode_render_command(WmtcmdRenderSetbuffer {
        r#type: types::<S>()[0],
        reserved: [0; 3],
        next: MemPtr::NULL,
        buffer,
        offset,
        index,
    });
}

/// Binds `texture`'s view `key` at stage `S`'s texture `index`, the view's every level read.
///
/// Every slice of the view is read too. `None`, or a texture without an allocation, unbinds the
/// slot.
pub fn bind_texture<const S: u8>(
    encoder: &mut Encoder,
    index: u8,
    texture: Option<(&Texture, TextureViewKey)>,
) {
    let view = texture.and_then(|(t, key)| encoder.access_texture_view::<S>(t, key, Access::READ));
    encoder.encode_render_command(WmtcmdRenderSettexture {
        r#type: types::<S>()[2],
        reserved: [0; 3],
        next: MemPtr::NULL,
        texture: view.map_or(0, |v| v.texture),
        index,
    });
}

/// Binds `sampler` (its cube texture's state when `cube`) at stage `S`'s sampler `index`.
///
/// The sampler is kept alive until the chunk is retired. `None` unbinds the slot.
pub fn bind_sampler<const S: u8>(
    encoder: &mut Encoder,
    index: u8,
    sampler: Option<&Arc<Sampler>>,
    cube: bool,
) {
    let state = sampler.map_or(0, |s| {
        encoder.retained.push(Arc::clone(s) as Arc<dyn Send + Sync>);
        if cube {
            s.state_cube().handle()
        } else {
            s.state().handle()
        }
    });
    encoder.encode_render_command(WmtcmdRenderSetsampler {
        r#type: types::<S>()[3],
        reserved: [0; 3],
        next: MemPtr::NULL,
        sampler: state,
        index,
    });
}

/// Binds a copy of `bytes` at stage `S`'s buffer `index`.
///
/// Up to [`SET_BYTES_MAX`] bytes travel in the record (Metal's `set*Bytes`), copied into the
/// encoder's memory until the next chunk; more go to the argument-buffer ring, whose blocks live
/// until the chunk is retired, and are bound as a buffer.
pub fn bind_bytes<const S: u8>(encoder: &mut Encoder, index: u8, bytes: &[u8]) {
    let [buffer_type, bytes_type, ..] = types::<S>();
    if bytes.len() <= SET_BYTES_MAX {
        let copy = encoder.allocate_cpu(bytes.len(), BYTES_ALIGN);
        // SAFETY: `copy` is `bytes.len()` bytes of the encoder's memory, nobody else's.
        unsafe {
            copy.as_ptr()
                .copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        }
        encoder.encode_render_command(WmtcmdRenderSetbytes {
            r#type: bytes_type,
            reserved: [0; 3],
            next: MemPtr::NULL,
            bytes: MemPtr::from_ptr(copy.as_ptr()),
            length: bytes.len() as u64,
            index,
        });
    } else {
        let ring = encoder.allocate_argument_buffer(bytes.len());
        // SAFETY: the slice's mapping is `bytes.len()` bytes the pass alone uses.
        unsafe {
            ring.mapping()
                .as_ptr()
                .copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        }
        encoder.encode_render_command(WmtcmdRenderSetbuffer {
            r#type: buffer_type,
            reserved: [0; 3],
            next: MemPtr::NULL,
            buffer: ring.buffer(),
            offset: ring.offset(),
            index,
        });
    }
}

#[cfg(test)]
mod tests;
