//! `ID3D12PipelineState` and `ID3D12PipelineLibrary1` (`d3d12_pipeline_*.cpp`).

pub mod compute;
pub mod graphics;
pub mod library;
pub mod record;

use core::{ffi::c_void, ptr};

use winemetal_sys::{
    NSObject_description, NSString_getCString, NSString_lengthOfBytesUsingEncoding, Obj, Pool,
    wire::WMTUTF8_STRING_ENCODING,
};

pub use self::{compute::Compute, graphics::Graphics};
use crate::{
    abi::{
        pipeline::{PipelineStateStreamDesc, ShaderBytecode},
        vtbl::ID3D12PipelineStateVtbl,
    },
    com::{E_INVALIDARG, Hresult, init_out_ptr},
    pipeline_stream::{ParsedStream, parse_pipeline_stream, stream_bytes},
    root_signature::object::cast_root_signature,
    shader_cache::Sha1Digest,
};

/// Resolved `ID3D12PipelineState` kind (`IsComputePipelineState`, `d3d12_device.hpp:244`).
#[derive(Clone, Copy)]
pub enum PipelineKind<'a> {
    Graphics(&'a Graphics),
    Compute(&'a Compute),
}

/// Casts a non-null `ID3D12PipelineState` pointer into [`PipelineKind`] if its vtable matches
///
/// [`Graphics`] or [`Compute`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_pipeline<'a>(raw: *mut c_void) -> Option<PipelineKind<'a>> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and its first pointer-sized word is its COM vtable pointer.
    let vtbl_ptr = unsafe { *raw.cast::<*const ID3D12PipelineStateVtbl>() };
    if ptr::eq(vtbl_ptr, &raw const graphics::VTBL) {
        // SAFETY: `vtbl_ptr` matches `graphics::VTBL`, so `raw` is a live `Graphics`.
        Some(PipelineKind::Graphics(unsafe { &*raw.cast::<Graphics>() }))
    } else if ptr::eq(vtbl_ptr, &raw const compute::VTBL) {
        // SAFETY: `vtbl_ptr` matches `compute::VTBL`, so `raw` is a live `Compute`.
        Some(PipelineKind::Compute(unsafe { &*raw.cast::<Compute>() }))
    } else {
        None
    }
}

pub(super) fn obj_description(object: &Obj) -> String {
    let _pool = Pool::new();
    // SAFETY: `object` is live; the description is an autoreleased string, valid under `_pool`.
    let text = unsafe { NSObject_description(object.handle()) };
    if text == 0 {
        return String::new();
    }
    // SAFETY: `text` is a live `NSString` handle.
    let length = unsafe { NSString_lengthOfBytesUsingEncoding(text, WMTUTF8_STRING_ENCODING) };
    let length = usize::try_from(length).unwrap_or(0);
    let mut buffer = vec![0u8; length + 1];
    // SAFETY: `buffer` has room for `length` bytes and the trailing NUL byte.
    unsafe {
        NSString_getCString(
            text,
            buffer.as_mut_ptr().cast(),
            u64::try_from(buffer.len()).unwrap_or(0),
            WMTUTF8_STRING_ENCODING,
        );
    }
    buffer.truncate(length);
    String::from_utf8_lossy(&buffer).into_owned()
}

pub(super) unsafe fn root_blob<'a>(root_signature: *mut c_void) -> record::Blob<'a> {
    if root_signature.is_null() {
        return record::Blob::NONE;
    }
    // SAFETY: `root_signature` is non-null.
    unsafe { cast_root_signature(root_signature) }.map_or(record::Blob::NONE, |rs| record::Blob {
        id: rs.sha1,
        data: Some(&rs.blob),
    })
}

pub(super) const unsafe fn shader_blob<'a>(
    id: Sha1Digest,
    bc: &ShaderBytecode,
) -> record::Blob<'a> {
    if bc.shader_bytecode.is_null() || bc.bytecode_length == 0 {
        record::Blob { id, data: None }
    } else {
        // SAFETY: `bc.shader_bytecode` is non-null and valid for `bc.bytecode_length` bytes.
        let slice = unsafe {
            core::slice::from_raw_parts(bc.shader_bytecode.cast::<u8>(), bc.bytecode_length)
        };
        record::Blob {
            id,
            data: Some(slice),
        }
    }
}

pub(super) unsafe fn parse_stream(desc: *const c_void) -> Result<ParsedStream, Hresult> {
    if desc.is_null() {
        return Err(E_INVALIDARG);
    }
    // SAFETY: `desc` is non-null and points to a `PipelineStateStreamDesc`.
    let stream_desc = unsafe { &*desc.cast::<PipelineStateStreamDesc>() };
    if stream_desc.pipeline_state_subobject_stream.is_null() && stream_desc.size_in_bytes != 0 {
        return Err(E_INVALIDARG);
    }
    // SAFETY: `stream_desc` was checked for non-null stream pointer when `size_in_bytes != 0`.
    let bytes = unsafe { stream_bytes(stream_desc) };
    parse_pipeline_stream(bytes, &mut |w| crate::warn!("{w}")).map_err(|e| {
        crate::err!("{e}");
        e.hresult()
    })
}

/// `ID3D12Device2::CreatePipelineState` (`d3d12_device.cpp:1503`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_pipeline_state(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePipelineState");
    // SAFETY: `pso` is a caller out-pointer or null.
    unsafe { init_out_ptr(pso) };
    // SAFETY: `desc` is validated by `parse_stream`.
    match unsafe { parse_stream(desc) } {
        Ok(parsed) if parsed.is_compute => {
            // SAFETY: `parsed.compute` is a valid `ComputePipelineStateDesc`.
            unsafe {
                compute::create_compute_pipeline_state(
                    this,
                    (&raw const parsed.compute).cast::<c_void>(),
                    riid,
                    pso,
                )
            }
        }
        Ok(parsed) => {
            // SAFETY: `parsed.graphics` is a valid `GraphicsPipelineStateDesc`.
            unsafe {
                graphics::create_graphics_pipeline_state(
                    this,
                    (&raw const parsed.graphics).cast::<c_void>(),
                    riid,
                    pso,
                )
            }
        }
        Err(hr) => hr,
    }
}

#[cfg(test)]
mod tests;

/// The magic in a cached pipeline-state blob this driver produced.
const CACHED_PSO_MAGIC: [u8; 8] = *b"DXMTCPS1";

/// The descriptor hash a cached blob carries, if it is one of ours and complete.
fn cached_desc_hash(blob: *const c_void, size: usize) -> Option<u64> {
    if blob.is_null() || size != 16 {
        return None;
    }
    // SAFETY: the caller promises `size` readable bytes at `blob`; `size` is 16.
    let bytes = unsafe { core::slice::from_raw_parts(blob.cast::<u8>(), 16) };
    if bytes[..8] != CACHED_PSO_MAGIC {
        return None;
    }
    let mut hash = [0u8; 8];
    hash.copy_from_slice(&bytes[8..]);
    Some(u64::from_ne_bytes(hash))
}

/// A cached pipeline-state blob carrying `hash`.
fn cached_pso_blob(hash: u64) -> Vec<u8> {
    let mut blob = Vec::with_capacity(16);
    blob.extend_from_slice(&CACHED_PSO_MAGIC);
    blob.extend_from_slice(&hash.to_ne_bytes());
    blob
}
