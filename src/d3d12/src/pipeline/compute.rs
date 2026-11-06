//! `MTLD3D12ComputePipelineState` (`d3d12_pipeline_compute.cpp`, `d3d12_device.hpp:276-283`).

use core::{
    ffi::c_void,
    hash::{Hash, Hasher},
    mem::zeroed,
    ptr,
};
use std::sync::Arc;

use dxmt_util::{com::ComObject, private_data::PrivateData, sync::lock};
use rustc_hash::FxHasher;
use winemetal_sys::{
    MTLDevice_newComputePipelineState, Obj,
    sm50::{
        MtlShaderReflection, SM50_BINDING_INDEX_ROOT_ARGUMENTS, SM50_BINDING_INDEX_STATIC_SAMPLERS,
        SM50_SHADER_COMMON, SM50_SHADER_METAL_310, SM50_SHADER_ROOT_SIGNATURE,
        Sm50ShaderCommonData, Sm50ShaderRootSignatureData,
    },
};
use winemetal_wire::{MemPtr, WMTComputePipelineInfo, WMTSize};

use crate::{
    abi::{
        pipeline::ComputePipelineStateDesc,
        vtbl::{ID3D12PipelineStateVtbl, IUnknownVtbl},
    },
    blob::create_blob,
    capture::{
        captured_root_signature, captured_shader, dump_dxil, dxil_capture_mode, log_pipeline,
    },
    com::{
        ComRefCounted, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult,
        IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE, IID_ID3D12_PIPELINE_STATE,
        IID_IUNKNOWN, S_OK, add_ref_com, init_out_ptr, release_com, set_name_private_data, vtbl,
    },
    device::Device,
    pipeline::{
        obj_description,
        record::{record_compute, recording_on},
        root_blob, shader_blob,
    },
    root_signature::object::cast_root_signature,
    shader_cache::{CachedShader, CompileTarget, FunctionKind, Sha1Digest, compile_function},
};

/// `MTLD3D12ComputePipelineState` (`d3d12_device.hpp:276-283`).
#[repr(C)]
pub struct Compute {
    pub vtbl: &'static ID3D12PipelineStateVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub is_compute: bool,
    pub desc_hash: u64,
    pub pso: Option<Obj>,
    pub threadgroup_size: WMTSize,
}

impl ComRefCounted for Compute {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Compute {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was `add_ref_com`'d when `self` was constructed.
            unsafe { release_com(self.device) };
        }
    }
}

pub(super) static VTBL: ID3D12PipelineStateVtbl = ID3D12PipelineStateVtbl {
    query_interface: crate::slot!(
        "computepso.QueryInterface",
        (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
            if object.is_null() {
                return E_POINTER;
            }
            // SAFETY: `object` is non-null.
            unsafe { init_out_ptr(object) };
            // SAFETY: `riid` is null or points to a `Guid`.
            let Some(guid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            if matches!(
                guid,
                IID_IUNKNOWN
                    | IID_ID3D12_OBJECT
                    | IID_ID3D12_DEVICE_CHILD
                    | IID_ID3D12_PAGEABLE
                    | IID_ID3D12_PIPELINE_STATE
            ) {
                // SAFETY: `this` is a live `Compute`.
                unsafe { add_ref_com(this.cast::<Compute>()) };
                // SAFETY: `object` is non-null and writable.
                unsafe { object.write_unaligned(this) };
                S_OK
            } else {
                crate::warn!("D3D12ComputePipelineState: Unknown interface query {guid}");
                E_NOINTERFACE
            }
        }
    ),
    add_ref: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Compute`.
        unsafe { add_ref_com(this.cast::<Compute>()) }
    }),
    release: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Compute`.
        unsafe { release_com(this.cast::<Compute>()) }
    }),
    get_private_data: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `Compute`.
            let pso = unsafe { &*this.cast::<Compute>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.get_data(guid, data_size, data) }
        }
    ),
    set_private_data: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: `this` is a live `Compute`.
            let pso = unsafe { &*this.cast::<Compute>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.set_data(guid, data_size, data) }
        }
    ),
    set_private_data_interface: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: `this` is a live `Compute`.
            let pso = unsafe { &*this.cast::<Compute>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.set_interface(guid, data.cast_mut()) }
        }
    ),
    set_name: crate::slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: `this` is a live `Compute`.
        let pso = unsafe { &*this.cast::<Compute>() };
        // SAFETY: Forwarded from COM slot.
        unsafe { set_name_private_data(&pso.private_data, name.cast::<u16>()) }
    }),
    get_device: crate::slot!(
        (this: *mut c_void, riid: *const c_void, pp_device: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `Compute`.
            let pso = unsafe { &*this.cast::<Compute>() };
            if pso.device.is_null() {
                // SAFETY: `pp_device` is a caller out-pointer.
                unsafe { init_out_ptr(pp_device) };
                return E_NOINTERFACE;
            }
            let dev_ptr = pso.device.cast::<c_void>();
            // SAFETY: `dev_ptr` is a live `Device` COM object.
            let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
            // SAFETY: Forwarded to `Device::QueryInterface`.
            unsafe { (unk.query_interface)(dev_ptr, riid, pp_device) }
        }
    ),
    get_cached_blob: crate::slot!(
        "computepso.GetCachedBlob",
        (_this: *mut c_void, pp_blob: *mut *mut c_void) -> Hresult => {
            if pp_blob.is_null() {
                return E_POINTER;
            }
            // SAFETY: `_this` is a live `Compute` PSO.
            let pso = unsafe { &*_this.cast::<Compute>() };
            // SAFETY: `pp_blob` is non-null.
            unsafe { create_blob(super::cached_pso_blob(pso.desc_hash), pp_blob) }
        }
    ),
};

