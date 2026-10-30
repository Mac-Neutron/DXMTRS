//! `MTLTextureDescriptor` from a `WMTTextureInfo`.

use objc2::runtime::{NSObject, NSObjectProtocol, Sel};
use objc2::{extern_class, extern_methods};
use objc2_foundation::NSInteger;
use objc2_metal::{MTLResourceOptions, MTLTextureDescriptor, MTLTextureType, MTLTextureUsage};
use winemetal_wire::{WMT_TEXTURE_FLAG_LOSSLESS, WMTTextureInfo};

use crate::ffi::AbortOnPanic;
use crate::float_cast::u64_to_usize;
use crate::format::pixel_format;

extern_class!(
    /// `MTLTextureDescriptor`, re-declared for its private `setCompressionMode:`.
    #[unsafe(super(NSObject))]
    #[name = "MTLTextureDescriptor"]
    struct PrivateTextureDescriptor;
);

impl PrivateTextureDescriptor {
    extern_methods!(
        /// `-setCompressionMode:`.
        ///
        /// 1 asks for Apple's lossless compression, which a pixel-format view usage does not
        /// turn off.
        #[unsafe(method(setCompressionMode:))]
        #[unsafe(method_family = none)]
        fn set_compression_mode(&self, mode: NSInteger);
    );
}

/// Fills `desc` from `info`, with the C's sends in the C's order.
///
/// A descriptor asked for lossless compression (`WMTTextureFlagLossless`) gets
/// `setCompressionMode:1` when it answers that selector.
pub fn fill_texture_descriptor(desc: &MTLTextureDescriptor, info: &WMTTextureInfo) {
    desc.setTextureType(MTLTextureType(info.r#type() as usize));
    desc.setPixelFormat(pixel_format(info.pixel_format));
    // SAFETY: the C's sends of the guest's values: Metal checks the descriptor when a texture is
    // made from it, and raises, as it did for the C.
    unsafe { desc.setWidth(info.width as usize) };
    // SAFETY: as above.
    unsafe { desc.setHeight(info.height as usize) };
    // SAFETY: as above.
    unsafe { desc.setDepth(info.depth as usize) };
    // SAFETY: as above.
    unsafe { desc.setArrayLength(info.array_length as usize) };
    // SAFETY: as above.
    unsafe { desc.setMipmapLevelCount(info.mipmap_level_count() as usize) };
    // SAFETY: as above.
    unsafe { desc.setSampleCount(info.sample_count() as usize) };
    desc.setUsage(MTLTextureUsage(info.usage() as usize));
    desc.setResourceOptions(MTLResourceOptions(u64_to_usize(info.options)));
    if info.flags & WMT_TEXTURE_FLAG_LOSSLESS != 0
        && desc.respondsToSelector(Sel::register(c"setCompressionMode:"))
    {
        // SAFETY: the same object under a second declaration of its own class.
        let private = unsafe { &*core::ptr::from_ref(desc).cast::<PrivateTextureDescriptor>() };
        private.set_compression_mode(1);
    }
}

/// `fill_texture_descriptor`, for the C handlers.
///
/// A null `desc` or `info` fills nothing (the C sent to nil, or read through null).
///
/// # Safety
///
/// `desc` is null or a live `MTLTextureDescriptor`, `info` null or a live `WMTTextureInfo`.
#[unsafe(export_name = "fill_texture_descriptor")]
pub unsafe extern "C-unwind" fn fill_texture_descriptor_c(
    desc: *mut MTLTextureDescriptor,
    info: *mut WMTTextureInfo,
) {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let desc = unsafe { desc.as_ref() };
    // SAFETY: the caller's contract.
    let info = unsafe { info.as_ref() };
    if let (Some(desc), Some(info)) = (desc, info) {
        fill_texture_descriptor(desc, info);
    }
}
