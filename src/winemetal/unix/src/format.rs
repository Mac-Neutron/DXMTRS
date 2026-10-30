//! `WMTPixelFormat` to `MTLPixelFormat`, and the custom swizzle a format carries in its high bits.

use objc2_metal::{MTLPixelFormat, MTLTextureSwizzle, MTLTextureSwizzleChannels};
use winemetal_wire::{WMTTextureSwizzleChannels, decode_format_component_swizzle, original_format};

/// The Metal pixel format of a `WMTPixelFormat`: `ORIGINAL_FORMAT`.
///
/// The format without the custom swizzle in its high bits.
#[must_use]
pub const fn pixel_format(format: u32) -> MTLPixelFormat {
    MTLPixelFormat(original_format(format) as usize)
}

/// The swizzle a texture view of `format` gets for the requested `channels`.
///
/// Each channel offset by the swizzle `format` carries: `DECODE_FORMAT_COMPONENT_SWIZZLE` on each
/// component.
#[must_use]
pub fn swizzle(format: u32, channels: WMTTextureSwizzleChannels) -> MTLTextureSwizzleChannels {
    let decode = |component, base: u8| {
        let [v, ..] =
            decode_format_component_swizzle(format, component, u32::from(base)).to_le_bytes();
        MTLTextureSwizzle(v)
    };
    MTLTextureSwizzleChannels {
        red: decode(0, channels.r),
        green: decode(1, channels.g),
        blue: decode(2, channels.b),
        alpha: decode(3, channels.a),
    }
}

#[cfg(test)]
mod tests;
