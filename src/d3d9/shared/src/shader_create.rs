//! Fixed-layout shader creation inputs and API metadata.
//!
//! Token and semantic buffers remain owned by the caller for the synchronous call.
//! Parsed programs stay in the native device until an ordered frame adopts them.

use bitflags::bitflags;

/// Shader stage requested by the API entry point.
#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex = 0,
    Pixel = 1,
}

pub use airconv_ir::dxso::DeclUsage;

/// A shader input declaration in its original declaration order.
#[repr(C)]
pub struct ShaderInputSemantic {
    pub usage: DeclUsage,
    pub usage_index: u8,
    pub register_index: u16,
}

bitflags! {
    /// Shader properties needed synchronously by API state capture.
    #[repr(transparent)]
    pub struct ShaderUsage: u8 {
        const RELATIVE_CONST = 1 << 0;
        const INT_CONST = 1 << 1;
        const BOOL_CONST = 1 << 2;
        const BUMP_ENV = 1 << 3;
        const AUTOMATIC_FOG = 1 << 4;
        /// A `ps_3_0` reads an input semantic outside the fixed-function varyings.
        ///
        /// Such a draw's pixel variant records which of those semantics the
        /// bound vertex shader outputs (`VariantKey::linked_input_mask`).
        const LINKED_INPUTS = 1 << 5;
    }
}

const _: () = {
    assert!(size_of::<ShaderInputSemantic>() == 4);
    assert!(align_of::<ShaderInputSemantic>() == 2);
};