/// Computes the build-local description hash for a `D3D12_COMPUTE_PIPELINE_STATE_DESC`
///
/// (`HashComputeDesc`, `d3d12_pipeline_library.cpp:78`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
#[must_use]
pub unsafe fn hash_compute_desc(desc: *const c_void) -> u64 {
    // SAFETY: Caller guarantees `desc` points to a valid `ComputePipelineStateDesc`.
    let d = unsafe { &*desc.cast::<ComputePipelineStateDesc>() };
    let mut hasher = FxHasher::default();
    if !d.root_signature.is_null() {
        // SAFETY: `d.root_signature` is non-null.
        if let Some(rs) = unsafe { cast_root_signature(d.root_signature) } {
            (rs.blob.len() as u64).hash(&mut hasher);
            rs.blob.hash(&mut hasher);
        }
    }
    (d.cs.bytecode_length as u64).hash(&mut hasher);
    if !d.cs.shader_bytecode.is_null() && d.cs.bytecode_length > 0 {
        // SAFETY: `d.cs.shader_bytecode` is non-null and valid for `d.cs.bytecode_length` bytes.
        let cs = unsafe {
            core::slice::from_raw_parts(d.cs.shader_bytecode.cast::<u8>(), d.cs.bytecode_length)
        };
        cs.hash(&mut hasher);
    }
    d.node_mask.hash(&mut hasher);
    d.flags.hash(&mut hasher);
    hasher.finish() | 1
}

unsafe fn initialize_compute(
    dev: &Device,
    desc: &ComputePipelineStateDesc,
) -> Result<(Obj, WMTSize, Sha1Digest), Hresult> {
    if desc.cs.shader_bytecode.is_null() || desc.cs.bytecode_length == 0 {
        return Err(E_INVALIDARG);
    }
    // SAFETY: `desc.cs.shader_bytecode` is non-null and valid for `desc.cs.bytecode_length` bytes.
    let cs_slice = unsafe {
        core::slice::from_raw_parts(
            desc.cs.shader_bytecode.cast::<u8>(),
            desc.cs.bytecode_length,
        )
    };
    dump_dxil(cs_slice);

    let rs_blob: Option<&[u8]> = if desc.root_signature.is_null() {
        None
    } else {
        // SAFETY: `desc.root_signature` is non-null.
        unsafe { cast_root_signature(desc.root_signature) }.map(|rs| &*rs.blob)
    };
    if dxil_capture_mode() {
        log_pipeline(&format!(
            "cs{}{}",
            captured_shader("cs", cs_slice),
            captured_root_signature(rs_blob)
        ));
    }

    let (rs_ptr, rs_len) = rs_blob
        .map_or((desc.cs.shader_bytecode, desc.cs.bytecode_length), |b| {
            (b.as_ptr().cast::<c_void>(), b.len())
        });
    let mut rootsig = Sm50ShaderRootSignatureData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: rs_ptr,
        bytecode_length: rs_len,
    };
    let mut common = Sm50ShaderCommonData {
        next: (&raw mut rootsig).cast(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };

    let mut shader_cs = CachedShader::new(cs_slice)?;
    let Some(cs_mut) = Arc::get_mut(&mut shader_cs) else {
        return Err(E_FAIL);
    };
    // SAFETY: `MtlShaderReflection` is an all-zero-valid POD C struct.
    let mut ref_cs = unsafe { zeroed::<MtlShaderReflection>() };
    let hr = cs_mut.reflection(&mut ref_cs);
    if hr < 0 {
        return Err(hr);
    }
    // SAFETY: Compute shader reflection populates the `threadgroup_size` union arm.
    let tg = unsafe { ref_cs.u.threadgroup_size };
    let threadgroup_size = WMTSize {
        width: u64::from(tg[0]),
        height: u64::from(tg[1]),
        depth: u64::from(tg[2]),
    };

    let Some(metal) = dev.mtl.as_ref() else {
        return Err(E_FAIL);
    };
    // SAFETY: `common` is a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
    let cs_func = unsafe {
        compile_function(
            metal,
            &dev.shader_functions,
            FunctionKind::Shader,
            cs_mut,
            None,
            (&raw mut common).cast(),
            &CompileTarget {
                name: "cs_main",
                stage: "cs",
            },
        )
    }?;
    let digest_cs = *cs_mut.digest();

    let total_tg = threadgroup_size
        .width
        .saturating_mul(threadgroup_size.height)
        .saturating_mul(threadgroup_size.depth);
    let info = WMTComputePipelineInfo {
        compute_function: cs_func.handle(),
        binary_archives_for_lookup: MemPtr::NULL,
        binary_archive_for_serialization: 0,
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 1,
        tgsize_is_multiple_of_sgwidth: u8::from(total_tg != 0 && total_tg.is_multiple_of(32)),
        immutable_buffers: (1 << SM50_BINDING_INDEX_ROOT_ARGUMENTS)
            | (1 << SM50_BINDING_INDEX_STATIC_SAMPLERS),
    };
    let mut err_handle = 0u64;
    // SAFETY: `metal` is a live `MTLDevice`; `info` and `err_handle` are valid for the call.
    let pso_handle = unsafe {
        MTLDevice_newComputePipelineState(metal.handle(), &raw const info, &raw mut err_handle)
    };
    // SAFETY: `err_handle` is 0 or a newly retained `NSError` handle.
    let err_obj = unsafe { Obj::adopt(err_handle) };
    // SAFETY: `pso_handle` is 0 or a newly retained `MTLComputePipelineState` handle.
    let Some(pso) = (unsafe { Obj::adopt(pso_handle) }) else {
        let msg = err_obj.as_ref().map_or_else(String::new, obj_description);
        crate::err!("Failed to create compute PSO: {msg}");
        return Err(E_FAIL);
    };
    Ok((pso, threadgroup_size, digest_cs))
}

