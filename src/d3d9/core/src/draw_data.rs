//! Draw-side constants and parameter packing shared with the core runtime.

use crate::dxso::VariantKey;

/// Zero bytes fed to a stream the declaration reads but nothing is bound to.
///
/// Bound inline at the stream's slot under a `Constant` layout, so every
/// vertex and instance reads offset 0 and sees zeros, the value a D3D9 vertex
/// reads from an unbound stream. Sized to the `setVertexBytes` limit; a
/// declaration whose extent on one stream exceeds it is not something a
/// 16-element declaration can produce.
pub static NULL_STREAM_ZEROS: [u8; 4096] = [0; 4096];

/// Serialize the D3D9 alpha-reference float for upload to the PS slot-14 buffer.
///
/// Returns `(buffer, len)` with `len == 0` when alpha test is off. A fixed
/// buffer (not a `Vec`) so the per-dirty-draw build never touches the allocator.
#[must_use]
pub const fn build_alpha_ref_bytes(variant: VariantKey, alpha_ref: f32) -> ([u8; 4], usize) {
    if variant.alpha_func == 0 || variant.alpha_func == 8 {
        return ([0u8; 4], 0);
    }
    (alpha_ref.to_le_bytes(), 4)
}

#[cfg(test)]
mod tests;
