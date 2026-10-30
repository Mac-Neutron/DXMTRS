//! Texture and buffer access: a texture's properties, its bytes in and out, a buffer's contents.

use winemetal_wire::{
    MemPtr, UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret, UnixcallMtlbufferUpdatecontents,
    UnixcallMtltextureReplaceregion, WMTOrigin, WMTSize, slot,
};

use crate::transport::{call, low_half};

exports! {
    fn MTLTexture_pixelFormat(texture: u64) -> u32 {
        low_half(call(slot::MTL_TEXTURE_PIXEL_FORMAT, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret)
    }

    fn MTLTexture_width(texture: u64) -> u64 {
        call(slot::MTL_TEXTURE_WIDTH, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret
    }

    fn MTLTexture_height(texture: u64) -> u64 {
        call(slot::MTL_TEXTURE_HEIGHT, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret
    }

    fn MTLTexture_depth(texture: u64) -> u64 {
        call(slot::MTL_TEXTURE_DEPTH, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret
    }

    fn MTLTexture_arrayLength(texture: u64) -> u64 {
        call(slot::MTL_TEXTURE_ARRAY_LENGTH, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret
    }

    fn MTLTexture_mipmapLevelCount(texture: u64) -> u64 {
        call(slot::MTL_TEXTURE_MIPMAP_LEVEL_COUNT, |p: &mut UnixcallGenericObjUint64Ret| p.handle = texture).ret
    }

    fn MTLTexture_replaceRegion(
        texture: u64,
        origin: WMTOrigin,
        size: WMTSize,
        level: u64,
        slice: u64,
        data: MemPtr,
        bytes_per_row: u64,
        bytes_per_image: u64,
    ) {
        call(slot::MTL_TEXTURE_REPLACE_REGION, |p: &mut UnixcallMtltextureReplaceregion| {
            p.texture = texture;
            p.origin = origin;
            p.size = size;
            p.level = level;
            p.slice = slice;
            p.data = data;
            p.bytes_per_row = bytes_per_row;
            p.bytes_per_image = bytes_per_image;
        });
    }

    fn MTLBuffer_didModifyRange(buffer: u64, start: u64, length: u64) {
        // The C sends `length` in the field named `ret`.
        call(slot::MTL_BUFFER_DID_MODIFY_RANGE, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = buffer;
            p.arg = start;
            p.ret = length;
        });
    }

    fn MTLBuffer_updateContents(buffer: u64, offset: u64, data: MemPtr, length: u64) {
        call(slot::MTL_BUFFER_UPDATE_CONTENTS, |p: &mut UnixcallMtlbufferUpdatecontents| {
            p.buffer = buffer;
            p.offset = offset;
            p.data = data;
            p.length = length;
        });
    }

    fn MTLTexture_getBytes(
        texture: u64,
        origin: WMTOrigin,
        size: WMTSize,
        level: u64,
        slice: u64,
        data: MemPtr,
        bytes_per_row: u64,
        bytes_per_image: u64,
    ) {
        // replaceRegion's params, as the C.
        call(slot::MTL_TEXTURE_GET_BYTES, |p: &mut UnixcallMtltextureReplaceregion| {
            p.texture = texture;
            p.origin = origin;
            p.size = size;
            p.level = level;
            p.slice = slice;
            p.data = data;
            p.bytes_per_row = bytes_per_row;
            p.bytes_per_image = bytes_per_image;
        });
    }
}