/// `ID3D12Device::CreateComputePipelineState` (`d3d12_device.cpp:638`,
///
/// `d3d12_pipeline_compute.cpp:140`).
///
/// A Metal compute pipeline shared by PSOs with the same descriptor.
pub struct CachedComputePso {
    pub pso: Obj,
    pub threadgroup_size: WMTSize,
    pub digest: Sha1Digest,
}

/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_compute_pipeline_state(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateComputePipelineState");
    // SAFETY: `pso` is a caller-supplied out-pointer or null.
    unsafe { init_out_ptr(pso) };
    if this.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    let device_ptr = this.cast::<Device>();
    // SAFETY: `device_ptr` is non-null and points to a live `Device`.
    let dev = unsafe { &*device_ptr };
    // SAFETY: `desc` is non-null and points to a `ComputePipelineStateDesc`.
    let d = unsafe { &*desc.cast::<ComputePipelineStateDesc>() };
    let desc_hash = super::cached_desc_hash(
        d.cached_pso.cached_blob,
        d.cached_pso.cached_blob_size_in_bytes,
    )
    // SAFETY: `desc` is valid.
    .unwrap_or_else(|| unsafe { hash_compute_desc(desc) });
    let cached = {
        let cache = lock(&dev.compute_psos);
        cache.get(&desc_hash).map(|cached| {
            // SAFETY: the cached `Obj` holds a live Metal pipeline.
            let pso = unsafe { Obj::retain(cached.pso.handle()) };
            (pso, cached.threadgroup_size, cached.digest)
        })
    };
    let (mtl_pso, threadgroup_size, digest_cs) = if let Some((Some(pso), tg, digest)) = cached {
        (pso, tg, digest)
    } else {
        // SAFETY: `dev` and `d` are valid.
        let (pso, tg, digest) = match unsafe { initialize_compute(dev, d) } {
            Ok(res) => res,
            Err(hr) => return hr,
        };
        // SAFETY: `pso` is live; the cache takes a reference of its own.
        if let Some(cached_pso) = unsafe { Obj::retain(pso.handle()) } {
            lock(&dev.compute_psos).insert(
                desc_hash,
                CachedComputePso {
                    pso: cached_pso,
                    threadgroup_size: tg,
                    digest,
                },
            );
        }
        (pso, tg, digest)
    };
    if recording_on() {
        // SAFETY: `d.root_signature` is null or a valid `ID3D12RootSignature`.
        let root_rec = unsafe { root_blob(d.root_signature) };
        // SAFETY: `d.cs` was validated in `initialize_compute`.
        let compute_rec = unsafe { shader_blob(digest_cs, &d.cs) };
        // SAFETY: `desc` is valid.
        unsafe { record_compute(desc, root_rec, compute_rec) };
    }
    // SAFETY: `device_ptr` is a live `Device`.
    unsafe { add_ref_com(device_ptr) };
    let raw = Box::into_raw(Box::new(Compute {
        vtbl: &VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: device_ptr,
        is_compute: true,
        desc_hash,
        pso: Some(mtl_pso),
        threadgroup_size,
    }));
    // SAFETY: `raw` is a freshly allocated `Compute` with 1 initial public ref.
    let hr = unsafe { (VTBL.query_interface)(raw.cast::<c_void>(), riid, pso) };
    // SAFETY: Release the initial ref from `ComObject::new()`.
    unsafe { release_com(raw) };
    hr
}
